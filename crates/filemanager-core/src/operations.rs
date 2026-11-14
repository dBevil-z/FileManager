use crate::{Core, Result, err, files, paths, str_arg};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    io::Write,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Fingerprint {
    pub device: u64,
    pub inode: u64,
    pub length: u64,
    pub modified: i64,
    pub nanos: i64,
    pub tree: Option<String>,
}
pub fn fingerprint(path: &Path) -> Result<Fingerprint> {
    let meta = fs::symlink_metadata(path).map_err(err)?;
    let tree = if meta.is_dir() {
        let mut rows = Vec::new();
        for item in WalkDir::new(path).follow_links(false).max_open(16) {
            let item = item.map_err(err)?;
            let m = fs::symlink_metadata(item.path()).map_err(err)?;
            rows.push((
                item.path()
                    .strip_prefix(path)
                    .map_err(err)?
                    .as_os_str()
                    .as_bytes()
                    .to_vec(),
                m.ino(),
                m.len(),
                m.mtime(),
                m.mtime_nsec(),
            ));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        let mut hash = Sha256::new();
        for (name, ino, len, mtime, nanos) in rows {
            hash.update(name.len().to_le_bytes());
            hash.update(name);
            hash.update(ino.to_le_bytes());
            hash.update(len.to_le_bytes());
            hash.update(mtime.to_le_bytes());
            hash.update(nanos.to_le_bytes());
        }
        Some(format!("{:x}", hash.finalize()))
    } else {
        None
    };
    Ok(Fingerprint {
        device: meta.dev(),
        inode: meta.ino(),
        length: meta.len(),
        modified: meta.mtime(),
        nanos: meta.mtime_nsec(),
        tree,
    })
}

fn replacement_version(path: &Path) -> Result<String> {
    let meta = fs::symlink_metadata(path).map_err(err)?;
    let bytes =
        serde_json::to_vec(&(fingerprint(path)?, meta.ctime(), meta.ctime_nsec())).map_err(err)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UndoStep {
    EmptyDir {
        path: String,
        inode: u64,
        device: u64,
    },
    Remove {
        path: String,
        fallback: Option<String>,
        fingerprint: Fingerprint,
    },
    Move {
        from: String,
        to: String,
        fallback: Option<String>,
        fingerprint: Fingerprint,
    },
    Trash {
        item: crate::trash::TrashItem,
    },
    CrossVolumeMove {
        item: crate::trash::TrashItem,
        target: String,
        staging: String,
        fingerprint: Fingerprint,
    },
    Metadata {
        namespace: String,
        before: Value,
        after_revision: i64,
        #[serde(default)]
        after_value: Option<Value>,
    },
    Write {
        path: String,
        backup: String,
        fingerprint: Fingerprint,
        #[serde(default)]
        before: Option<Fingerprint>,
        #[serde(default)]
        fallback: Option<String>,
    },
}
pub struct Journal<'a> {
    core: &'a Core,
    pub id: String,
}
impl<'a> Journal<'a> {
    pub fn begin(core: &'a Core, label: &str) -> Result<Self> {
        let id = Uuid::new_v4().to_string();
        core.connection()?.execute("INSERT INTO journal(id,label,payload,status,created) VALUES(?1,?2,'[]','running',?3)",params![id,label,chrono::Utc::now().timestamp_millis()]).map_err(err)?;
        Ok(Self { core, id })
    }
    pub fn record(&self, steps: &[UndoStep]) -> Result<()> {
        self.set(steps, "running")
    }
    fn set(&self, steps: &[UndoStep], status: &str) -> Result<()> {
        self.core
            .connection()?
            .execute(
                "UPDATE journal SET payload=?1,status=?2 WHERE id=?3",
                params![serde_json::to_string(steps).map_err(err)?, status, self.id],
            )
            .map_err(err)?;
        Ok(())
    }
    pub fn finish(&self, steps: Vec<UndoStep>) -> Result<()> {
        self.set(&steps, if steps.is_empty() { "empty" } else { "ready" })
    }
}

pub fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
    rename_with_flags(from, to, libc::RENAME_NOREPLACE)
}
fn rename_with_flags(from: &Path, to: &Path, flags: u32) -> std::io::Result<()> {
    let a = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let b = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    let rc = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            a.as_ptr(),
            libc::AT_FDCWD,
            b.as_ptr(),
            flags,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
pub fn remove_path(path: &Path) -> Result<()> {
    paths::reject_mount_tree(path)?;
    let meta = fs::symlink_metadata(path).map_err(err)?;
    if meta.is_dir() {
        fs::remove_dir_all(path).map_err(err)
    } else {
        fs::remove_file(path).map_err(err)
    }
}

pub(crate) fn discard_stage(path: &Path) -> Result<()> {
    paths::reject_mount_tree(path)?;
    let uid = unsafe { libc::geteuid() };
    // Only for a private, unpublished copy. Never change permissions on source
    // entries to make a cross-volume move finish.
    for entry in WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(err)?;
        if entry.file_type().is_dir() {
            let meta = fs::symlink_metadata(entry.path()).map_err(err)?;
            if meta.uid() == uid && meta.mode() & 0o700 != 0o700 {
                fs::set_permissions(
                    entry.path(),
                    fs::Permissions::from_mode(meta.mode() | 0o700),
                )
                .map_err(err)?;
            }
        }
    }
    remove_path(path)
}

