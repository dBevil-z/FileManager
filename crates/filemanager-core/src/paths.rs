use crate::{Core, Result, err};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Component, Path, PathBuf},
};

// Persistent path tokens are a storage format, including existing undo records.
const PREFIX: &str = "fileos:bytes:";
pub fn encode(path: &Path) -> String {
    path.to_str().map(str::to_owned).unwrap_or_else(|| {
        format!(
            "{PREFIX}{}",
            URL_SAFE_NO_PAD.encode(path.as_os_str().as_bytes())
        )
    })
}
pub fn decode(core: &Core, input: &str) -> Result<PathBuf> {
    if input.contains('\0') {
        return Err("路径不能含空字符".into());
    }
    let raw = if let Some(token) = input.strip_prefix(PREFIX) {
        PathBuf::from(OsString::from_vec(
            URL_SAFE_NO_PAD.decode(token).map_err(err)?,
        ))
    } else if input == "~" {
        core.home_dir.clone()
    } else if let Some(rest) = input.strip_prefix("~/") {
        core.home_dir.join(rest)
    } else {
        PathBuf::from(input)
    };
    let absolute = if raw.is_absolute() {
        raw
    } else {
        core.home_dir.join(raw)
    };
    let mut cleaned = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                cleaned.pop();
            }
            Component::CurDir => {}
            _ => cleaned.push(component.as_os_str()),
        }
    }
    Ok(cleaned)
}
pub fn existing(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}
/// Resolve directory aliases while preserving a symlink at the entry itself.
pub fn canonical_entry(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().ok_or("路径缺少上级目录")?;
    let name = path.file_name().ok_or("路径缺少文件名")?;
    Ok(std::fs::canonicalize(parent).map_err(err)?.join(name))
}
pub fn mount_points() -> Result<Vec<PathBuf>> {
    let data = std::fs::read("/proc/self/mountinfo").map_err(err)?;
    let mut mounts = Vec::new();
    for encoded in data
        .split(|b| *b == b'\n')
        .filter_map(|line| line.split(|b| *b == b' ').nth(4))
    {
        let mut decoded = Vec::with_capacity(encoded.len());
        let mut i = 0;
        while i < encoded.len() {
            if let [b'\\', a @ b'0'..=b'3', b @ b'0'..=b'7', c @ b'0'..=b'7', ..] = &encoded[i..] {
                decoded.push((a - b'0') * 64 + (b - b'0') * 8 + (c - b'0'));
                i += 4;
            } else {
                decoded.push(encoded[i]);
                i += 1;
            }
        }
        mounts.push(PathBuf::from(OsString::from_vec(decoded)));
    }
    Ok(mounts)
}

pub fn reject_mount_tree(path: &Path) -> Result<()> {
    let path = canonical_entry(path)?;
    if std::fs::symlink_metadata(&path).map_err(err)?.is_symlink() {
        return Ok(());
    }
    if mount_points()?.iter().any(|mount| mount.starts_with(&path)) {
        return Err(format!(
            "{} 是挂载点或包含挂载点，未操作其中的文件",
            path.display()
        ));
    }
    Ok(())
}
pub fn check_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\0')
        || name.as_bytes().len() > 255
    {
        return Err("文件名为空、过长，或包含不允许的字符".into());
    }
    Ok(())
}
pub fn protect(core: &Core, path: &Path) -> Result<()> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let protected = [
        PathBuf::from("/"),
        core.home_dir.clone(),
        core.data_dir.clone(),
    ];
    if protected
        .iter()
        .any(|p| canonical == *p || p.starts_with(&canonical))
    {
        return Err("此位置包含主目录或应用数据，不能直接操作".into());
    }
    Ok(())
}
pub fn unique(path: &Path) -> PathBuf {
    unique_avoiding(path, |_| false)
}
pub fn unique_avoiding(path: &Path, reserved: impl Fn(&Path) -> bool) -> PathBuf {
    if !existing(path) && !reserved(path) {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or(Path::new("/"));
    let stem = path.file_stem().unwrap_or_default();
    let ext = path.extension();
    for n in 2u64.. {
        let mut name = stem.to_os_string();
        name.push(format!(" {n}"));
        if let Some(ext) = ext {
            name.push(".");
            name.push(ext);
        }
        let candidate = parent.join(name);
        if !existing(&candidate) && !reserved(&candidate) {
            return candidate;
        }
    }
    unreachable!()
}
pub fn percent_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/-_.~".contains(b) {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
pub fn percent_decode(text: &str) -> Result<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err("损坏的转义路径".into());
            }
            out.push(u8::from_str_radix(&text[i + 1..i + 3], 16).map_err(err)?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Ok(out)
}
