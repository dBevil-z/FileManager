use super::{
    Listing, TIMEOUT, check_cancel, check_size, cleanup_message,
    profile::{Config, Provider, Secret},
    progress, source_unchanged, tls_config,
};
use crate::{Result, err};
use futures_util::TryStreamExt;
use http::{Request, Response};
use opendal::raw::{OpWrite, oio::Write as _};
use opendal::{
    Buffer, Error, ErrorKind, HttpBody, HttpTransport, HttpTransporter, OperationContext, Operator,
    services,
};
use opendal_http_transport_reqwest::ReqwestTransport;
use reqsign_aliyun_oss::{Credential, RequestSigner, SigningVersion, StaticCredentialProvider};
use reqsign_core::{Context, Signer};
use std::{
    fs::{File, Metadata},
    io::{Read, Write},
    sync::OnceLock,
};

const PART: usize = 8 * 1024 * 1024;
const MAX_UPLOAD: u64 = PART as u64 * 10_000;

fn message(e: Error) -> String {
    // Backend errors may contain HTTP headers, response bodies or server-supplied text.
    let message = match e.kind() {
        ErrorKind::NotFound => "远程文件、目录或 Bucket 不存在",
        ErrorKind::PermissionDenied => "远程服务拒绝访问，请检查凭据、Region 和读写权限",
        ErrorKind::ConditionNotMatch | ErrorKind::AlreadyExists => "远程文件已存在或已更改，未覆盖",
        ErrorKind::ConfigInvalid => "对象存储配置无效，请检查地址、Region 和 Bucket",
        ErrorKind::Unsupported => "此对象存储服务不支持当前操作",
        _ if e.message().contains("timeout") => "远程操作超时，请检查网络和服务器",
        _ => "对象存储请求失败，请检查地址、TLS 证书、Region 和服务状态",
    };
    message.into()
}

pub(super) struct Reader {
    lister: opendal::Lister,
    path: String,
}

impl Reader {
    pub async fn open(op: Operator, path: &str) -> Result<Self> {
        let lister = op.lister_with(path).limit(500).await.map_err(message)?;
        Ok(Self {
            lister,
            path: path.into(),
        })
    }

    pub async fn next(&mut self) -> Result<Option<super::pagination::Step>> {
        super::deadline(async {
            loop {
                let Some(e) = self.lister.try_next().await.map_err(message)? else {
                    return Ok(None);
                };
                if e.path() == self.path || (self.path.is_empty() && e.path() == "/") {
                    continue;
                }
                let meta = e.metadata();
                let relative = e.path().strip_prefix(&self.path).and_then(|p| {
                    if meta.is_dir() {
                        p.strip_suffix('/')
                    } else {
                        Some(p)
                    }
                });
                return Ok(Some(match relative {
                    Some(name) => super::pagination::entry(
                        &self.path,
                        name,
                        meta.is_dir(),
                        Some(meta.content_length()),
                        meta.last_modified()
                            .map(|m| m.into_inner().as_millisecond()),
                    ),
                    None => super::pagination::Step::Skipped,
                }));
            }
        })
        .await
    }
}

pub(super) async fn ensure_dir(op: &Operator, path: &str) -> Result<()> {
    // Check both a file key and its directory prefix; object stores allow both.
    match op.stat(path.trim_end_matches('/')).await {
        Ok(meta) if !meta.is_dir() => return Err("远程已有同名文件，不能创建文件夹".into()),
        Ok(_) => return Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => (),
        Err(e) => return Err(message(e)),
    }
    let mut entries = op.lister_with(path).limit(1).await.map_err(message)?;
    if entries.try_next().await.map_err(message)?.is_some() {
        return Ok(());
    }
    mkdir(op, path).await
}

