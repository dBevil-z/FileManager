use crate::{
    Core, Result, err, files,
    operations::{self, Fingerprint},
    paths,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs,
    io::Write,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TrashItem {
    pub original: String,
    pub trashed: String,
    pub info: String,
    pub deleted: String,
    pub fingerprint: Fingerprint,
}
fn private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(err)?;
    let meta = fs::symlink_metadata(path).map_err(err)?;
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } {
        return Err("回收站目录不是当前用户拥有的真实目录".into());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(err)
}
fn mount_root(path: &Path) -> Result<PathBuf> {
    let mut root = path.parent().ok_or("路径没有上级目录")?.to_path_buf();
    let device = fs::metadata(&root).map_err(err)?.dev();
    while let Some(parent) = root.parent() {
        if fs::metadata(parent).map_err(err)?.dev() != device {
            break;
        }
        root = parent.to_path_buf();
    }
    Ok(root)
}
fn trash_for(core: &Core, path: &Path) -> Result<PathBuf> {
    let device = fs::symlink_metadata(path).map_err(err)?.dev();
    private_directory(&core.trash_dir)?;
    if fs::metadata(&core.trash_dir).map_err(err)?.dev() == device {
        return Ok(core.trash_dir.clone());
    }
    let root = mount_root(path)?;
    let shared = root.join(".Trash");
    let uid = unsafe { libc::geteuid() };
    let dir = if fs::symlink_metadata(&shared).is_ok_and(|m| m.is_dir() && m.mode() & 0o1000 != 0) {
        shared.join(uid.to_string())
    } else {
        root.join(format!(".Trash-{uid}"))
    };
    private_directory(&dir)
        .map_err(|e| format!("此磁盘无法创建安全的回收站，源文件已保留：{e}"))?;
    Ok(dir)
}
fn all_trash_dirs(core: &Core) -> Vec<PathBuf> {
    let mut dirs = vec![core.trash_dir.clone()];
    let uid = unsafe { libc::geteuid() };
    for root in paths::mount_points().unwrap_or_default() {
        for dir in [
            root.join(format!(".Trash-{uid}")),
            root.join(".Trash").join(uid.to_string()),
        ] {
            if !dirs.contains(&dir)
                && fs::symlink_metadata(&dir)
                    .is_ok_and(|m| m.is_dir() && m.uid() == uid && m.mode() & 0o077 == 0)
            {
                dirs.push(dir);
            }
        }
    }
    dirs
}
pub fn plan(core: &Core, path: &Path) -> Result<TrashItem> {
    paths::protect(core, path)?;
    paths::reject_mount_tree(path)?;
    let trash_dir = trash_for(core, path)?;
    if path.starts_with(&trash_dir) {
        return Err("不能将回收站本身移入回收站".into());
    }
    for path in [
        &trash_dir,
        &trash_dir.join("files"),
        &trash_dir.join("info"),
    ] {
        private_directory(path)?;
    }
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}", Uuid::new_v4().simple()));
    let mut info_name = name.clone();
    info_name.push(".trashinfo");
    Ok(TrashItem {
        original: paths::encode(path),
        trashed: paths::encode(&trash_dir.join("files").join(name)),
        info: paths::encode(&trash_dir.join("info").join(info_name)),
        deleted: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        fingerprint: operations::fingerprint(path)?,
    })
}
pub fn apply(core: &Core, item: &TrashItem) -> Result<()> {
    let original = paths::decode(core, &item.original)?;
    let trashed = paths::decode(core, &item.trashed)?;
    let info = paths::decode(core, &item.info)?;
    if operations::fingerprint(&original)? != item.fingerprint {
        return Err("移入回收站前文件已经改变，已保留原文件".into());
    }
    let trash_dir = info
        .parent()
        .and_then(Path::parent)
        .ok_or("回收站路径无效")?;
    let stored_path = if trash_dir == core.trash_dir {
        original.clone()
    } else {
        original
            .strip_prefix(mount_root(&original)?)
            .map_err(err)?
            .to_path_buf()
    };
    let text = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        paths::percent_encode(stored_path.as_os_str().as_bytes()),
        item.deleted
    );
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&info)
        .map_err(err)?;
    file.write_all(text.as_bytes()).map_err(err)?;
    file.sync_all().map_err(err)?;
    match operations::move_path(&original, &trashed) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(info);
            Err(e)
        }
    }
}
pub fn restore(core: &Core, item: &TrashItem) -> Result<()> {
    let original = paths::decode(core, &item.original)?;
    let trashed = paths::decode(core, &item.trashed)?;
    let info = paths::decode(core, &item.info)?;
    if !paths::existing(&trashed) && paths::existing(&original) {
        return if operations::fingerprint(&original)? == item.fingerprint {
            Ok(())
        } else {
            Err("原位置已有其他内容，已保留".into())
        };
    }
    if paths::existing(&original) {
        return Err(format!(
            "原位置 {} 已有文件，回收站中的文件已保留",
            original.display()
        ));
    }
    if !original.parent().is_some_and(Path::is_dir) {
        return Err("原文件夹已不存在，请选择其他恢复位置".into());
    }
    if operations::fingerprint(&trashed)? != item.fingerprint {
        return Err("回收站中的文件已被修改，予以保留".into());
    }
    operations::move_path(&trashed, &original)?;
    if info.exists() {
        fs::remove_file(info).map_err(err)?;
    }
    Ok(())
}
fn read_items(core: &Core) -> Result<Vec<TrashItem>> {
    let mut items = Vec::new();
    for trash_dir in all_trash_dirs(core) {
        let dir = trash_dir.join("info");
        if !dir.exists() {
            continue;
        }
        for item in fs::read_dir(dir).map_err(err)?.flatten() {
            let info = item.path();
            if info.extension().and_then(|s| s.to_str()) != Some("trashinfo") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&info) else {
                continue;
            };
            let Some(original) = text.lines().find_map(|l| l.strip_prefix("Path=")) else {
                continue;
            };
            let Ok(bytes) = paths::percent_decode(original) else {
                continue;
            };
            let mut original = PathBuf::from(OsString::from_vec(bytes));
            if !original.is_absolute() {
                if trash_dir == core.trash_dir
                    || original
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    continue;
                }
                let Some(mut root) = trash_dir.parent() else {
                    continue;
                };
                if root.file_name().is_some_and(|s| s == ".Trash") {
                    let Some(parent) = root.parent() else {
                        continue;
                    };
                    root = parent;
                }
                original = root.join(original);
            }
            let Some(name) = info.file_stem() else {
                continue;
            };
            let trashed = trash_dir.join("files").join(name);
            let Ok(fingerprint) = operations::fingerprint(&trashed) else {
                continue;
            };
            items.push(TrashItem {
                original: paths::encode(&original),
                trashed: paths::encode(&trashed),
                info: paths::encode(&info),
                deleted: text
                    .lines()
                    .find_map(|l| l.strip_prefix("DeletionDate="))
                    .unwrap_or("")
                    .into(),
                fingerprint,
            });
        }
    }
    items.sort_by(|a, b| b.deleted.cmp(&a.deleted));
    Ok(items)
}
pub fn list(core: &Core) -> Result<Value> {
    let items = read_items(core)?;
    let mut size = 0u64;
    let mut result = Vec::new();
    for item in items {
        let original = paths::decode(core, &item.original)?;
        let mut entry = files::entry(&paths::decode(core, &item.trashed)?)?;
        entry.name = original
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        size += entry.size.unwrap_or(0);
        result.push(json!({"item":item,"entry":entry}));
    }
    Ok(json!({"items":result,"size":size,"path":paths::encode(&core.trash_dir)}))
}
pub fn restore_request(core: &Core, args: &Value) -> Result<Value> {
    let info = crate::str_arg(args, "info")?;
    let item = read_items(core)?
        .into_iter()
        .find(|i| i.info == info)
        .ok_or("此回收站项目不存在")?;
    if let Some(destination) = args["destination"].as_str() {
        let target = paths::decode(core, destination)?.join(
            paths::decode(core, &item.original)?
                .file_name()
                .ok_or("无效的原文件名")?,
        );
        let source = paths::decode(core, &item.trashed)?;
        let target = paths::unique(&target);
        paths::reject_mount_tree(&source)?;
        if operations::fingerprint(&source)? != item.fingerprint {
            return Err("回收站中的文件已被修改，予以保留".into());
        }
        match operations::rename_noreplace(&source, &target) {
            Ok(()) => fs::remove_file(paths::decode(core, &item.info)?).map_err(err)?,
            Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
                // Keep the trash original until it is explicitly cleared.
                // Publishing the restored copy does not depend on destructive
                // recursive cleanup in the old filesystem.
                let staging =
                    target.with_file_name(format!(".filemanager-{}.restore", Uuid::new_v4()));
                let result = crate::file_copy::copy(&source, &staging, true).and_then(|_| {
                    if operations::fingerprint(&source)? != item.fingerprint {
                        return Err("回收站中的文件已改变，予以保留".into());
                    }
                    operations::rename_noreplace(&staging, &target).map_err(err)
                });
                if let Err(e) = result {
                    let _ = operations::discard_stage(&staging);
                    return Err(e);
                }
                return Ok(
                    json!({"restored":true,"trash_copy_retained":true,"path":paths::encode(&target)}),
                );
            }
            Err(e) => return Err(err(e)),
        }
    } else {
        restore(core, &item)?;
    }
    Ok(json!({"restored":true}))
}
pub fn empty(core: &Core, args: &Value) -> Result<Value> {
    if args["confirmation"].as_str() != Some("EMPTY") {
        return Err("清空回收站必须明确确认，此操作不可撤销".into());
    }
    let items = read_items(core)?;
    let mut count = 0;
    let mut errors = Vec::new();
    for item in items {
        let result = operations::remove_path(&paths::decode(core, &item.trashed)?);
        match result {
            Ok(()) => {
                let _ = fs::remove_file(paths::decode(core, &item.info)?);
                count += 1;
            }
            Err(e) => errors.push(e),
        }
    }
    Ok(json!({"count":count,"errors":errors}))
}