pub fn copy_path(from: &Path, to: &Path) -> Result<()> {
    crate::file_copy::copy(from, to, false)
}

pub fn move_path(from: &Path, to: &Path) -> Result<()> {
    rename_noreplace(from, to).map_err(err)
}

/// A cross-volume move copies to an unpublished entry, retains the original
/// inode in the source volume's trash, and then publishes the copy. The
/// journal lets undo recover from an interruption between those steps.
pub fn move_logged(
    core: &Core,
    source: &Path,
    target: &Path,
    journal: &Journal<'_>,
    steps: &mut Vec<UndoStep>,
) -> Result<()> {
    paths::reject_mount_tree(source)?;
    let before = fingerprint(source)?;
    steps.push(UndoStep::Move {
        from: paths::encode(target),
        to: paths::encode(source),
        fallback: None,
        fingerprint: before.clone(),
    });
    journal.record(steps)?;
    match rename_noreplace(source, target) {
        Ok(()) => return Ok(()),
        Err(e) => {
            steps.pop();
            journal.record(steps)?;
            if e.raw_os_error() != Some(libc::EXDEV) {
                return Err(err(e));
            }
        }
    }
    let staging = target.with_file_name(format!(".filemanager-{}.part", Uuid::new_v4()));
    let item = crate::trash::plan(core, source)?;
    let staged = (|| {
        if item.fingerprint != before {
            return Err("源文件已改变，已保留原文件".into());
        }
        crate::file_copy::copy(source, &staging, true)?;
        if fingerprint(source)? != before {
            return Err("复制期间源文件已改变，已保留原文件".into());
        }
        crate::jobs::checkpoint()?;
        let copied = fingerprint(&staging)?;
        steps.push(UndoStep::CrossVolumeMove {
            item: item.clone(),
            target: paths::encode(target),
            staging: paths::encode(&staging),
            fingerprint: copied,
        });
        journal.record(steps)
    })();
    if let Err(e) = staged {
        let _ = discard_stage(&staging);
        return Err(e);
    }
    crate::trash::apply(core, &item)?;
    if let Err(e) = rename_noreplace(&staging, target) {
        crate::trash::restore(core, &item)?;
        return Err(err(e));
    }
    Ok(())
}
fn source_paths(core: &Core, args: &Value) -> Result<Vec<PathBuf>> {
    let values = args["paths"].as_array().ok_or("没有选择文件")?;
    if values.is_empty() {
        return Err("没有选择文件".into());
    }
    if values.len() > 20000 {
        return Err("一次最多操作 20000 个项目".into());
    }
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    let mut directories = HashSet::new();
    for value in values {
        let p = paths::decode(core, value.as_str().ok_or("无效路径")?)?;
        paths::protect(core, &p)?;
        let p = paths::canonical_entry(&p)?;
        let meta = fs::symlink_metadata(&p).map_err(err)?;
        if seen.insert(p.clone()) {
            if meta.is_dir() {
                directories.insert(p.clone());
            }
            paths.push(p);
        }
    }
    paths.retain(|p| !p.ancestors().skip(1).any(|a| directories.contains(a)));
    Ok(paths)
}