pub(super) fn http_base() -> Result<remote_http::Client> {
    static CLIENT: OnceLock<remote_http::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let client = remote_http::Client::builder()
        .tls_backend_preconfigured(tls_config()?)
        .redirect(remote_http::redirect::Policy::none())
        .connect_timeout(TIMEOUT)
        .read_timeout(TIMEOUT)
        .build()
        .map_err(|_| "无法初始化对象存储 HTTP 客户端")?;
    Ok(CLIENT.get_or_init(|| client).clone())
}

struct OssV4<T> {
    transport: T,
    signer: Signer<Credential>,
}

impl<T: HttpTransport> HttpTransport for OssV4<T> {
    async fn fetch(&self, req: Request<Buffer>) -> opendal::Result<Response<HttpBody>> {
        let (mut parts, body) = req.into_parts();
        self.signer
            .sign(&mut parts, None)
            .await
            .map_err(|_| Error::new(ErrorKind::ConfigInvalid, "OSS V4 signing failed"))?;
        for header in ["authorization", "x-oss-security-token"] {
            if let Some(value) = parts.headers.get_mut(header) {
                value.set_sensitive(true);
            }
        }
        self.transport.fetch(Request::from_parts(parts, body)).await
    }
}

pub(super) fn operator(config: &Config, secret: &Secret) -> Result<Operator> {
    operator_with_transport(config, secret, ReqwestTransport::from(http_base()?))
}

pub(super) async fn connect(config: &Config, secret: &Secret) -> Result<Operator> {
    let mut config = config.clone();
    if let Provider::Minio { region, .. } | Provider::S3 { region, .. } = &config.provider {
        if region.is_empty() {
            let detected = super::object_service::region(&config, secret).await?;
            match &mut config.provider {
                Provider::Minio { region, .. } | Provider::S3 { region, .. } => *region = detected,
                _ => unreachable!(),
            }
        }
    }
    operator(&config, secret)
}

fn operator_with_transport(
    config: &Config,
    secret: &Secret,
    client: impl HttpTransport + 'static,
) -> Result<Operator> {
    let minio;
    let provider = if let Provider::Minio {
        bucket,
        region,
        user,
    } = &config.provider
    {
        minio = Provider::S3 {
            bucket: bucket.clone(),
            region: region.clone(),
            access_key: user.clone(),
            virtual_host: false,
        };
        &minio
    } else {
        &config.provider
    };
    let (op, transport) = match provider {
        Provider::S3 {
            bucket,
            region,
            access_key,
            virtual_host,
        } => {
            let mut s3 = services::S3::default()
                .bucket(bucket)
                .region(region)
                .endpoint(&config.endpoint)
                .root(&config.root)
                .disable_config_load()
                .disable_ec2_metadata()
                .access_key_id(access_key)
                .secret_access_key(&secret.value);
            if !secret.token.is_empty() {
                s3 = s3.session_token(&secret.token);
            }
            if *virtual_host {
                s3 = s3.enable_virtual_host_style();
            }
            (
                Operator::new(s3).map_err(message)?,
                HttpTransporter::new(client),
            )
        }
        Provider::Oss {
            bucket,
            region,
            access_key,
        } => {
            // OpenDAL's OSS builder still defaults to V1, which new buckets reject.
            // Keep its object operations and sign the outgoing requests with reqsign V4.
            let oss = services::Oss::default()
                .bucket(bucket)
                .endpoint(&config.endpoint)
                .root(&config.root)
                .skip_signature();
            let mut provider = StaticCredentialProvider::new(access_key, &secret.value);
            if !secret.token.is_empty() {
                provider = provider.with_security_token(&secret.token);
            }
            let signer = Signer::new(
                Context::new(),
                provider,
                RequestSigner::new(bucket)
                    .with_region(region)
                    .with_signing_version(SigningVersion::V4),
            );
            (
                Operator::new(oss).map_err(message)?,
                HttpTransporter::new(OssV4 {
                    transport: client,
                    signer,
                }),
            )
        }
        _ => return Err("无效的对象存储协议".into()),
    };
    Ok(op
        .with_context(OperationContext::new().with_http_transport(transport))
        .layer(
            opendal::layers::TimeoutLayer::default()
                .with_timeout(TIMEOUT)
                .with_io_timeout(TIMEOUT),
        ))
}

