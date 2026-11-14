use super::{
    Backend, MAX_ENTRIES, block, check_cancel, directory, name,
    pagination::{Reader, Step},
    parent, profile, valid_path, write_slot,
};
use crate::{Core, Result, err, jobs, paths, str_arg};
use rustix::fs::{Mode, OFlags, mkdirat, openat};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File, Metadata, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(super) const DEPTH: usize = 64;
const TTL: Duration = Duration::from_secs(15 * 60);
thread_local! { static PROGRESS: Cell<Option<(u64, u64, u64)>> = const { Cell::new(None) }; }

pub(super) fn file_progress(path: &str, bytes: u64, _total: u64) -> bool {
    PROGRESS.with(|p| {
        if let Some((done, total, previous)) = p.get() {
            jobs::progress(path, previous.saturating_add(bytes), done, total);
            true
        } else {
            false
        }
    })
}
struct Progress;
impl Drop for Progress {
    fn drop(&mut self) {
        PROGRESS.with(|p| p.set(None));
    }
}

// Descend through directory file descriptors. A renamed directory or a swapped
// symlink cannot redirect a recursive download outside the selected directory.
pub(super) struct LocalDir(File);
impl LocalDir {
    pub fn open(path: &Path) -> Result<Self> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(path)
            .map(Self)
            .map_err(err)
    }
    fn proc_path(&self) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.0.as_raw_fd()))
    }
    fn child(&self, path: &str, create: bool) -> Result<Self> {
        let mut dir = self.0.try_clone().map_err(err)?;
        for segment in path
            .trim_end_matches('/')
            .split('/')
            .filter(|p| !p.is_empty())
        {
            paths::check_name(segment)?;
            if !super::valid_name(segment) {
                return Err("本地目标路径无效".into());
            }
            let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
            let fd = match openat(&dir, segment, flags, Mode::empty()) {
                Ok(fd) => fd,
                Err(rustix::io::Errno::NOENT) if create => {
                    match mkdirat(&dir, segment, Mode::from_bits_truncate(0o755)) {
                        Ok(()) | Err(rustix::io::Errno::EXIST) => (),
                        Err(e) => return Err(err(e)),
                    }
                    openat(&dir, segment, flags, Mode::empty()).map_err(err)?
                }
                Err(e) => return Err(format!("本地目录无法安全访问：{}", err(e))),
            };
            dir = File::from(fd);
        }
        Ok(Self(dir))
    }
    fn source(&self, path: &str, expected: &Snapshot) -> Result<(File, Metadata)> {
        let dir = self.child(parent(path), false)?;
        let fd = openat(
            &dir.0,
            name(path),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(err)?;
        let file = File::from(fd);
        let meta = file.metadata().map_err(err)?;
        if !meta.is_file() || Snapshot::local(&meta).identity != expected.identity {
            return Err("上传前本地文件已更改，请重新选择".into());
        }
        Ok((file, meta))
    }
    async fn download(&self, backend: &mut Backend, remote: &str, relative: &str) -> Result<u64> {
        let dir = self.child(parent(relative), true)?;
        paths::check_name(name(relative))?;
        let target = dir.proc_path().join(name(relative));
        if paths::existing(&target) {
            return Err("本地已有同名项目，请选择其他目录".into());
        }
        let mut staging = tempfile::Builder::new()
            .prefix(".filemanager-download-")
            .tempfile_in(dir.proc_path())
            .map_err(err)?;
        let bytes = backend.download(remote, staging.as_file_mut()).await?;
        check_cancel()?;
        staging.as_file().sync_all().map_err(err)?;
        check_cancel()?;
        staging.persist_noclobber(target).map_err(|e| {
            if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                "本地已有同名项目，请选择其他目录".into()
            } else {
                err(e.error)
            }
        })?;
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct Snapshot {
    pub kind: &'static str,
    pub size: Option<u64>,
    pub modified: Option<i64>,
    #[serde(skip)]
    identity: Option<(u64, u64, u64, i64, i64)>,
}
impl Snapshot {
    fn local(meta: &Metadata) -> Self {
        Self {
            kind: if meta.is_dir() {
                "directory"
            } else if meta.is_file() {
                "file"
            } else {
                "unsupported"
            },
            size: meta.is_file().then_some(meta.len()),
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|d| i64::try_from(d.as_millis()).ok()),
            identity: Some((
                meta.dev(),
                meta.ino(),
                meta.len(),
                meta.mtime(),
                meta.mtime_nsec(),
            )),
        }
    }
    fn remote(entry: &super::Entry) -> Self {
        Self {
            kind: if entry.is_dir { "directory" } else { "file" },
            size: entry.size,
            modified: entry.modified,
            identity: None,
        }
    }
}

