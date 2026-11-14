mod batch;
mod ftp;
mod object;
mod object_service;
mod pagination;
mod profile;
mod sftp;
mod sync;

use crate::{Core, Result, err, jobs, paths, str_arg};
use profile::{Config, Provider, Secret};
pub(super) use profile::{profiles, remove, save};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs::{File, Metadata, OpenOptions},
    future::Future,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::{Mutex, OnceLock, TryLockError},
    thread,
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(20);
const MAX_ENTRIES: usize = 100_000;
const CHUNK: usize = 256 * 1024;

#[derive(Default)]
pub(super) struct State {
    profiles: Mutex<profile::Profiles>,
    writes: Mutex<()>,
    pages: Mutex<pagination::Pages>,
    reports: Mutex<batch::Reports>,
    plans: Mutex<sync::Plans>,
}

#[derive(Clone, Debug, Serialize)]
struct Entry {
    name: String,
    path: String,
    is_dir: bool,
    size: Option<u64>,
    modified: Option<i64>,
}

// Browsing another connection must remain responsive during a transfer. Only
// remote mutations are serialized; cancellation still works while waiting.
fn write_slot(state: &State) -> Result<std::sync::MutexGuard<'_, ()>> {
    loop {
        check_cancel()?;
        match state.writes.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(75)),
            Err(TryLockError::Poisoned(e)) => return Err(err(e)),
        }
    }
}

#[derive(Default, Serialize)]
struct Listing {
    entries: Vec<Entry>,
    truncated: bool,
    skipped: usize,
}

impl Listing {
    fn push(
        &mut self,
        dir: &str,
        name: &str,
        is_dir: bool,
        size: Option<u64>,
        modified: Option<i64>,
    ) {
        let mut path = format!("{dir}{name}");
        if is_dir {
            path.push('/');
        }
        if !valid_name(name) || valid_path(&path, is_dir).is_err() {
            self.skipped += 1;
        } else {
            self.entries.push(Entry {
                name: name.into(),
                path,
                is_dir,
                size: size.filter(|_| !is_dir),
                modified,
            });
        }
    }
}

fn block<T>(future: impl Future<Output = Result<T>>) -> Result<T> {
    static RUNTIME: OnceLock<std::result::Result<tokio::runtime::Runtime, std::io::Error>> =
        OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("filemanager-remote")
                .enable_all()
                .build()
        })
        .as_ref()
        .map_err(err)?
        .block_on(future)
}

async fn deadline<T>(future: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(TIMEOUT, future)
        .await
        .map_err(|_| "远程操作超时，请检查网络和服务器")?
}

fn tls_config() -> Result<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    Ok(
        rustls::ClientConfig::builder_with_provider(
            rustls::crypto::ring::default_provider().into(),
        )
        .with_safe_default_protocol_versions()
        .map_err(err)?
        .with_root_certificates(roots)
        .with_no_client_auth(),
    )
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.chars().any(char::is_control)
}

fn valid_path(path: &str, directory: bool) -> Result<()> {
    if path.is_empty() && directory {
        return Ok(());
    }
    let body = if directory {
        path.strip_suffix('/').unwrap_or(path)
    } else {
        path
    };
    // OpenDAL normalizes whitespace at either end and empty segments; reject
    // aliases instead of silently operating on another object.
    if path.len() <= 4096 && path.trim() == path && body.split('/').all(valid_name) {
        Ok(())
    } else {
        Err("远程路径无效，不能包含空段、控制字符或 . / .. 路径段".into())
    }
}

fn directory(args: &Value) -> Result<&str> {
    let dir = str_arg(args, "path")?;
    if !dir.is_empty() && !dir.ends_with('/') {
        return Err("远程目录路径必须以 / 结尾".into());
    }
    valid_path(dir, true)?;
    Ok(dir)
}