pub(super) async fn list(op: &Operator, dir: &str, limit: usize) -> Result<Listing> {
    let mut lister = op
        .lister_with(dir)
        .limit(limit.min(500))
        .await
        .map_err(message)?;
    let mut listing = Listing::default();
    let mut seen = 0;
    super::deadline(async {
        while let Some(entry) = lister.try_next().await.map_err(message)? {
            if entry.path() == dir || (dir.is_empty() && entry.path() == "/") {
                continue;
            }
            if seen == limit {
                listing.truncated = true;
                break;
            }
            seen += 1;
            let meta = entry.metadata();
            let is_dir = meta.is_dir();
            let relative = entry
                .path()
                .strip_prefix(dir)
                .and_then(|p| if is_dir { p.strip_suffix('/') } else { Some(p) });
            if let Some(name) = relative {
                listing.push(
                    dir,
                    name,
                    is_dir,
                    Some(meta.content_length()),
                    meta.last_modified()
                        .map(|m| m.into_inner().as_millisecond()),
                );
            } else {
                listing.skipped += 1;
            }
            if limit == 1 {
                break;
            }
        }
        Ok(listing)
    })
    .await
}

pub(super) async fn mkdir(op: &Operator, path: &str) -> Result<()> {
    let mut existing = op.lister_with(path).limit(1).await.map_err(message)?;
    if existing.try_next().await.map_err(message)?.is_some() {
        return Err("远程已有同名文件夹，未覆盖".into());
    }
    // The high-level create_dir uses an unconditional zero-byte PUT. Use the
    // backend's conditional writer for the marker to preserve an existing key.
    let (args, _) = OpWrite::from_options(
        &op.info().capability(),
        opendal::options::WriteOptions {
            if_not_exists: true,
            ..Default::default()
        },
    )
    .map_err(message)?;
    let mut writer = op
        .service()
        .write_dyn(op.context(), path, args)
        .map_err(message)?;
    writer.close().await.map_err(message)?;
    Ok(())
}

pub(super) async fn delete(op: &Operator, path: &str, is_dir: bool) -> Result<()> {
    if is_dir {
        let mut items = op.lister_with(path).limit(2).await.map_err(message)?;
        super::deadline(async {
            while let Some(entry) = items.try_next().await.map_err(message)? {
                if entry.path() != path {
                    return Err("只能删除空的远程文件夹".into());
                }
            }
            Ok(())
        })
        .await?;
    }
    op.delete(path).await.map_err(message)
}

pub(super) async fn upload(
    op: &Operator,
    path: &str,
    file: &mut File,
    meta: &Metadata,
) -> Result<u64> {
    if meta.len() > MAX_UPLOAD {
        return Err("单个对象不能超过 80,000 MiB（8 MiB × 10,000 个分片）".into());
    }
    // Some S3-compatible servers implement the condition only on PutObject.
    // Check first as well; the final commit still sends a server-side condition.
    match op.stat(path).await {
        Ok(_) => return Err("远程已有同名文件，未覆盖".into()),
        Err(e) if e.kind() == ErrorKind::NotFound => (),
        Err(e) => return Err(message(e)),
    }
    let mut writer = op
        .writer_with(path)
        .chunk(PART)
        .if_not_exists(true)
        .await
        .map_err(message)?;
    let result: Result<u64> = async {
        let mut sent = 0;
        loop {
            check_cancel()?;
            let remaining = meta.len() - sent;
            if remaining == 0 {
                break;
            }
            let mut buf = vec![0; (remaining.min(PART as u64)) as usize];
            file.read_exact(&mut buf).map_err(err)?;
            check_cancel()?;
            let n = buf.len() as u64;
            writer.write(buf).await.map_err(message)?;
            sent += n;
            progress(path, sent, meta.len());
        }
        source_unchanged(file, meta, sent)?;
        writer.close().await.map_err(message)?;
        Ok(sent)
    }
    .await;
    match result {
        Ok(bytes) => Ok(bytes),
        Err(error) => Err(cleanup_message(
            error,
            writer.abort().await.map_err(message),
        )),
    }
}