pub fn operate(core: &Core, args: &Value) -> Result<Value> {
    let kind = str_arg(args, "kind")?;
    if !matches!(kind, "copy" | "move" | "trash" | "duplicate" | "symlink") {
        return Err("不支持的文件操作".into());
    }
    let sources = source_paths(core, args)?;
    let destination = if kind == "trash" || kind == "duplicate" {
        None
    } else {
        let p = paths::decode(core, str_arg(args, "destination")?)?;
        if !p.is_dir() {
            return Err("目标不是可访问的文件夹".into());
        }
        Some(fs::canonicalize(&p).map_err(err)?)
    };
    let conflict = args["conflict"].as_str().unwrap_or("ask");
    let resolutions: HashMap<_, _> = args["resolutions"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(path, policy)| {
            let p = paths::canonical_entry(&paths::decode(core, path).ok()?).ok()?;
            Some((p, policy.as_str()?))
        })
        .collect();
    let versions = &args["replacement_versions"];
    let mut plan = Vec::new();
    let mut conflicts = Vec::new();
    let mut reserved = HashMap::<PathBuf, PathBuf>::new();
    let selected = sources.iter().cloned().collect::<BTreeSet<_>>();
    for source in sources {
        if kind == "trash" {
            plan.push((source, None));
            continue;
        }
        let parent = if kind == "duplicate" {
            source.parent().ok_or("文件缺少上级目录")?
        } else {
            destination.as_deref().ok_or("缺少目标目录")?
        };
        let mut target = parent.join(source.file_name().ok_or("无效文件名")?);
        if kind == "duplicate" {
            target = paths::unique_avoiding(&target, |p| reserved.contains_key(p));
        }
        if source == target && kind == "move" {
            continue;
        }
        let meta = fs::symlink_metadata(&source).map_err(err)?;
        if meta.is_dir() && parent.starts_with(&source) {
            return Err("不能把文件夹复制或移动到自身及其子文件夹中".into());
        }
        let mut replace = None;
        let item_policy = resolutions.get(&source).copied().unwrap_or(conflict);
        let version = versions[paths::encode(&source)].as_str();
        if item_policy == "replace"
            && let Some(version) = version
        {
            if replacement_version(&target).ok().as_deref() != Some(version) {
                return Err("替换目标已改变，请重新确认".into());
            }
        }
        if paths::existing(&target) || reserved.contains_key(&target) {
            let planned = reserved.get(&target);
            let overlaps_source = selected
                .range(target.clone()..)
                .next()
                .is_some_and(|p| p.starts_with(&target));
            let can_replace = planned.is_none() && !overlaps_source;
            match item_policy {
                "skip" => continue,
                "keep_both" => target = paths::unique_avoiding(&target, |p| reserved.contains_key(p)),
                "replace" if planned.is_some() => return Err("同批文件使用同一目标名称，请保留两者或跳过".into()),
                "replace" if can_replace => replace = Some(replacement_version(&target)?),
                "replace" => return Err("目标是选中的源文件或其上级目录，请选择保留两者或跳过".into()),
                _ => conflicts.push(
                    json!({"source":files::entry(&source)?,"destination":files::entry(planned.unwrap_or(&target))?,"can_replace":can_replace,"replacement_version":can_replace.then(|| replacement_version(&target)).transpose()?}),
                ),
            }
        }
        reserved.insert(target.clone(), source.clone());
        plan.push((source, Some((target, replace))));
    }
    if !conflicts.is_empty() {
        return Ok(json!({"requires_confirmation":true,"conflicts":conflicts}));
    }
    if args["dry_run"] == true {
        return Ok(json!({"requires_confirmation":false,"count":plan.len()}));
    }
    let journal = Journal::begin(
        core,
        match kind {
            "copy" | "duplicate" => "复制文件",
            "move" => "移动文件",
            "trash" => "移到回收站",
            _ => "创建符号链接",
        },
    )?;
    let mut steps = Vec::new();
    let mut completed = Vec::new();
    let total = plan.len() as u64;
    for (index, (source, target)) in plan.into_iter().enumerate() {
        crate::jobs::checkpoint()?;
        crate::jobs::progress(&source.to_string_lossy(), 0, index as u64, total);
        if kind == "trash" {
            let item = crate::trash::plan(core, &source)?;
            steps.push(UndoStep::Trash { item: item.clone() });
            journal.record(&steps)?;
            crate::trash::apply(core, &item)?;
            completed.push(json!({"from":paths::encode(&source),"to":null}));
            continue;
        }
        let (target, replace) = target.ok_or("缺少目标路径")?;
        if let Some(expected) = replace {
            let item = crate::trash::plan(core, &target)?;
            if replacement_version(&target)? != expected {
                return Err(format!("{} 在操作前已改变，已保留", target.display()));
            }
            steps.push(UndoStep::Trash { item: item.clone() });
            journal.record(&steps)?;
            crate::trash::apply(core, &item)?;
        } else if paths::existing(&target) {
            return Err(format!("{} 在操作前已出现，已保留", target.display()));
        }
        if kind == "move" {
            move_logged(core, &source, &target, &journal, &mut steps)?;
        } else {
            let staging = target
                .parent()
                .ok_or("缺少目标目录")?
                .join(format!(".filemanager-{}.part", Uuid::new_v4()));
            let result = if kind == "symlink" {
                std::os::unix::fs::symlink(&source, &staging).map_err(err)
            } else {
                copy_path(&source, &staging)
            };
            if let Err(e) = result {
                let _ = discard_stage(&staging);
                return Err(e);
            }
            steps.push(UndoStep::Remove {
                path: paths::encode(&target),
                fallback: Some(paths::encode(&staging)),
                fingerprint: fingerprint(&staging)?,
            });
            journal.record(&steps)?;
            rename_noreplace(&staging, &target).map_err(err)?;
        }
        completed.push(json!({"from":paths::encode(&source),"to":paths::encode(&target)}));
    }
    journal.finish(steps)?;
    Ok(
        json!({"id":journal.id,"completed":completed,"count":completed.len(),"requires_confirmation":false}),
    )
}