pub(super) fn scan_local(root: &Path) -> Result<BTreeMap<String, Snapshot>> {
    fn walk(
        dir: &LocalDir,
        prefix: &str,
        depth: usize,
        map: &mut BTreeMap<String, Snapshot>,
    ) -> Result<()> {
        if depth > DEPTH {
            return Err("目录层级超过 64 层，请选择较小的范围".into());
        }
        for item in fs::read_dir(dir.proc_path()).map_err(err)? {
            check_cancel()?;
            if map.len() >= MAX_ENTRIES {
                return Err("扫描超过 100,000 项，请选择较小的范围".into());
            }
            let item = item.map_err(err)?;
            let name = item
                .file_name()
                .into_string()
                .map_err(|_| "扫描需要 UTF-8 文件名")?;
            let relative = format!("{prefix}{name}");
            valid_path(&relative, false)?;
            let meta = fs::symlink_metadata(item.path()).map_err(err)?;
            map.insert(relative.clone(), Snapshot::local(&meta));
            jobs::progress(&relative, 0, map.len() as u64, 0);
            if meta.is_dir() {
                walk(
                    &dir.child(&name, false)?,
                    &format!("{relative}/"),
                    depth + 1,
                    map,
                )?;
            }
        }
        Ok(())
    }
    let mut map = BTreeMap::new();
    walk(&LocalDir::open(root)?, "", 0, &mut map)?;
    Ok(map)
}

pub(super) async fn scan_remote(
    core: &Core,
    config: &profile::Config,
    secret: &profile::Secret,
    root: &str,
) -> Result<BTreeMap<String, Snapshot>> {
    let mut map = BTreeMap::new();
    let mut pending = vec![(root.to_owned(), 0)];
    while let Some((dir, depth)) = pending.pop() {
        check_cancel()?;
        if depth > DEPTH {
            return Err("目录层级超过 64 层，请选择较小的范围".into());
        }
        let mut reader = Reader::open(core, config, secret, &dir).await?;
        while let Some(step) = reader.next().await? {
            if map.len() >= MAX_ENTRIES {
                return Err("扫描超过 100,000 项，请选择较小的范围".into());
            }
            let Step::Entry(entry) = step else {
                return Err("远程目录含有无法读取的条目，无法生成完整计划".into());
            };
            let relative = entry
                .path
                .strip_prefix(root)
                .ok_or("远程目录超出扫描范围")?
                .trim_end_matches('/')
                .to_owned();
            if map
                .insert(relative.clone(), Snapshot::remote(&entry))
                .is_some()
            {
                return Err("远程目录含有无法映射到本地的同名项目".into());
            }
            jobs::progress(&entry.path, 0, map.len() as u64, 0);
            if entry.is_dir {
                pending.push((entry.path, depth + 1));
            }
        }
    }
    Ok(map)
}

struct Item {
    remote: String,
    relative: String,
    source: Option<(usize, String, Snapshot)>,
    is_dir: bool,
}
struct Transfer {
    items: Vec<Item>,
    roots: Vec<LocalDir>,
    destination: Option<LocalDir>,
    upload: bool,
}

fn strings(args: &Value, key: &str) -> Result<Vec<String>> {
    let values = args[key].as_array().ok_or("请选择需要传输的文件或目录")?;
    if values.is_empty() || values.len() > 1000 {
        return Err("一次可选择 1 到 1,000 个项目".into());
    }
    values
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "传输路径无效".into())
        })
        .collect()
}