pub(super) async fn download(op: &Operator, path: &str, file: &mut File) -> Result<u64> {
    let meta = op.stat(path).await.map_err(message)?;
    if !meta.is_file() {
        return Err("只能下载普通文件".into());
    }
    let size = meta.content_length();
    progress(path, 0, size);
    let mut reader = op.reader_with(path);
    if let Some(etag) = meta.etag() {
        reader = reader.if_match(etag);
    }
    // Stream the whole GET, including for empty files. HEAD supplies the expected size.
    let mut stream = reader
        .await
        .map_err(message)?
        .into_bytes_stream(..)
        .await
        .map_err(message)?;
    let mut received = 0;
    loop {
        check_cancel()?;
        let Some(chunk) = stream
            .try_next()
            .await
            .map_err(|_| "对象存储下载中断，请检查网络和服务器")?
        else {
            break;
        };
        received += chunk.len() as u64;
        if received > size {
            return Err("远程文件已更改，请重试".into());
        }
        file.write_all(&chunk).map_err(err)?;
        progress(path, received, size);
    }
    check_size(received, size)?;
    Ok(received)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<Request<Buffer>>>>);

    impl HttpTransport for Recorder {
        async fn fetch(&self, req: Request<Buffer>) -> opendal::Result<Response<HttpBody>> {
            let body = match *req.method() {
                http::Method::GET => {
                    "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>"
                }
                http::Method::PUT => "",
                _ => panic!("unexpected method"),
            };
            self.0.lock().unwrap().push(req);
            Ok(Response::new(HttpBody::new(
                futures_util::stream::iter([Ok(Buffer::from(body))]),
                Some(body.len() as u64),
            )))
        }
    }

    #[test]
    fn oss_requests_sign_v4_with_region_and_session_token() {
        let transport = Recorder::default();
        let config = Config {
            label: "Fixture".into(),
            endpoint: "https://oss-cn-hangzhou.aliyuncs.com".into(),
            root: "backups".into(),
            read_only: false,
            provider: Provider::Oss {
                bucket: "fixture-bucket".into(),
                region: "cn-hangzhou".into(),
                access_key: "fixture-id".into(),
            },
        };
        let secret = Secret {
            value: "fixture-secret".into(),
            token: "fixture-sts".into(),
        };
        let op = operator_with_transport(&config, &secret, transport.clone()).unwrap();
        super::super::block(async {
            assert!(list(&op, "", 1).await?.entries.is_empty());
            mkdir(&op, "archive/").await
        })
        .unwrap();

        let requests = transport.0.lock().unwrap();
        assert_eq!(requests.len(), 3);
        let query = requests[0].uri().query().unwrap();
        assert!(
            url::form_urlencoded::parse(query.as_bytes())
                .any(|(key, value)| key == "prefix" && value == "backups/"),
            "{query}"
        );
        assert_eq!(requests[2].uri().path(), "/backups/archive/");
        assert_eq!(requests[2].headers()["x-oss-forbid-overwrite"], "true");
        for req in requests.iter() {
            assert_eq!(
                req.uri().host(),
                Some("fixture-bucket.oss-cn-hangzhou.aliyuncs.com")
            );
            let auth = &req.headers()["authorization"];
            assert!(
                auth.to_str()
                    .unwrap()
                    .starts_with("OSS4-HMAC-SHA256 Credential=fixture-id/")
            );
            assert!(
                auth.to_str()
                    .unwrap()
                    .contains("/cn-hangzhou/oss/aliyun_v4_request")
            );
            assert!(auth.is_sensitive());
            assert_eq!(req.headers()["x-oss-security-token"], "fixture-sts");
            assert!(req.headers()["x-oss-security-token"].is_sensitive());
        }
    }
}
