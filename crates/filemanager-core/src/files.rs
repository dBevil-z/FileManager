use crate::{Core, Result, err, paths, str_arg};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use walkdir::WalkDir;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub extension: String,
    pub kind: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub target_is_dir: bool,
    pub hidden: bool,
    pub size: Option<u64>,
    pub modified: Option<f64>,
    pub created: Option<f64>,
    pub accessed: Option<f64>,
    pub inode: u64,
    pub device: u64,
    pub writable: bool,
}
pub fn seconds(time: std::io::Result<SystemTime>) -> Option<f64> {
    time.ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|v| v.as_secs_f64())
}
pub fn kind(path: &Path, is_dir: bool) -> &'static str {
    if is_dir {
        return "folder";
    }
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" | "heic" | "heif" | "avif" | "bmp"
        | "tif" | "tiff" | "ico" | "raw" | "cr2" | "nef" => "image",
        "mp4" | "mkv" | "mov" | "webm" | "avi" | "m4v" | "mpeg" => "video",
        "mp3" | "wav" | "flac" | "ogg" | "opus" | "m4a" | "aac" => "audio",
        "pdf" => "pdf",
        "md" | "markdown" | "mdown" | "mkd" => "markdown",
        "doc" | "docx" | "odt" | "rtf" | "pages" => "document",
        "xls" | "xlsx" | "ods" | "csv" | "numbers" => "spreadsheet",
        "ppt" | "pptx" | "odp" | "key" => "presentation",
        "zip" | "7z" | "gz" | "tgz" | "tar" | "bz2" | "xz" | "rar" | "zst" => "archive",
        "deb" | "rpm" | "appimage" | "dmg" | "pkg" | "msi" | "exe" => "installer",
        "rs" | "js" | "ts" | "tsx" | "jsx" | "vue" | "py" | "java" | "go" | "c" | "cpp" | "h"
        | "sh" | "zsh" | "sql" | "html" | "css" | "json" | "yaml" | "yml" | "toml" | "xml" => {
            "code"
        }
        "txt" | "log" | "ini" | "conf" | "cfg" | "env" => "text",
        _ => "file",
    }
}
pub fn entry(path: &Path) -> Result<Entry> {
    let meta = fs::symlink_metadata(path).map_err(err)?;
    let name = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned();
    let link = meta.file_type().is_symlink();
    let writable = std::ffi::CString::new(path.as_os_str().as_bytes())
        .ok()
        .is_some_and(|p| unsafe { libc::access(p.as_ptr(), libc::W_OK) == 0 });
    Ok(Entry {
        path: paths::encode(path),
        hidden: name.starts_with('.'),
        name,
        extension: path
            .extension()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default(),
        kind: kind(path, meta.is_dir()).into(),
        is_dir: meta.is_dir(),
        is_symlink: link,
        target_is_dir: link && fs::metadata(path).is_ok_and(|m| m.is_dir()),
        size: if meta.is_dir() {
            None
        } else {
            Some(meta.len())
        },
        modified: seconds(meta.modified()),
        created: seconds(meta.created()),
        accessed: seconds(meta.accessed()),
        inode: meta.ino(),
        device: meta.dev(),
        writable,
    })
}
pub fn list(core: &Core, args: &Value) -> Result<Value> {
    let path = paths::decode(core, str_arg(args, "path")?)?;
    let hidden = args["show_hidden"].as_bool().unwrap_or(false);
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    for item in fs::read_dir(&path).map_err(|e| format!("无法打开 {}：{e}", path.display()))? {
        match item {
            Ok(item) => {
                if !hidden && item.file_name().as_bytes().starts_with(b".") {
                    continue;
                }
                match entry(&item.path()) {
                    Ok(e) => entries.push(e),
                    Err(e) => warnings.push(e),
                }
            }
            Err(e) => warnings.push(e.to_string()),
        }
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    let total = entries.len();
    let offset = args["offset"].as_u64().unwrap_or(0) as usize;
    let limit = args["limit"].as_u64().unwrap_or(10000).min(20000) as usize;
    let entries: Vec<_> = entries.into_iter().skip(offset).take(limit).collect();
    Ok(
        json!({"path":paths::encode(&path),"display_path":path.to_string_lossy(),"parent":path.parent().map(paths::encode),"entries":entries,"total":total,"warnings":warnings,"writable":entry(&path)?.writable}),
    )
}
pub fn info(core: &Core, args: &Value) -> Result<Value> {
    Ok(json!(entry(&paths::decode(core, str_arg(args, "path")?)?)?))
}
pub fn places(core: &Core) -> Value {
    let candidates = [
        ("home", "主目录", core.home_dir.clone()),
        (
            "desktop",
            "桌面",
            user_directory(core, "DESKTOP", "Desktop"),
        ),
        (
            "documents",
            "文档",
            user_directory(core, "DOCUMENTS", "Documents"),
        ),
        (
            "downloads",
            "下载",
            user_directory(core, "DOWNLOAD", "Downloads"),
        ),
        (
            "pictures",
            "图片",
            user_directory(core, "PICTURES", "Pictures"),
        ),
        ("music", "音乐", user_directory(core, "MUSIC", "Music")),
        ("videos", "视频", user_directory(core, "VIDEOS", "Videos")),
        ("root", "文件系统", PathBuf::from("/")),
    ];
    json!(
        candidates
            .into_iter()
            .filter(|(_, _, p)| p.exists())
            .map(|(id, name, p)| json!({"id":id,"name":name,"path":paths::encode(&p)}))
            .collect::<Vec<_>>()
    )
}
pub fn user_directory(core: &Core, key: &str, fallback: &str) -> PathBuf {
    let config =
        fs::read_to_string(core.home_dir.join(".config/user-dirs.dirs")).unwrap_or_default();
    let prefix = format!("XDG_{key}_DIR=");
    config
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .map(|value| {
            value
                .trim_matches('"')
                .replace("$HOME", &core.home_dir.to_string_lossy())
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| core.home_dir.join(fallback))
}
pub fn read_text(core: &Core, args: &Value) -> Result<Value> {
    let path = paths::decode(core, str_arg(args, "path")?)?;
    let metadata = fs::metadata(&path).map_err(err)?;
    if !metadata.is_file() || metadata.len() > 5 * 1024 * 1024 {
        return Err("仅支持 5 MB 以内的文本文件，请使用其他应用打开".into());
    }
    let bytes = fs::read(&path).map_err(err)?;
    if bytes.contains(&0) {
        return Err("此文件不是纯文本".into());
    }
    let text =
        std::str::from_utf8(&bytes).map_err(|_| "此文件不是 UTF-8 文本，请使用其他应用打开")?;
    Ok(
        json!({"text":text,"revision":format!("{:x}",Sha256::digest(&bytes)),"entry":entry(&path)?,"line_ending":if text.contains("\r\n"){"crlf"}else{"lf"}}),
    )
}
pub fn folder_size(core: &Core, args: &Value) -> Result<Value> {
    let path = paths::decode(core, str_arg(args, "path")?)?;
    let mut size = 0u64;
    let mut files = 0u64;
    let mut folders = 0u64;
    let mut errors = 0u64;
    let mut seen = std::collections::HashSet::new();
    let mut truncated = false;
    for item in WalkDir::new(&path).follow_links(false).max_open(16) {
        let Ok(item) = item else {
            errors += 1;
            continue;
        };
        if files + folders >= 1_000_000 {
            truncated = true;
            break;
        }
        let Ok(meta) = fs::symlink_metadata(item.path()) else {
            errors += 1;
            continue;
        };
        if meta.is_dir() {
            folders += 1;
        } else if seen.insert((meta.dev(), meta.ino())) {
            size = size.saturating_add(meta.len());
            files += 1;
        }
    }
    Ok(
        json!({"path":paths::encode(&path),"size":size,"files":files,"folders":folders.saturating_sub(1),"errors":errors,"truncated":truncated}),
    )
}
pub fn folder_info(core: &Core, args: &Value) -> Result<Value> {
    let path = paths::decode(core, str_arg(args, "path")?)?;
    fn children(path: &Path, depth: u8) -> Vec<Value> {
        if depth == 0 {
            return vec![];
        }
        let mut result = Vec::new();
        if let Ok(read) = fs::read_dir(path) {
            for item in read.flatten().take(100) {
                if let Ok(e) = entry(&item.path()) {
                    if !e.hidden {
                        let kids = if e.is_dir {
                            children(&item.path(), depth - 1)
                        } else {
                            vec![]
                        };
                        result.push(json!({"entry":e,"children":kids}));
                    }
                }
            }
        }
        result.sort_by(|a, b| {
            b["entry"]["is_dir"]
                .as_bool()
                .cmp(&a["entry"]["is_dir"].as_bool())
                .then_with(|| {
                    a["entry"]["name"]
                        .as_str()
                        .cmp(&b["entry"]["name"].as_str())
                })
        });
        result
    }
    Ok(json!({"entry":entry(&path)?,"children":children(&path,2)}))
}
pub fn complete(core: &Core, args: &Value) -> Result<Value> {
    let input = str_arg(args, "text")?;
    let path = paths::decode(core, input)?;
    let (parent, prefix) = if input.ends_with('/') {
        (path.clone(), String::new())
    } else {
        (
            path.parent().unwrap_or(&core.home_dir).to_path_buf(),
            path.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
    };
    let mut items = Vec::new();
    if let Ok(read) = fs::read_dir(&parent) {
        for item in read.flatten() {
            let name = item.file_name().to_string_lossy().into_owned();
            if name.to_lowercase().starts_with(&prefix.to_lowercase()) {
                if let Ok(e) = entry(&item.path()) {
                    items.push(
                        json!({"path":e.path,"name":e.name,"is_dir":e.is_dir||e.target_is_dir}),
                    );
                }
            }
            if items.len() >= 100 {
                break;
            }
        }
    }
    items.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(json!({"items":items,"exists":paths::existing(&path),"parent":paths::encode(&parent)}))
}
pub fn search(core: &Core, args: &Value) -> Result<Value> {
    let query = str_arg(args, "query")?.to_lowercase();
    if query.trim().is_empty() {
        return Ok(json!({"entries":[],"truncated":false}));
    }
    let roots = args["roots"].as_array().ok_or("缺少搜索范围")?;
    let limit = args["limit"].as_u64().unwrap_or(500).min(5000) as usize;
    let mut entries = Vec::new();
    let mut visited = 0usize;
    let mut truncated = false;
    for root in roots {
        let path = paths::decode(core, root.as_str().ok_or("无效的搜索目录")?)?;
        let walk = WalkDir::new(path)
            .follow_links(false)
            .max_depth(30)
            .max_open(16)
            .into_iter()
            .filter_entry(|e| {
                e.depth() == 0
                    || !matches!(
                        e.file_name().to_str(),
                        Some("node_modules" | ".git" | "target" | ".cache")
                    )
            });
        for item in walk.flatten() {
            visited += 1;
            if visited > 200000 || entries.len() >= limit {
                truncated = true;
                break;
            }
            let name = item.file_name().to_string_lossy();
            if (!name.starts_with('.') || args["show_hidden"].as_bool() == Some(true))
                && name.to_lowercase().contains(&query)
            {
                if let Ok(e) = entry(item.path()) {
                    entries.push(e);
                }
            }
        }
        if truncated {
            break;
        }
    }
    Ok(json!({"entries":entries,"total":entries.len(),"visited":visited,"truncated":truncated}))
}