impl Transfer {
    async fn plan(
        core: &Core,
        args: &Value,
        config: &profile::Config,
        secret: &profile::Secret,
    ) -> Result<Self> {
        let upload = match str_arg(args, "direction")? {
            "upload" => true,
            "download" => false,
            _ => return Err("传输方向无效".into()),
        };
        let mut plan = Self {
            items: vec![],
            roots: vec![],
            destination: None,
            upload,
        };
        let mut root_indices = HashMap::<PathBuf, usize>::new();
        let mut targets = HashSet::new();
        let mut add = |item: Item| -> Result<()> {
            if plan.items.len() >= MAX_ENTRIES {
                return Err("扫描超过 100,000 项，请选择较小的范围".into());
            }
            valid_path(&item.remote, item.is_dir)?;
            let target = if upload {
                item.remote.trim_end_matches('/')
            } else {
                item.relative.trim_end_matches('/')
            };
            for part in item.relative.trim_end_matches('/').split('/') {
                paths::check_name(part)?;
            }
            let key = if matches!(config.provider, profile::Provider::Ftp { .. }) && upload {
                target.to_lowercase()
            } else {
                target.into()
            };
            if !targets.insert(key) {
                return Err("所选项目存在同名目标，请分开传输".into());
            }
            plan.items.push(item);
            Ok(())
        };
        if upload {
            let path = directory(args)?;
            for input in strings(args, "local_paths")? {
                check_cancel()?;
                let local = paths::canonical_entry(&paths::decode(core, &input)?)?;
                let meta = fs::symlink_metadata(&local).map_err(err)?;
                if !meta.is_file() && !meta.is_dir() {
                    return Err("只能上传普通文件或目录，不能上传符号链接".into());
                }
                let name = local
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or("远程上传需要 UTF-8 文件名")?
                    .to_owned();
                let root_path = if meta.is_file() {
                    local.parent().ok_or("缺少上级目录")?
                } else {
                    local.as_path()
                };
                let root = if let Some(index) = root_indices.get(root_path) {
                    *index
                } else {
                    let index = plan.roots.len();
                    plan.roots.push(LocalDir::open(root_path)?);
                    root_indices.insert(root_path.to_owned(), index);
                    index
                };
                if meta.is_file() {
                    add(Item {
                        remote: format!("{path}{name}"),
                        relative: name.clone(),
                        source: Some((root, name, Snapshot::local(&meta))),
                        is_dir: false,
                    })?;
                } else {
                    add(Item {
                        remote: format!("{path}{name}/"),
                        relative: name.clone(),
                        source: None,
                        is_dir: true,
                    })?;
                    for (relative, snapshot) in scan_local(&local)? {
                        if snapshot.kind == "unsupported" {
                            return Err("目录包含符号链接或特殊文件，请移除选择后重试".into());
                        }
                        let is_dir = snapshot.kind == "directory";
                        add(Item {
                            remote: format!(
                                "{path}{name}/{relative}{}",
                                if is_dir { "/" } else { "" }
                            ),
                            relative: format!("{name}/{relative}"),
                            source: (!is_dir).then_some((root, relative, snapshot)),
                            is_dir,
                        })?;
                    }
                }
            }
        } else {
            let local = paths::decode(core, str_arg(args, "local_dir")?)?
                .canonicalize()
                .map_err(err)?;
            plan.destination = Some(LocalDir::open(&local)?);
            let mut selected = strings(args, "paths")?;
            for path in &selected {
                valid_path(path, path.ends_with('/'))?;
                if path.is_empty() {
                    return Err("请选择起始目录中的项目".into());
                }
            }
            selected.sort();
            selected.dedup();
            let mut covered = Vec::<String>::new();
            for path in selected {
                if covered.iter().any(|p| path.starts_with(p)) {
                    continue;
                }
                let name = name(&path).to_owned();
                let is_dir = path.ends_with('/');
                add(Item {
                    remote: path.clone(),
                    relative: name.clone(),
                    source: None,
                    is_dir,
                })?;
                if is_dir {
                    covered.push(path.clone());
                    for (relative, snapshot) in scan_remote(core, config, secret, &path).await? {
                        let is_dir = snapshot.kind == "directory";
                        add(Item {
                            remote: format!("{path}{relative}{}", if is_dir { "/" } else { "" }),
                            relative: format!("{name}/{relative}"),
                            source: None,
                            is_dir,
                        })?;
                    }
                }
            }
        }
        Ok(plan)
    }
}

