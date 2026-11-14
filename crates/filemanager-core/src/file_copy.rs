use crate::{Result, err, jobs, paths};
use rustix::{
    fs::{self as unix, AtFlags, Gid, Uid, XattrFlags},
    io::Errno,
};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

fn xattr_names(path: &Path) -> Result<Vec<u8>> {
    let size = match unix::llistxattr(path, &mut [0u8; 0][..]) {
        Ok(size) => size,
        Err(Errno::NOTSUP) => return Ok(Vec::new()),
        Err(e) => return Err(err(e)),
    };
    if size == 0 {
        return Ok(Vec::new());
    }
    let mut bytes = vec![0; size];
    let used = unix::llistxattr(path, bytes.as_mut_slice()).map_err(err)?;
    bytes.truncate(used);
    Ok(bytes)
}

pub fn metadata(from: &Path, to: &Path, meta: &fs::Metadata, strict: bool) -> Result<()> {
    let target = fs::symlink_metadata(to).map_err(err)?;
    if strict && (meta.uid() != target.uid() || meta.gid() != target.gid()) {
        unix::chownat(
            unix::CWD,
            to,
            Some(Uid::from_raw(meta.uid())),
            Some(Gid::from_raw(meta.gid())),
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(|e| format!("无法保留原文件的所有者或用户组：{e}"))?;
    }
    if !meta.is_symlink() {
        fs::set_permissions(
            to,
            fs::Permissions::from_mode(meta.mode() & if strict { 0o7777 } else { 0o777 }),
        )
        .map_err(err)?;
    }
    let names = xattr_names(from)?;
    for name in names.split(|b| *b == 0).filter(|name| !name.is_empty()) {
        if !strict && name == b"security.capability" {
            continue;
        }
        let name = OsStr::from_bytes(name);
        let size = unix::lgetxattr(from, name, &mut [0u8; 0][..]).map_err(err)?;
        let mut value = vec![0; size];
        let actual = if size == 0 {
            0
        } else {
            unix::lgetxattr(from, name, value.as_mut_slice()).map_err(err)?
        };
        unix::lsetxattr(to, name, &value[..actual], XattrFlags::empty())
            .map_err(|e| format!("无法保留扩展属性 {}：{e}", name.to_string_lossy()))?;
    }
    if strict {
        for name in xattr_names(to)?
            .split(|b| *b == 0)
            .filter(|name| !name.is_empty())
        {
            if !names.split(|b| *b == 0).any(|old| old == name) {
                unix::lremovexattr(to, OsStr::from_bytes(name))
                    .map_err(|e| format!("无法保留原文件的扩展属性：{e}"))?;
            }
        }
    }
    Ok(())
}

fn times(to: &Path, meta: &fs::Metadata) -> Result<()> {
    let ts = unix::Timestamps {
        last_access: unix::Timespec {
            tv_sec: meta.atime(),
            tv_nsec: meta.atime_nsec(),
        },
        last_modification: unix::Timespec {
            tv_sec: meta.mtime(),
            tv_nsec: meta.mtime_nsec(),
        },
    };
    unix::utimensat(unix::CWD, to, &ts, AtFlags::SYMLINK_NOFOLLOW).map_err(err)
}

pub fn copy(from: &Path, to: &Path, strict: bool) -> Result<()> {
    paths::reject_mount_tree(from)?;
    copy_entry(
        from,
        to,
        strict,
        &mut HashMap::new(),
        &mut vec![0; 256 * 1024],
    )
}

fn copy_entry(
    from: &Path,
    to: &Path,
    strict: bool,
    links: &mut HashMap<(u64, u64), PathBuf>,
    buffer: &mut [u8],
) -> Result<()> {
    jobs::checkpoint()?;
    let meta = fs::symlink_metadata(from).map_err(err)?;
    let key = (meta.dev(), meta.ino());
    if meta.is_file() && meta.nlink() > 1 {
        if let Some(first) = links.get(&key) {
            return fs::hard_link(first, to).map_err(err);
        }
        links.insert(key, to.to_path_buf());
    }

    let mut handle = None;
    if meta.is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(from).map_err(err)?, to).map_err(err)?;
    } else if meta.is_dir() {
        fs::DirBuilder::new().mode(0o700).create(to).map_err(err)?;
        for entry in fs::read_dir(from).map_err(err)? {
            let entry = entry.map_err(err)?;
            copy_entry(
                &entry.path(),
                &to.join(entry.file_name()),
                strict,
                links,
                buffer,
            )?;
        }
        handle = Some(fs::File::open(to).map_err(err)?);
    } else if meta.is_file() {
        let mut source = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(from)
            .map_err(err)?;
        let opened = source.metadata().map_err(err)?;
        if !opened.is_file() || opened.ino() != meta.ino() || opened.dev() != meta.dev() {
            return Err("复制前源文件发生变化，已保留原文件".into());
        }
        let mut dest = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(to)
            .map_err(err)?;
        let mut bytes = 0u64;
        loop {
            let n = source.read(buffer).map_err(err)?;
            if n == 0 {
                break;
            }
            dest.write_all(&buffer[..n]).map_err(err)?;
            jobs::checkpoint()?;
            bytes += n as u64;
            jobs::progress(&from.to_string_lossy(), bytes, 0, 0);
        }
        handle = Some(dest);
    } else {
        return Err("不支持复制设备、套接字或管道文件".into());
    }

    metadata(from, to, &meta, strict)?;
    times(to, &meta)?;
    let after = fs::symlink_metadata(from).map_err(err)?;
    if meta.ino() != after.ino()
        || meta.dev() != after.dev()
        || meta.len() != after.len()
        || meta.mtime() != after.mtime()
        || meta.mtime_nsec() != after.mtime_nsec()
        || meta.ctime() != after.ctime()
        || meta.ctime_nsec() != after.ctime_nsec()
    {
        return Err(format!(
            "复制期间源文件发生变化：{}；已保留源文件",
            from.display()
        ));
    }
    if let Some(handle) = handle {
        handle.sync_all().map_err(err)?;
    }
    Ok(())
}

use std::os::unix::fs::DirBuilderExt;