pub fn create_folder(core: &Core, args: &Value) -> Result<Value> {
    let parent = paths::decode(core, str_arg(args, "parent")?)?;
    let name = str_arg(args, "name")?;
    paths::check_name(name)?;
    let path = parent.join(name);
    fs::create_dir(&path).map_err(err)?;
    let metadata = fs::symlink_metadata(&path).map_err(err)?;
    let journal = Journal::begin(core, "新建文件夹")?;
    journal.finish(vec![UndoStep::EmptyDir {
        path: paths::encode(&path),
        inode: metadata.ino(),
        device: metadata.dev(),
    }])?;
    Ok(json!({"entry":files::entry(&path)?,"id":journal.id}))
}
pub fn rename_plan(core: &Core, args: &Value) -> Result<Value> {
    let sources = source_paths(core, args)?;
    let mode = args["mode"].as_str().unwrap_or("replace");
    let text = args["text"].as_str().unwrap_or("");
    let find = args["find"].as_str().unwrap_or("");
    let start = args["start"].as_u64().unwrap_or(1);
    let pad = args["padding"].as_u64().unwrap_or(2).min(10) as usize;
    let keep_extension = args["keep_extension"] != false;
    let mut items = Vec::new();
    let mut used = std::collections::HashSet::new();
    let mut valid = true;
    for (i, source) in sources.iter().enumerate() {
        let old = source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let stem = if source.is_dir() || !keep_extension {
            old.clone()
        } else {
            source
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        };
        let new_stem = match mode {
            "number" => format!("{text} {:0pad$}", start + i as u64),
            "prefix" => format!("{text}{stem}"),
            "suffix" => format!("{stem}{text}"),
            "name" => text.into(),
            _ => {
                if find.is_empty() {
                    stem.clone()
                } else {
                    stem.replace(find, text)
                }
            }
        };
        let name = if mode == "name" || !keep_extension {
            new_stem
        } else if let Some(ext) = source.extension().filter(|_| !source.is_dir()) {
            format!("{new_stem}.{}", ext.to_string_lossy())
        } else {
            new_stem
        };
        let target = source.parent().ok_or("缺少上级目录")?.join(&name);
        let error = paths::check_name(&name).err().or_else(|| {
            if !used.insert(target.clone()) {
                Some("多个项目将得到同一名称".into())
            } else if paths::existing(&target) && !sources.contains(&target) {
                Some("此名称已经存在".into())
            } else {
                None
            }
        });
        if error.is_some() {
            valid = false;
        }
        items.push(json!({"path":paths::encode(source),"name":name,"old_name":old,"target":paths::encode(&target),"error":error}));
    }
    Ok(json!({"items":items,"valid":valid}))
}
pub fn rename_apply(core: &Core, args: &Value) -> Result<Value> {
    let items = args["items"].as_array().ok_or("缺少重命名计划")?;
    if items.len() > 20000 {
        return Err("项目数量过多".into());
    }
    let mut plan = Vec::new();
    let mut targets = std::collections::HashSet::new();
    let sources: Vec<_> = items
        .iter()
        .map(|item| paths::decode(core, str_arg(item, "path")?))
        .collect::<Result<_>>()?;
    for item in items {
        let source = paths::decode(core, str_arg(item, "path")?)?;
        paths::protect(core, &source)?;
        let name = str_arg(item, "name")?;
        paths::check_name(name)?;
        let target = source.parent().ok_or("缺少目录")?.join(name);
        if !targets.insert(target.clone()) || paths::existing(&target) && !sources.contains(&target)
        {
            return Err("重命名计划存在名称冲突".into());
        }
        if source != target {
            let stage = source
                .parent()
                .unwrap()
                .join(format!(".filemanager-{}.rename", Uuid::new_v4()));
            plan.push((source, target, stage));
        }
    }
    let journal = Journal::begin(core, "批量重命名")?;
    let mut steps = Vec::new();
    for (source, _, stage) in &plan {
        steps.push(UndoStep::Move {
            from: paths::encode(stage),
            to: paths::encode(source),
            fallback: None,
            fingerprint: fingerprint(source)?,
        });
        journal.record(&steps)?;
        rename_noreplace(source, stage).map_err(err)?;
    }
    for (i, (_, target, stage)) in plan.iter().enumerate() {
        if let UndoStep::Move { from, fallback, .. } = &mut steps[i] {
            *from = paths::encode(target);
            *fallback = Some(paths::encode(stage));
        }
        journal.record(&steps)?;
        rename_noreplace(stage, target).map_err(err)?;
    }
    journal.finish(steps)?;
    Ok(json!({"id":journal.id,"count":plan.len()}))
}