#[derive(Serialize)]
struct ItemResult {
    path: String,
    is_dir: bool,
    status: &'static str,
    bytes: u64,
    error: Option<String>,
}
struct Report {
    updated: Instant,
    id: String,
    bucket: Option<String>,
    items: Vec<ItemResult>,
    total: usize,
    bytes: u64,
}
impl Report {
    fn summary(&self, token: &str) -> Value {
        let failed = self.items.iter().filter(|r| r.status == "failed").count();
        let completed = self.items.iter().filter(|r| r.status == "done").count();
        json!({"report_id": token, "total": self.total, "completed": completed, "failed": failed, "pending": self.total - completed - failed, "bytes": self.bytes})
    }
}
#[derive(Default)]
pub(super) struct Reports(HashMap<String, Report>);
impl Reports {
    fn insert(&mut self, token: String, report: Report) {
        self.0.retain(|_, r| r.updated.elapsed() < TTL);
        while self.0.len() >= 8
            || self.0.values().map(|r| r.items.len()).sum::<usize>() + report.items.len() > 200_000
        {
            let Some(key) = self
                .0
                .iter()
                .min_by_key(|(_, r)| r.updated)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            self.0.remove(&key);
        }
        self.0.insert(token, report);
    }
}

pub(crate) fn report(core: &Core, args: &Value) -> Result<Value> {
    let mut reports = core.remote.reports.lock().map_err(err)?;
    reports.0.retain(|_, r| r.updated.elapsed() < TTL);
    let token = str_arg(args, "report_id")?;
    let report = reports
        .0
        .get(token)
        .ok_or("传输结果已过期，请查看任务摘要")?;
    if report.id != str_arg(args, "id")? || report.bucket.as_deref() != args["bucket"].as_str() {
        return Err("传输结果不属于此连接".into());
    }
    let offset = args["offset"].as_u64().unwrap_or(0).min(MAX_ENTRIES as u64) as usize;
    let failed_only = args["failed_only"].as_bool().unwrap_or(false);
    let items: Vec<_> = report
        .items
        .iter()
        .filter(|i| !failed_only || i.status == "failed")
        .collect();
    Ok(
        json!({"summary": report.summary(token), "count": items.len(), "items": items.into_iter().skip(offset).take(100).collect::<Vec<_>>()}),
    )
}

