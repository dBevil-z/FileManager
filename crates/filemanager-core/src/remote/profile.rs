use super::{State, block, deadline, valid_path};
use crate::{Core, Result, err, str_arg};
use rusqlite::{OptionalExtension, params};
use secret_service::{EncryptionType, SecretService};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::{Host, Url};
use uuid::Uuid;
use zeroize::{ZeroizeOnDrop, Zeroizing};

const KEYRING_ERROR: &str = "无法访问系统钥匙串，请解锁钥匙串或仅在本次运行使用凭据";
const MAX_PROFILES: usize = 128;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub label: String,
    pub endpoint: String,
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub read_only: bool,
    pub provider: Provider,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Provider {
    Minio {
        #[serde(default)]
        bucket: String,
        #[serde(default)]
        region: String,
        user: String,
    },
    S3 {
        #[serde(default)]
        bucket: String,
        #[serde(default)]
        region: String,
        access_key: String,
        #[serde(default)]
        virtual_host: bool,
    },
    Oss {
        bucket: String,
        region: String,
        access_key: String,
    },
    Sftp {
        user: String,
        auth: SshAuth,
        #[serde(default)]
        key_file: String,
    },
    Ftp {
        user: String,
    },
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum SshAuth {
    Agent,
    Key,
    Password,
}

#[derive(Clone, Default, Deserialize, Serialize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub(super) struct Secret {
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub token: String,
}

#[derive(Deserialize, Serialize)]
struct Stored {
    config: Config,
    revision: u64,
    keyring_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Draft {
    id: Option<String>,
    revision: Option<u64>,
    config: Config,
    secret: Option<Secret>,
    #[serde(default)]
    remember: bool,
}

#[derive(Default)]
pub(super) struct Profiles {
    session: std::collections::HashMap<String, Secret>,
}

fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
    // JSON errors may quote malformed input. The input can contain secrets.
    serde_json::from_value(v.clone()).map_err(|_| "远程连接参数无效".into())
}

fn parse_id(id: &str) -> Result<&str> {
    if Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id) {
        Ok(id)
    } else {
        Err("无效的远程连接编号".into())
    }
}

fn field(s: &str, max: usize) -> bool {
    s.len() <= max && !s.chars().any(char::is_control)
}