fn name(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

fn parent(path: &str) -> &str {
    let path = path.trim_end_matches('/');
    path.rfind('/').map_or("", |i| &path[..=i])
}

fn check_cancel() -> Result<()> {
    if jobs::cancelled() {
        Err("远程传输已取消".into())
    } else {
        Ok(())
    }
}

fn progress(path: &str, bytes: u64, total: u64) {
    if !batch::file_progress(path, bytes, total) {
        jobs::progress(name(path), bytes, bytes, total);
    }
}

fn check_size(bytes: u64, expected: u64) -> Result<()> {
    if bytes == expected {
        Ok(())
    } else {
        Err("传输大小与文件不符，未提交目标文件".into())
    }
}

fn source_unchanged(file: &File, before: &Metadata, bytes: u64) -> Result<()> {
    check_size(bytes, before.len())?;
    let after = file.metadata().map_err(err)?;
    if before.len() != after.len()
        || before.modified().map_err(err)? != after.modified().map_err(err)?
    {
        return Err("上传期间本地文件已更改，请重试".into());
    }
    check_cancel()
}

fn cleanup_message(error: String, cleanup: Result<()>) -> String {
    if cleanup.is_err() {
        format!("{error}；服务器可能保留了未完成的上传，请检查远程目录")
    } else {
        error
    }
}

enum Backend {
    Object(opendal::Operator),
    Sftp(sftp::Client),
    Ftp(Box<ftp::Client>),
}

impl Backend {
    async fn ensure_dir(&mut self, path: &str) -> Result<()> {
        match self {
            Self::Object(op) => object::ensure_dir(op, path).await,
            Self::Sftp(client) => client.ensure_dir(path),
            Self::Ftp(client) => client.ensure_dir(path).await,
        }
    }
    async fn connect(core: &Core, config: &Config, secret: &Secret) -> Result<Self> {
        match config.provider {
            Provider::Minio { .. } | Provider::S3 { .. } | Provider::Oss { .. } => {
                if config.lists_buckets() {
                    return Err("请先选择 Bucket，再操作桶内文件".into());
                }
                object::connect(config, secret).await.map(Self::Object)
            }
            Provider::Sftp { .. } => sftp::Client::connect(core, config, secret)
                .await
                .map(Self::Sftp),
            Provider::Ftp { .. } => ftp::Client::connect(config, secret)
                .await
                .map(|f| Self::Ftp(Box::new(f))),
        }
    }

    async fn listing(&mut self, dir: &str, limit: usize) -> Result<Listing> {
        match self {
            Self::Object(op) => object::list(op, dir, limit).await,
            Self::Sftp(client) => client.list(dir, limit),
            Self::Ftp(client) => client.list(dir, limit).await,
        }
    }

    async fn mkdir(&mut self, path: &str) -> Result<()> {
        match self {
            Self::Object(op) => object::mkdir(op, path).await,
            Self::Sftp(client) => client.mkdir(path),
            Self::Ftp(client) => client.mkdir(path).await,
        }
    }

    async fn delete(&mut self, path: &str, is_dir: bool) -> Result<()> {
        match self {
            Self::Object(op) => object::delete(op, path, is_dir).await,
            Self::Sftp(client) => client.delete(path, is_dir),
            Self::Ftp(client) => client.delete(path, is_dir).await,
        }
    }

    async fn upload(&mut self, path: &str, file: &mut File, meta: &Metadata) -> Result<u64> {
        match self {
            Self::Object(op) => object::upload(op, path, file, meta).await,
            Self::Sftp(client) => client.upload(path, file, meta),
            Self::Ftp(client) => client.upload(path, file, meta).await,
        }
    }

    async fn download(&mut self, path: &str, file: &mut File) -> Result<u64> {
        match self {
            Self::Object(op) => object::download(op, path, file).await,
            Self::Sftp(client) => client.download(path, file),
            Self::Ftp(client) => client.download(path, file).await,
        }
    }
}

fn operate<T>(
    core: &Core,
    args: &Value,
    write: bool,
    action: impl AsyncFnOnce(&mut Backend) -> Result<T>,
) -> Result<T> {
    let _slot = write.then(|| write_slot(&core.remote)).transpose()?;
    let (config, secret) = profile::scoped(core, args, write)?;
    check_cancel()?;
    block(async {
        let mut backend = Backend::connect(core, &config, &secret).await?;
        action(&mut backend).await
    })
}

pub(super) fn test(core: &Core, args: &Value) -> Result<Value> {
    let (config, secret) = profile::config_for_test(core, args)?;
    block(async {
        if config.lists_buckets() {
            object_service::Buckets::open(config, secret).await?;
            return Ok(json!({"ok": true, "scope": "buckets"}));
        }
        Backend::connect(core, &config, &secret)
            .await?
            .listing("", 1)
            .await?;
        Ok(json!({ "ok": true }))
    })
}

pub(super) fn list(core: &Core, args: &Value) -> Result<Value> {
    pagination::list(core, args)
}

pub(super) use batch::{report as transfer_report, transfer};
pub(super) use pagination::close as list_close;
pub(super) use sync::{close as sync_close, page as sync_page, preview as sync_preview};

pub(super) fn mkdir(core: &Core, args: &Value) -> Result<Value> {
    let dir = directory(args)?;
    let name = str_arg(args, "name")?;
    if !valid_name(name) || name.trim() != name {
        return Err("远程文件夹名称无效".into());
    }
    let path = format!("{dir}{name}/");
    valid_path(&path, true)?;
    operate(core, args, true, async |backend| backend.mkdir(&path).await)?;
    Ok(json!({ "path": path }))
}

pub(super) fn delete(core: &Core, args: &Value) -> Result<Value> {
    let path = str_arg(args, "path")?;
    let is_dir = path.ends_with('/');
    valid_path(path, is_dir)?;
    operate(core, args, true, async |backend| {
        backend.delete(path, is_dir).await
    })?;
    Ok(json!({ "ok": true }))
}

pub(super) fn upload(core: &Core, args: &Value) -> Result<Value> {
    let dir = directory(args)?;
    let source = paths::decode(core, str_arg(args, "local_path")?)?;
    let name = source
        .file_name()
        .and_then(|p| p.to_str())
        .ok_or("远程上传需要 UTF-8 文件名")?;
    let path = format!("{dir}{name}");
    valid_path(&path, false)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&source)
        .map_err(err)?;
    let meta = file.metadata().map_err(err)?;
    if !meta.is_file() {
        return Err("只能上传普通文件".into());
    }
    let bytes = operate(core, args, true, async |backend| {
        progress(&path, 0, meta.len());
        backend.upload(&path, &mut file, &meta).await
    })?;
    Ok(json!({ "path": path, "bytes": bytes }))
}

pub(super) fn download(core: &Core, args: &Value) -> Result<Value> {
    let path = str_arg(args, "path")?;
    valid_path(path, false)?;
    paths::check_name(name(path))?;
    let directory = paths::decode(core, str_arg(args, "local_dir")?)?
        .canonicalize()
        .map_err(err)?;
    let target = directory.join(name(path));
    if paths::existing(&target) {
        return Err("本地已有同名项目，请选择其他目录".into());
    }
    let mut file = tempfile::Builder::new()
        .prefix(".filemanager-download-")
        .tempfile_in(&directory)
        .map_err(err)?;
    let bytes = operate(core, args, false, async |backend| {
        backend.download(path, file.as_file_mut()).await
    })?;
    check_cancel()?;
    file.as_file().sync_all().map_err(err)?;
    check_cancel()?;
    file.persist_noclobber(&target).map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            "本地已有同名项目，请选择其他目录".into()
        } else {
            err(e.error)
        }
    })?;
    Ok(json!({ "path": paths::encode(&target), "bytes": bytes }))
}

fn remote_file(directory: &Path, path: &str) -> std::path::PathBuf {
    if path.is_empty() {
        directory.to_owned()
    } else {
        directory.join(path.trim_end_matches('/'))
    }
}