pub(crate) fn transfer(core: &Core, args: &Value) -> Result<Value> {
    let upload = str_arg(args, "direction")? == "upload";
    let _slot = upload.then(|| write_slot(&core.remote)).transpose()?;
    let id = str_arg(args, "id")?;
    let revision = args["revision"].as_u64().ok_or("批量传输需要连接版本号")?;
    let (config, secret) = profile::scoped(core, args, upload)?;
    if config.lists_buckets() {
        return Err("请先选择 Bucket，再操作桶内文件".into());
    }
    let plan = block(Transfer::plan(core, args, &config, &secret))?;
    let _progress = Progress;
    let mut report = Report {
        updated: Instant::now(),
        id: id.into(),
        bucket: args["bucket"].as_str().map(str::to_owned),
        items: vec![],
        total: plan.items.len(),
        bytes: 0,
    };
    let mut cancelled = false;
    block(async {
        let mut backend = None;
        let mut blocked = Vec::<String>::new();
        for item in &plan.items {
            if check_cancel().is_err() {
                cancelled = true;
                break;
            }
            // A profile edited or removed during a batch stops subsequent items.
            if let Err(error) = profile::load(core, id, Some(revision), upload) {
                report.items.push(ItemResult {
                    path: item.remote.clone(),
                    is_dir: item.is_dir,
                    status: "failed",
                    bytes: 0,
                    error: Some(error),
                });
                break;
            }
            PROGRESS.with(|p| {
                p.set(Some((
                    report.items.len() as u64,
                    report.total as u64,
                    report.bytes,
                )))
            });
            let result: Result<u64> = async {
                if blocked.iter().any(|p| item.remote.starts_with(p)) {
                    return Err("上级目录传输失败，已跳过此项目".into());
                }
                if backend.is_none() {
                    backend = Some(Backend::connect(core, &config, &secret).await?);
                }
                let client = backend.as_mut().unwrap();
                if item.is_dir {
                    if plan.upload {
                        client.ensure_dir(&item.remote).await?;
                    } else {
                        plan.destination
                            .as_ref()
                            .unwrap()
                            .child(&item.relative, true)?;
                    }
                    Ok(0)
                } else if let Some((root, path, snapshot)) = &item.source {
                    let (mut file, meta) = plan.roots[*root].source(path, snapshot)?;
                    client.upload(&item.remote, &mut file, &meta).await
                } else {
                    plan.destination
                        .as_ref()
                        .unwrap()
                        .download(client, &item.remote, &item.relative)
                        .await
                }
            }
            .await;
            match result {
                Ok(bytes) => {
                    report.bytes = report.bytes.saturating_add(bytes);
                    report.items.push(ItemResult {
                        path: item.remote.clone(),
                        is_dir: item.is_dir,
                        status: "done",
                        bytes,
                        error: None,
                    });
                }
                Err(error) => {
                    backend = None; // An interrupted FTP control connection cannot be reused.
                    if check_cancel().is_err() {
                        cancelled = true;
                        break;
                    }
                    if item.is_dir {
                        blocked.push(item.remote.clone());
                    }
                    report.items.push(ItemResult {
                        path: item.remote.clone(),
                        is_dir: item.is_dir,
                        status: "failed",
                        bytes: 0,
                        error: Some(error),
                    });
                }
            }
            jobs::progress(
                &item.remote,
                report.bytes,
                report.items.len() as u64,
                report.total as u64,
            );
        }
        Ok(())
    })?;
    for item in &plan.items[report.items.len()..] {
        report.items.push(ItemResult {
            path: item.remote.clone(),
            is_dir: item.is_dir,
            status: "unprocessed",
            bytes: 0,
            error: Some(
                if cancelled {
                    "任务已取消，此项目尚未处理"
                } else {
                    "批次已停止，此项目尚未处理"
                }
                .into(),
            ),
        });
    }
    let token = uuid::Uuid::new_v4().to_string();
    let summary = report.summary(&token);
    report.updated = Instant::now();
    core.remote
        .reports
        .lock()
        .map_err(err)?
        .insert(token, report);
    jobs::partial_result(&summary);
    if cancelled {
        return Err("远程传输已取消".into());
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn recursive_local_scan_keeps_links_unsupported_and_rejects_link_parents() {
        let t = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret"), b"keep").unwrap();
        fs::create_dir(t.path().join("normal")).unwrap();
        fs::write(t.path().join("normal/empty"), b"").unwrap();
        symlink(outside.path(), t.path().join("alias")).unwrap();
        let scan = scan_local(t.path()).unwrap();
        assert_eq!(scan["alias"].kind, "unsupported");
        assert_eq!(scan["normal/empty"].size, Some(0));
        assert!(!scan.contains_key("alias/secret"));
        let root = LocalDir::open(t.path()).unwrap();
        assert!(root.child("alias", true).is_err());
        assert!(root.child("../escape", true).is_err());
        assert_eq!(fs::read(outside.path().join("secret")).unwrap(), b"keep");
    }
    #[test]
    fn upload_snapshot_rejects_replacement_before_open() {
        let t = tempfile::tempdir().unwrap();
        let file = t.path().join("file");
        fs::write(&file, b"old").unwrap();
        let snapshot = Snapshot::local(&fs::metadata(&file).unwrap());
        fs::rename(&file, t.path().join("old")).unwrap();
        fs::write(&file, b"new").unwrap();
        assert!(
            LocalDir::open(t.path())
                .unwrap()
                .source("file", &snapshot)
                .is_err()
        );
    }
}