pub(super) fn bucket_name(bucket: &str) -> bool {
    (3..=63).contains(&bucket.len())
        && bucket.split('.').all(|s| {
            !s.is_empty()
                && s.as_bytes()[0].is_ascii_alphanumeric()
                && s.as_bytes()[s.len() - 1].is_ascii_alphanumeric()
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
        && bucket.parse::<std::net::Ipv4Addr>().is_err()
}

impl Config {
    pub fn bucket(&self) -> Option<&str> {
        match &self.provider {
            Provider::Minio { bucket, .. }
            | Provider::S3 { bucket, .. }
            | Provider::Oss { bucket, .. } => Some(bucket),
            _ => None,
        }
    }

    pub fn lists_buckets(&self) -> bool {
        matches!(&self.provider, Provider::Minio { bucket, .. } | Provider::S3 { bucket, .. } if bucket.is_empty())
    }

    fn in_bucket(mut self, args: &Value) -> Result<Self> {
        if let Some(value) = args.get("bucket") {
            let name = value.as_str().ok_or("Bucket 参数无效")?;
            match &mut self.provider {
                Provider::Minio { bucket, .. } | Provider::S3 { bucket, .. }
                    if (bucket.is_empty() && bucket_name(name)) || bucket == name =>
                {
                    *bucket = name.into();
                }
                Provider::Oss { bucket, .. } if bucket == name => (),
                _ => return Err("Bucket 不属于此连接范围".into()),
            }
        }
        Ok(self)
    }

    pub fn origin(&self) -> Result<Url> {
        Url::parse(&self.endpoint).map_err(|_| "服务器地址无效".into())
    }

    fn checked(mut self) -> Result<Self> {
        self.label = self.label.trim().to_owned();
        self.endpoint = self.endpoint.trim().to_owned();
        if self.label.is_empty() || !field(&self.label, 100) || !field(&self.endpoint, 2048) {
            return Err("连接名称或地址无效".into());
        }
        let url = self.origin()?;
        let scheme = match &self.provider {
            Provider::Minio { .. } | Provider::S3 { .. } => {
                matches!(url.scheme(), "http" | "https")
            }
            Provider::Oss { .. } => url.scheme() == "https",
            Provider::Sftp { .. } => matches!(url.scheme(), "sftp" | "ssh"),
            Provider::Ftp { .. } => matches!(url.scheme(), "ftp" | "ftps"),
        };
        if !scheme
            || !self.endpoint.contains("://")
            || url.host().is_none()
            || url.port() == Some(0)
            || self.endpoint.contains(['@', '\\'])
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("服务器地址只能包含协议、主机和端口；请在下方填写目录和凭据".into());
        }
        self.endpoint = url.to_string().trim_end_matches('/').to_owned();
        let root = self.root.trim().trim_matches('/');
        let explicit_root = !self.root.trim().is_empty()
            && matches!(self.provider, Provider::Sftp { .. } | Provider::Ftp { .. });
        valid_path(root, true)?;
        self.root = if explicit_root {
            format!("/{root}")
        } else {
            root.to_owned()
        };
        let oss = matches!(self.provider, Provider::Oss { .. });
        let virtual_host = matches!(
            self.provider,
            Provider::S3 {
                virtual_host: true,
                ..
            }
        );
        match &mut self.provider {
            Provider::Minio {
                bucket,
                region,
                user,
            } => {
                *bucket = bucket.trim().into();
                *region = region.trim().into();
                *user = user.trim().into();
                if (!bucket.is_empty() && !bucket_name(bucket))
                    || !valid_region(region)
                    || user.is_empty()
                    || !field(user, 512)
                    || user.chars().any(char::is_whitespace)
                {
                    return Err("请填写有效的 MinIO 账号；Bucket 和 Region 可以留空".into());
                }
            }
            Provider::S3 {
                bucket,
                region,
                access_key,
                ..
            }
            | Provider::Oss {
                bucket,
                region,
                access_key,
            } => {
                *bucket = bucket.trim().into();
                *region = region.trim().into();
                if (bucket.is_empty() && oss)
                    || (!bucket.is_empty() && !bucket_name(bucket))
                    || (oss && bucket.contains('.'))
                    || (region.is_empty() && oss)
                    || !valid_region(region)
                    || access_key.is_empty()
                    || !field(access_key, 512)
                    || access_key.chars().any(char::is_whitespace)
                {
                    return Err("请填写有效的 Bucket、Region 和 Access Key ID".into());
                }
                if (oss || virtual_host)
                    && !matches!(url.host(), Some(Host::Domain(host)) if host.parse::<std::net::IpAddr>().is_err())
                {
                    return Err("虚拟主机寻址要求服务器使用域名".into());
                }
            }
            Provider::Sftp {
                user,
                auth,
                key_file,
            } => {
                if user.is_empty() || !field(user, 255) || !field(key_file, 4096) {
                    return Err("请填写有效的 SSH 用户名和密钥文件路径".into());
                }
                if *auth == SshAuth::Key && key_file.is_empty() {
                    return Err("请选择 SSH 私钥文件".into());
                }
                if *auth != SshAuth::Key {
                    key_file.clear();
                }
            }
            Provider::Ftp { user } => {
                if user.is_empty() || !field(user, 255) {
                    return Err("请填写 FTP 用户名；匿名连接使用 anonymous".into());
                }
            }
        }
        Ok(self)
    }

    fn needs_secret(&self) -> bool {
        match &self.provider {
            Provider::Sftp { auth, .. } => *auth == SshAuth::Password,
            Provider::Ftp { user } => !user.eq_ignore_ascii_case("anonymous"),
            _ => true,
        }
    }

    fn check_secret(&self, secret: &Secret) -> Result<()> {
        if !field(&secret.value, 4096)
            || !field(&secret.token, 16384)
            || (!secret.token.is_empty()
                && !matches!(
                    self.provider,
                    Provider::Minio { .. } | Provider::S3 { .. } | Provider::Oss { .. }
                ))
            || (!secret.value.is_empty()
                && matches!(
                    self.provider,
                    Provider::Sftp {
                        auth: SshAuth::Agent,
                        ..
                    }
                ))
        {
            return Err("远程连接凭据格式无效".into());
        }
        if self.needs_secret() && secret.value.is_empty() {
            return Err("请输入此连接的密码或密钥".into());
        }
        Ok(())
    }

    fn same_credentials(&self, previous: &Self) -> bool {
        self.endpoint == previous.endpoint && self.provider == previous.provider
    }
}

pub(super) fn valid_region(region: &str) -> bool {
    region.len() <= 80
        && region
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl Secret {
    fn is_empty(&self) -> bool {
        self.value.is_empty() && self.token.is_empty()
    }
}

impl Stored {
    fn read(core: &Core, id: &str) -> Result<Self> {
        let value: Option<String> = core
            .connection()?
            .query_row(
                "SELECT value FROM remote_profiles WHERE id=?1",
                [parse_id(id)?],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        serde_json::from_str(&value.ok_or("远程连接不存在")?).map_err(err)
    }

    fn public(&self, id: &str, profiles: &Profiles) -> Value {
        json!({
            "id": id, "config": self.config, "revision": self.revision,
            "remember": self.keyring_id.is_some(),
            "needs_secret": self.keyring_id.is_none() && self.config.needs_secret() && !profiles.session.contains_key(id)
        })
    }
}

fn attrs(id: &str) -> std::collections::HashMap<&str, &str> {
    [
        ("application", "io.filemanager.linux"),
        ("remote-credential", id),
    ]
    .into()
}

fn keyring_get(id: &str) -> Result<Secret> {
    block(deadline(async {
        let service = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(|_| KEYRING_ERROR)?;
        let found = service
            .search_items(attrs(id))
            .await
            .map_err(|_| KEYRING_ERROR)?;
        let item = if let Some(item) = found.unlocked.first() {
            item
        } else if let Some(item) = found.locked.first() {
            item.unlock().await.map_err(|_| KEYRING_ERROR)?;
            item
        } else {
            return Err("钥匙串中没有此凭据，请重新输入".into());
        };
        let data = Zeroizing::new(item.get_secret().await.map_err(|_| KEYRING_ERROR)?);
        serde_json::from_slice(&data).map_err(|_| KEYRING_ERROR.into())
    }))
}

fn keyring_set(id: &str, secret: &Secret) -> Result<()> {
    block(deadline(async {
        let service = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(|_| KEYRING_ERROR)?;
        let collection = service
            .get_default_collection()
            .await
            .map_err(|_| KEYRING_ERROR)?;
        if collection.is_locked().await.map_err(|_| KEYRING_ERROR)? {
            collection.unlock().await.map_err(|_| KEYRING_ERROR)?;
        }
        let data = Zeroizing::new(serde_json::to_vec(secret).map_err(|_| KEYRING_ERROR)?);
        collection
            .create_item(
                "FileManager remote storage",
                attrs(id),
                &data,
                true,
                "application/json",
            )
            .await
            .map_err(|_| KEYRING_ERROR)?;
        Ok(())
    }))
}

fn keyring_remove(id: &str) -> Result<()> {
    block(deadline(async {
        let service = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(|_| KEYRING_ERROR)?;
        let found = service
            .search_items(attrs(id))
            .await
            .map_err(|_| KEYRING_ERROR)?;
        for item in &found.locked {
            item.unlock().await.map_err(|_| KEYRING_ERROR)?;
        }
        for item in found.unlocked.iter().chain(&found.locked) {
            item.delete().await.map_err(|_| KEYRING_ERROR)?;
        }
        Ok(())
    }))
}

pub(crate) fn profiles(core: &Core) -> Result<Value> {
    let state = core.remote.profiles.lock().map_err(err)?;
    let conn = core.connection()?;
    let mut query = conn
        .prepare("SELECT id,value FROM remote_profiles ORDER BY rowid")
        .map_err(err)?;
    let mut rows = query.query([]).map_err(err)?;
    let mut values = Vec::new();
    while let Some(row) = rows.next().map_err(err)? {
        let id: String = row.get(0).map_err(err)?;
        let stored: Stored =
            serde_json::from_str(&row.get::<_, String>(1).map_err(err)?).map_err(err)?;
        values.push(stored.public(&id, &state));
    }
    Ok(json!(values))
}

pub(crate) fn save(core: &Core, args: &Value) -> Result<Value> {
    let draft: Draft = parse(args)?;
    let config = draft.config.checked()?;
    let mut state = core.remote.profiles.lock().map_err(err)?;
    let mut conn = core.connection()?;
    let (id, old) = if let Some(id) = draft.id {
        let old = Stored::read(core, &id)?;
        if draft.revision != Some(old.revision) {
            return Err("连接已在其他窗口更改，请刷新后重试".into());
        }
        (id, Some(old))
    } else {
        let count: usize = conn
            .query_row("SELECT count(*) FROM remote_profiles", [], |r| r.get(0))
            .map_err(err)?;
        if count >= MAX_PROFILES {
            return Err("远程连接数量已达上限".into());
        }
        (Uuid::new_v4().to_string(), None)
    };
    let old_key = old.as_ref().and_then(|p| p.keyring_id.as_deref());
    let same = old
        .as_ref()
        .is_some_and(|p| config.same_credentials(&p.config));
    let keep = draft.secret.is_none() && same && draft.remember && old_key.is_some();
    let secret = if keep {
        None
    } else {
        let secret = match draft.secret {
            Some(secret) => secret,
            None if same => match (state.session.get(&id), old_key) {
                (Some(secret), _) => secret.clone(),
                (None, Some(key)) => keyring_get(key)?,
                (None, None) => Secret::default(),
            },
            None => Secret::default(),
        };
        config.check_secret(&secret)?;
        Some(secret)
    };
    let mut new_key = None;
    if let Some(secret) = &secret {
        if draft.remember && !secret.is_empty() {
            let key = Uuid::new_v4().to_string();
            keyring_set(&key, secret)?;
            new_key = Some(key);
        }
    }
    let record = Stored {
        config,
        revision: old.as_ref().map_or(1, |p| p.revision + 1),
        keyring_id: if keep {
            old_key.map(str::to_owned)
        } else {
            new_key.clone()
        },
    };
    let mut persist = || -> Result<()> {
        let tx = conn.transaction().map_err(err)?;
        tx.execute("INSERT INTO remote_profiles(id,value) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET value=excluded.value",
            params![id, serde_json::to_string(&record).map_err(err)?]).map_err(err)?;
        if let Some(key) = old_key.filter(|_| !keep) {
            keyring_remove(key)?;
        }
        tx.commit().map_err(err)
    };
    if let Err(e) = (persist)() {
        if let Some(key) = &new_key {
            let _ = keyring_remove(key);
        }
        return Err(e);
    }
    state.session.remove(&id);
    if let Some(secret) = secret.filter(|secret| !secret.is_empty() && new_key.is_none()) {
        state.session.insert(id.clone(), secret);
    }
    Ok(record.public(&id, &state))
}

pub(crate) fn remove(core: &Core, args: &Value) -> Result<Value> {
    let id = str_arg(args, "id")?;
    let mut state = core.remote.profiles.lock().map_err(err)?;
    let stored = Stored::read(core, id)?;
    if args["revision"].as_u64() != Some(stored.revision) {
        return Err("连接已在其他窗口更改，请刷新后重试".into());
    }
    let mut conn = core.connection()?;
    let tx = conn.transaction().map_err(err)?;
    tx.execute("DELETE FROM remote_profiles WHERE id=?1", [id])
        .map_err(err)?;
    if let Some(key) = &stored.keyring_id {
        keyring_remove(key)?;
    }
    tx.commit().map_err(err)?;
    state.session.remove(id);
    Ok(json!({ "ok": true }))
}

pub(super) fn config_for_test(core: &Core, args: &Value) -> Result<(Config, Secret)> {
    let draft: Draft = parse(args)?;
    let config = draft.config.checked()?;
    let secret = match (draft.secret, draft.id) {
        (Some(secret), _) => secret,
        (None, Some(id)) => {
            let (old, session) = resolve(&core.remote, core, &id, draft.revision, false)?;
            if config.same_credentials(&old.config) {
                read_secret(session, old.keyring_id)?
            } else {
                Secret::default()
            }
        }
        _ => Secret::default(),
    };
    config.check_secret(&secret)?;
    Ok((config, secret))
}

fn resolve(
    state: &State,
    core: &Core,
    id: &str,
    revision: Option<u64>,
    write: bool,
) -> Result<(Stored, Option<Secret>)> {
    let profiles = state.profiles.lock().map_err(err)?;
    let stored = Stored::read(core, id)?;
    if revision != Some(stored.revision) {
        return Err("连接已在其他窗口更改，请刷新后重试".into());
    }
    if write && stored.config.read_only {
        return Err("此连接为只读".into());
    }
    Ok((stored, profiles.session.get(id).cloned()))
}

pub(super) fn load(
    core: &Core,
    id: &str,
    revision: Option<u64>,
    write: bool,
) -> Result<(Config, Secret)> {
    let (stored, session) = resolve(&core.remote, core, id, revision, write)?;
    let secret = read_secret(session, stored.keyring_id)?;
    stored.config.check_secret(&secret)?;
    Ok((stored.config, secret))
}

pub(super) fn scoped(core: &Core, args: &Value, write: bool) -> Result<(Config, Secret)> {
    let (config, secret) = load(core, str_arg(args, "id")?, args["revision"].as_u64(), write)?;
    Ok((config.in_bucket(args)?, secret))
}

fn read_secret(session: Option<Secret>, keyring_id: Option<String>) -> Result<Secret> {
    match (session, keyring_id) {
        (Some(secret), _) => Ok(secret),
        (None, Some(key)) => keyring_get(&key),
        (None, None) => Ok(Secret::default()),
    }
}
