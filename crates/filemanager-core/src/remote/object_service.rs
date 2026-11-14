//! S3 account-level discovery. Object operations remain in OpenDAL.
use super::{
    Entry, MAX_ENTRIES, check_cancel, deadline,
    object::http_base,
    profile::{Config, Provider, Secret, bucket_name, valid_region},
};
use crate::Result;
use http::{Method, Request};
use reqsign_aws_v4::{RequestSigner, StaticCredentialProvider};
use reqsign_core::{Context, Signer};
use serde::Deserialize;
use std::collections::{HashSet, VecDeque};
use url::Url;

const DEFAULT_REGION: &str = "us-east-1";
const MAX_XML: usize = 4 * 1024 * 1024;

fn settings(config: &Config) -> Result<(&str, &str, &str, bool)> {
    match &config.provider {
        Provider::Minio {
            bucket,
            region,
            user,
        } => Ok((bucket, region, user, false)),
        Provider::S3 {
            bucket,
            region,
            access_key,
            virtual_host,
        } => Ok((bucket, region, access_key, *virtual_host)),
        _ => Err("此协议不支持列出所有 Bucket".into()),
    }
}

fn endpoint(config: &Config, bucket: Option<&str>) -> Result<Url> {
    let mut url = config.origin()?;
    if let Some(bucket) = bucket {
        if settings(config)?.3 {
            let host = format!("{bucket}.{}", url.host_str().ok_or("服务器地址无效")?);
            url.set_host(Some(&host)).map_err(|_| "服务器地址无效")?;
        } else {
            url.path_segments_mut()
                .map_err(|_| "服务器地址无效")?
                .pop_if_empty()
                .push(bucket);
        }
    }
    Ok(url)
}

struct Reply {
    status: u16,
    region: Option<String>,
    body: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ServiceError {
    #[serde(default)]
    code: String,
    region: Option<String>,
}

#[derive(Deserialize)]
struct LocationConstraint {
    #[serde(rename = "$text", default)]
    region: String,
}

async fn get(config: &Config, secret: &Secret, url: &Url, region: &str) -> Result<Reply> {
    let user = settings(config)?.2;
    deadline(async {
        let mut provider = StaticCredentialProvider::new(user, &secret.value);
        if !secret.token.is_empty() {
            provider = provider.with_session_token(&secret.token);
        }
        let signer = Signer::new(Context::new(), provider, RequestSigner::new("s3", region));
        let (mut parts, _) = Request::builder()
            .method(Method::GET)
            .uri(url.as_str())
            .body(())
            .map_err(|_| "对象存储请求地址无效")?
            .into_parts();
        signer
            .sign(&mut parts, None)
            .await
            .map_err(|_| "对象存储签名失败")?;
        for header in ["authorization", "x-amz-security-token"] {
            if let Some(value) = parts.headers.get_mut(header) {
                value.set_sensitive(true);
            }
        }
        let mut response = http_base()?
            .get(url.clone())
            .headers(parts.headers)
            .send()
            .await
            .map_err(|_| "对象存储请求失败，请检查 API 地址、网络和 TLS 证书")?;
        let status = response.status().as_u16();
        let region = response
            .headers()
            .get("x-amz-bucket-region")
            .and_then(|v| v.to_str().ok())
            .filter(|r| !r.is_empty() && valid_region(r))
            .map(str::to_owned);
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "对象存储响应读取失败")?
        {
            check_cancel()?;
            if body.len() + chunk.len() > MAX_XML {
                return Err("Bucket 响应过大，请指定 Bucket 连接".into());
            }
            body.extend_from_slice(&chunk);
        }
        let body = String::from_utf8(body)
            .map_err(|_| "服务器没有返回有效的 S3 XML，请使用 MinIO API 端口")?;
        Ok(Reply {
            status,
            region,
            body,
        })
    })
    .await
}

fn hint(reply: &Reply) -> Option<String> {
    reply.region.clone().or_else(|| {
        quick_xml::de::from_str::<ServiceError>(&reply.body)
            .ok()?
            .region
            .filter(|r| !r.is_empty() && valid_region(r))
    })
}

fn failure(reply: &Reply, buckets: bool) -> String {
    let code = quick_xml::de::from_str::<ServiceError>(&reply.body)
        .unwrap_or_default()
        .code;
    match code.as_str() {
        "InvalidAccessKeyId" | "SignatureDoesNotMatch" | "InvalidToken" | "ExpiredToken" => {
            "登录失败，请检查 MinIO 账号密码或 Access Key、会话令牌".into()
        }
        "AccessDenied" if buckets => "账号没有列桶权限；可填写已授权的 Bucket 直接打开".into(),
        "AccessDenied" => "账号没有访问此 Bucket 的权限".into(),
        "NoSuchBucket" => "Bucket 不存在".into(),
        "AuthorizationHeaderMalformed" | "InvalidRegion" => {
            "无法自动识别 Region，请手动填写服务器配置的 Region".into()
        }
        _ if reply.status == 401 || reply.status == 403 => {
            "登录或访问被拒绝，请检查账号密码和权限".into()
        }
        _ => "服务器没有返回有效的 S3 响应，请检查 MinIO API 地址和端口".into(),
    }
}