pub fn write_text(core: &Core, args: &Value) -> Result<Value> {
    let requested = paths::decode(core, str_arg(args, "path")?)?;
    let path = if paths::existing(&requested) {
        fs::canonicalize(&requested).map_err(err)?
    } else {
        requested
    };
    let text = str_arg(args, "text")?;
    if text.len() > 5 * 1024 * 1024 {
        return Err("文本不能超过 5 MB".into());
    }
    let exists = paths::existing(&path);
    if !exists && args["revision"].as_str().is_some_and(|r| !r.is_empty()) {
        return Err("原文件已被移动或删除，请重新打开文件或另存为".into());
    }
    let before = exists.then(|| fingerprint(&path)).transpose()?;
    let current = if exists {
        Some(files::read_text(
            core,
            &json!({"path":paths::encode(&path)}),
        )?)
    } else {
        None
    };
    if let Some(current) = &current {
        if args["revision"] != current["revision"] {
            return Err("文件已被其他应用修改；请重新载入或另存为".into());
        }
    }
    let parent = path.parent().ok_or("缺少文件夹")?;
    let temp = parent.join(format!(".filemanager-{}.save", Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(err)?;
    file.write_all(text.as_bytes()).map_err(err)?;
    file.sync_all().map_err(err)?;
    if exists {
        crate::file_copy::metadata(&path, &temp, &fs::metadata(&path).map_err(err)?, true)?;
        file.sync_all().map_err(err)?;
    }
    let journal = Journal::begin(core, "保存文本文件")?;
    let steps = if let Some(before) = before.as_ref() {
        let backups = core.data_dir.join("backups");
        fs::create_dir_all(&backups).map_err(err)?;
        let backup = if fs::metadata(&backups).map_err(err)?.dev() == before.device {
            backups.join(Uuid::new_v4().to_string())
        } else {
            parent.join(format!(".filemanager-{}.backup", Uuid::new_v4()))
        };
        vec![UndoStep::Write {
            path: paths::encode(&path),
            backup: paths::encode(&backup),
            fingerprint: fingerprint(&temp)?,
            before: Some(before.clone()),
            fallback: Some(paths::encode(&temp)),
        }]
    } else {
        vec![UndoStep::Remove {
            path: paths::encode(&path),
            fallback: Some(paths::encode(&temp)),
            fingerprint: fingerprint(&temp)?,
        }]
    };
    journal.record(&steps)?;
    if let UndoStep::Write {
        before: Some(before),
        backup,
        fingerprint: after,
        ..
    } = &steps[0]
    {
        if fingerprint(&path)? != *before
            || files::read_text(core, &json!({"path":paths::encode(&path)}))?["revision"]
                != args["revision"]
        {
            let _ = fs::remove_file(&temp);
            return Err("文件已被其他应用修改；请重新载入或另存为".into());
        }
        // Save atomically while retaining the actual previous inode for undo.
        rename_with_flags(&temp, &path, libc::RENAME_EXCHANGE).map_err(err)?;
        if fingerprint(&temp)? != *before {
            if fingerprint(&path)? == *after {
                rename_with_flags(&temp, &path, libc::RENAME_EXCHANGE).map_err(err)?;
            }
            return Err("保存时文件已改变，原内容和编辑稿均已保留".into());
        }
        if let Err(error) = rename_noreplace(&temp, &paths::decode(core, backup)?) {
            if error.raw_os_error() != Some(libc::EXDEV) {
                return Err(err(error));
            }
            // A bind mount may share st_dev. The adjacent journal fallback
            // still contains the original inode.
        }
    } else {
        rename_noreplace(&temp, &path).map_err(err)?;
    }
    journal.finish(steps)?;
    files::read_text(core, &json!({"path":paths::encode(&path)}))
}

fn apply_undo(core: &Core, step: &UndoStep) -> Result<()> {
    match step {
        UndoStep::EmptyDir {
            path,
            inode,
            device,
        } => {
            let p = paths::decode(core, path)?;
            if !p.exists() {
                return Ok(());
            }
            let m = fs::symlink_metadata(&p).map_err(err)?;
            if m.ino() != *inode || m.dev() != *device {
                return Err("目录已被替换，予以保留".into());
            }
            fs::remove_dir(p).map_err(err)
        }
        UndoStep::Remove {
            path,
            fallback,
            fingerprint: expected,
        } => {
            let path = paths::decode(core, path)?;
            let present = paths::existing(&path);
            if !present || fingerprint(&path)? != *expected {
                if let Some(fallback) = fallback {
                    let stage = paths::decode(core, fallback)?;
                    if paths::existing(&stage) && fingerprint(&stage)? == *expected {
                        return discard_stage(&stage);
                    }
                }
                if !present {
                    return Ok(());
                }
                return Err(format!("{} 已被修改，已保留", path.display()));
            }
            if fingerprint(&path)? != *expected {
                return Err(format!("{} 已被修改，已保留", path.display()));
            }
            let item = crate::trash::plan(core, &path)?;
            crate::trash::apply(core, &item)
        }
        UndoStep::Move {
            from,
            to,
            fallback,
            fingerprint: expected,
        } => {
            let mut from = paths::decode(core, from)?;
            let to = paths::decode(core, to)?;
            if !paths::existing(&from) {
                if let Some(fallback) = fallback {
                    let stage = paths::decode(core, fallback)?;
                    if paths::existing(&stage) {
                        from = stage;
                    }
                }
                if !paths::existing(&from) && paths::existing(&to) && fingerprint(&to)? == *expected
                {
                    return Ok(());
                }
            }
            if paths::existing(&to) {
                return Err(format!("原位置 {} 已有文件，已保留两者", to.display()));
            }
            if fingerprint(&from)? != *expected {
                return Err(format!("{} 已被修改，已保留", from.display()));
            }
            move_path(&from, &to)
        }
        UndoStep::Trash { item } => crate::trash::restore(core, item),
        UndoStep::CrossVolumeMove {
            item,
            target,
            staging,
            fingerprint: expected,
        } => {
            let target = paths::decode(core, target)?;
            let staging = paths::decode(core, staging)?;
            let original = paths::decode(core, &item.original)?;
            let trashed = paths::decode(core, &item.trashed)?;
            let stage_matches = paths::existing(&staging) && fingerprint(&staging)? == *expected;
            let target_matches = paths::existing(&target) && fingerprint(&target)? == *expected;
            if paths::existing(&target) && !target_matches && !stage_matches {
                return Err(format!(
                    "{} 已被修改，已保留目标和原件备份",
                    target.display()
                ));
            }
            if paths::existing(&trashed) && fingerprint(&trashed)? != item.fingerprint {
                return Err("回收站中的原件已改变，已保留两者".into());
            }
            crate::trash::restore(core, item)?;
            if fingerprint(&original)? != item.fingerprint {
                return Err("无法验证恢复的原件，目标副本已保留".into());
            }
            if target_matches {
                if fingerprint(&target)? != *expected {
                    return Err("目标副本已改变，予以保留".into());
                }
                rename_noreplace(&target, &staging).map_err(err)?;
            }
            if (stage_matches || target_matches) && paths::existing(&staging) {
                if fingerprint(&staging)? != *expected {
                    let _ = rename_noreplace(&staging, &target);
                    return Err("目标副本已改变，予以保留".into());
                }
                // Discard the verified duplicate only after restoring the original.
                if let Err(error) = discard_stage(&staging) {
                    return Err(format!("原件已恢复，暂存副本尚未清理：{error}"));
                }
            }
            Ok(())
        }
        UndoStep::Metadata {
            namespace,
            before,
            after_revision,
            after_value,
        } => {
            let current = core.get_value(namespace)?;
            if let Some(after) = after_value {
                if current["data"] != *after {
                    return Err("设置后来已被修改，保留当前内容".into());
                }
            } else if current["revision"].as_i64() != Some(*after_revision) {
                return Err("设置版本已改变，保留当前内容".into());
            }
            core.put_value(namespace, before, current["revision"].as_i64())?;
            Ok(())
        }
        UndoStep::Write {
            path,
            backup,
            fingerprint: expected,
            before,
            fallback,
        } => {
            let path = paths::decode(core, path)?;
            let mut backup = paths::decode(core, backup)?;
            if let Some(before) = before {
                let current = fingerprint(&path)?;
                if current == *before {
                    return Ok(());
                }
                if !paths::existing(&backup)
                    && let Some(fallback) = fallback
                {
                    backup = paths::decode(core, fallback)?;
                }
                if current != *expected || fingerprint(&backup)? != *before {
                    return Err(format!(
                        "{} 后来又被修改，已保留当前内容和备份",
                        path.display()
                    ));
                }
                rename_with_flags(&backup, &path, libc::RENAME_EXCHANGE).map_err(err)?;
                // The newer version remains recoverable in the private backup.
                return Ok(());
            }
            fs::symlink_metadata(&backup).map_err(err)?;
            if fingerprint(&path)? != *expected {
                return Err(format!(
                    "{} 后来又被修改，已保留当前内容和备份",
                    path.display()
                ));
            }
            let item = crate::trash::plan(core, &path)?;
            crate::trash::apply(core, &item)?;
            move_path(&backup, &path)
        }
    }
}
pub fn defer_undo(core: &Core, args: &Value) -> Result<Value> {
    let id = str_arg(args, "id")?;
    let changed = core
        .connection()?
        .execute(
            "UPDATE journal SET status='deferred' WHERE id=?1 AND status='partial'",
            [id],
        )
        .map_err(err)?;
    Ok(json!({"ok":changed == 1}))
}

pub fn undo(core: &Core, args: &Value) -> Result<Value> {
    let conn = core.connection()?;
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT id,label,payload FROM journal WHERE
        (?1 IS NULL AND status IN ('ready','running','partial')) OR
        (?1 IS NOT NULL AND id=?1 AND status IN ('running','partial','deferred'))
        ORDER BY created DESC,rowid DESC LIMIT 1",
            [args["id"].as_str()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(err)?;
    let Some((id, label, payload)) = row else {
        return Ok(json!({"count":0,"conflicts":[],"message":"没有可撤销的操作"}));
    };
    let steps: Vec<UndoStep> = serde_json::from_str(&payload).map_err(err)?;
    if label == "批量重命名" && steps.iter().all(|s| matches!(s, UndoStep::Move { .. })) {
        return undo_rename_batch(core, &id, &label, steps);
    }
    let mut remaining = Vec::new();
    let mut conflicts = Vec::new();
    let mut count = 0;
    for step in steps.iter().rev() {
        match apply_undo(core, step) {
            Ok(()) => count += 1,
            Err(e) => {
                conflicts.push(e);
                remaining.push(step.clone());
            }
        }
    }
    remaining.reverse();
    conn.execute(
        "UPDATE journal SET status=?1,payload=?2 WHERE id=?3",
        params![
            if conflicts.is_empty() {
                "undone"
            } else {
                "partial"
            },
            serde_json::to_string(&remaining).map_err(err)?,
            id
        ],
    )
    .map_err(err)?;
    Ok(json!({"id":id,"label":label,"count":count,"conflicts":conflicts}))
}

/// Stage the whole rename set before restoring names. This also resumes an
/// interrupted forward rename or undo, without replacing unrelated files.
fn undo_rename_batch(
    core: &Core,
    id: &str,
    label: &str,
    mut steps: Vec<UndoStep>,
) -> Result<Value> {
    let mut resolved = Vec::new();
    let mut conflicts = Vec::new();
    for step in &steps {
        let UndoStep::Move {
            from,
            to,
            fallback,
            fingerprint: expected,
        } = step
        else {
            unreachable!()
        };
        let to_path = paths::decode(core, to)?;
        let candidates = [Some(from.as_str()), fallback.as_deref(), Some(to.as_str())];
        let source = candidates
            .into_iter()
            .flatten()
            .filter_map(|p| paths::decode(core, p).ok())
            .find(|p| fingerprint(p).is_ok_and(|fp| fp == *expected));
        match source {
            Some(source) => resolved.push((source, to_path)),
            None => conflicts.push(format!("{from} 已被修改或移走，保留当前文件")),
        }
    }
    if conflicts.is_empty() {
        for (_, target) in &resolved {
            if paths::existing(target) && !resolved.iter().any(|(source, _)| source == target) {
                conflicts.push(format!("原位置 {} 已有新文件，保留两者", target.display()));
            }
        }
    }
    if !conflicts.is_empty() {
        core.connection()?
            .execute("UPDATE journal SET status='partial' WHERE id=?1", [id])
            .map_err(err)?;
        return Ok(json!({"id":id,"label":label,"count":0,"conflicts":conflicts}));
    }
    let journal = Journal {
        core,
        id: id.into(),
    };
    for (i, (source, target)) in resolved.iter().enumerate() {
        if source == target {
            continue;
        }
        let stage = source
            .parent()
            .ok_or("缺少目录")?
            .join(format!(".filemanager-{}.undo", Uuid::new_v4()));
        if let UndoStep::Move { from, fallback, .. } = &mut steps[i] {
            *from = paths::encode(&stage);
            *fallback = Some(paths::encode(source));
        }
        journal.record(&steps)?;
        rename_noreplace(source, &stage).map_err(err)?;
    }
    for (i, step) in steps.iter().enumerate() {
        if resolved[i].0 != resolved[i].1 {
            apply_undo(core, step)?;
        }
    }
    core.connection()?
        .execute(
            "UPDATE journal SET status='undone',payload='[]' WHERE id=?1",
            [id],
        )
        .map_err(err)?;
    Ok(json!({"id":id,"label":label,"count":steps.len(),"conflicts":[]}))
}