// Region is optional in the profile. Probe on the same validated endpoint and
// retry only with a validated signing-region hint, never a redirect URL.
pub(super) async fn region(config: &Config, secret: &Secret) -> Result<String> {
    let (bucket, configured, _, _) = settings(config)?;
    if !configured.is_empty() {
        return Ok(configured.into());
    }
    let mut url = endpoint(config, Some(bucket))?;
    url.set_query(Some("location"));
    let mut reply = get(config, secret, &url, DEFAULT_REGION).await?;
    let detected = hint(&reply);
    if !(200..300).contains(&reply.status) {
        if let Some(region) = detected.as_deref().filter(|r| *r != DEFAULT_REGION) {
            reply = get(config, secret, &url, region).await?;
        }
    }
    if (200..300).contains(&reply.status) {
        let location: LocationConstraint = quick_xml::de::from_str(&reply.body)
            .map_err(|_| "无法读取 Bucket Region，请检查 S3 API 地址")?;
        let region = match location.region.trim() {
            "" => DEFAULT_REGION,
            "EU" => "eu-west-1",
            r => r,
        };
        if !valid_region(region) {
            return Err("服务器返回的 Region 无效".into());
        }
        return Ok(region.into());
    }
    // GetBucketLocation is a separate permission. A restricted account may
    // still list/read objects, so let that operation enforce its own policy.
    if reply.status == 403 {
        return Ok(hint(&reply)
            .or(detected)
            .unwrap_or_else(|| DEFAULT_REGION.into()));
    }
    Err(failure(&reply, false))
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Bucket {
    name: String,
    creation_date: Option<String>,
}
#[derive(Deserialize)]
struct BucketItems {
    #[serde(rename = "Bucket", default)]
    entries: Vec<Bucket>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BucketPage {
    buckets: BucketItems,
    continuation_token: Option<String>,
}

pub(super) struct Buckets {
    config: Config,
    secret: Secret,
    entries: VecDeque<Entry>,
    next: Option<String>,
    seen_tokens: HashSet<String>,
    seen_names: HashSet<String>,
    region: String,
}
impl Buckets {
    pub async fn open(config: Config, secret: Secret) -> Result<Self> {
        let region = match settings(&config)?.1 {
            "" => DEFAULT_REGION,
            r => r,
        }
        .to_owned();
        let mut reader = Self {
            config,
            secret,
            entries: VecDeque::new(),
            next: None,
            seen_tokens: HashSet::new(),
            seen_names: HashSet::new(),
            region,
        };
        reader.fetch().await?;
        Ok(reader)
    }

    async fn fetch(&mut self) -> Result<()> {
        let mut url = endpoint(&self.config, None)?;
        url.query_pairs_mut().append_pair("max-buckets", "1000");
        if let Some(token) = &self.next {
            url.query_pairs_mut()
                .append_pair("continuation-token", token);
        }
        let mut reply = get(&self.config, &self.secret, &url, &self.region).await?;
        if !(200..300).contains(&reply.status) && settings(&self.config)?.1.is_empty() {
            if let Some(region) = hint(&reply).filter(|r| r != &self.region) {
                self.region = region;
                reply = get(&self.config, &self.secret, &url, &self.region).await?;
            }
        }
        if !(200..300).contains(&reply.status) {
            return Err(failure(&reply, true));
        }
        let page: BucketPage = quick_xml::de::from_str(&reply.body)
            .map_err(|_| "服务器没有返回 Bucket 列表，请使用 MinIO API 端口而非控制台端口")?;
        self.next = page.continuation_token.filter(|t| !t.is_empty());
        if let Some(token) = &self.next {
            if token.len() > 4096
                || self.seen_tokens.len() >= 1000
                || !self.seen_tokens.insert(token.clone())
            {
                return Err("服务器返回了无效的 Bucket 分页令牌".into());
            }
        }
        for b in page.buckets.entries {
            if !bucket_name(&b.name) || !self.seen_names.insert(b.name.clone()) {
                return Err("服务器返回了无效或重复的 Bucket 名称".into());
            }
            if self.seen_names.len() > MAX_ENTRIES {
                return Err("Bucket 数量超过上限，请指定 Bucket 连接".into());
            }
            self.entries.push_back(Entry {
                path: format!("{}/", b.name),
                name: b.name,
                is_dir: true,
                size: None,
                modified: b
                    .creation_date
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|t| t.timestamp_millis()),
            });
        }
        Ok(())
    }

    pub async fn next(&mut self) -> Result<Option<super::pagination::Step>> {
        loop {
            check_cancel()?;
            if let Some(entry) = self.entries.pop_front() {
                return Ok(Some(super::pagination::Step::Entry(entry)));
            }
            if self.next.is_none() {
                return Ok(None);
            }
            self.fetch().await?;
        }
    }
}
