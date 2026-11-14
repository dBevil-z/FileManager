use crate::{
    Core, Result, err, files, jobs,
    operations::{self, Fingerprint, Journal, UndoStep},
    paths, str_arg,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};
pub fn category(kind: &str, name: &str) -> &'static str {
    if name.to_lowercase().contains("screenshot") || name.contains("截图") {
        "截图"
    } else {
        match kind {
            "image" => "图片",
            "video" => "视频",
            "audio" => "音频",
            "archive" => "压缩包",
            "installer" => "安装包",
            "code" => "代码",
            "pdf" | "document" | "spreadsheet" | "presentation" | "markdown" | "text" => "文档",
            _ => "其他",
        }
    }
}
pub fn plan(core: &Core, args: &Value) -> Result<Value> {
    let root = paths::decode(core, str_arg(args, "path")?)?;
    let mut items = Vec::new();
    let mut skipped = Vec::new();
    for item in fs::read_dir(&root).map_err(err)? {
        jobs::checkpoint()?;
        let item = item.map_err(err)?;
        let e = files::entry(&item.path())?;
        if e.is_dir || e.hidden || e.is_symlink {
            continue;
        }
        let name = e.name.to_lowercase();
        let recent = fs::metadata(item.path())
            .ok()
            .and_then(|m| m.modified().ok())
            .is_none_or(|t| {
                SystemTime::now()
                    .duration_since(t)
                    .unwrap_or(Duration::ZERO)
                    .as_secs()
                    < 120
            });
        if recent
            || [".part", ".crdownload", ".download", ".tmp"]
                .iter()
                .any(|x| name.ends_with(x))
        {
            skipped
                .push(json!({"entry":e,"reason":if recent{"最近两分钟修改"}else{"下载尚未完成"}}));
            continue;
        }
        let group = category(&e.kind, &e.name);
        let target = if group == "截图" {
            args["screenshots"]
                .as_str()
                .map(|p| paths::decode(core, p))
                .transpose()?
                .unwrap_or_else(|| root.join(group))
        } else {
            root.join(group)
        };
        items.push(json!({"entry":e,"category":group,"destination":paths::encode(&target),"fingerprint":operations::fingerprint(&item.path())?,"selected":true,"action":if group=="安装包"&&args["trash_installers"]==true{"trash"}else{"move"}}));
    }
    items.sort_by(|a, b| {
        a["entry"]["name"]
            .as_str()
            .cmp(&b["entry"]["name"].as_str())
    });
    Ok(json!({"root":paths::encode(&root),"items":items,"skipped":skipped}))
}
pub fn apply(core: &Core, args: &Value) -> Result<Value> {
    let plan = &args["plan"];
    let items = plan["items"].as_array().ok_or("整理计划无效")?;
    let chosen: Vec<_> = items.iter().filter(|v| v["selected"] != false).collect();
    for v in &chosen {
        let source = paths::decode(core, str_arg(&v["entry"], "path")?)?;
        let expected: Fingerprint =
            serde_json::from_value(v["fingerprint"].clone()).map_err(err)?;
        if operations::fingerprint(&source)? != expected {
            return Err(format!("{} 已改变，请重新生成整理计划", source.display()));
        }
        paths::protect(core, &source)?;
    }
    let journal = Journal::begin(core, "整理文件")?;
    let mut steps = Vec::new();
    let mut folders = std::collections::HashSet::new();
    for (index, v) in chosen.iter().enumerate() {
        jobs::checkpoint()?;
        let source = paths::decode(core, str_arg(&v["entry"], "path")?)?;
        jobs::progress(
            &source.to_string_lossy(),
            0,
            index as u64,
            chosen.len() as u64,
        );
        if v["action"] == "trash" {
            let item = crate::trash::plan(core, &source)?;
            steps.push(UndoStep::Trash { item: item.clone() });
            journal.record(&steps)?;
            crate::trash::apply(core, &item)?;
            continue;
        }
        let dest = paths::decode(core, str_arg(v, "destination")?)?;
        if fs::canonicalize(&dest)
            .ok()
            .is_some_and(|d| d.starts_with(&source))
        {
            return Err("不能整理进源文件自身".into());
        }
        if !dest.exists() {
            fs::create_dir(&dest).map_err(err)?;
            folders.insert(dest.clone());
            let m = fs::metadata(&dest).map_err(err)?;
            use std::os::unix::fs::MetadataExt;
            steps.push(UndoStep::EmptyDir {
                path: paths::encode(&dest),
                inode: m.ino(),
                device: m.dev(),
            });
            journal.record(&steps)?;
        }
        let target = paths::unique(&dest.join(source.file_name().ok_or("无效文件名")?));
        operations::move_logged(core, &source, &target, &journal, &mut steps)?;
    }
    journal.finish(steps)?;
    Ok(json!({"id":journal.id,"count":chosen.len()}))
}
pub fn insights(core: &Core, args: &Value) -> Result<Value> {
    let root = paths::decode(core, str_arg(args, "path")?)?;
    let mut groups = std::collections::BTreeMap::<String, Value>::new();
    for i in fs::read_dir(root).map_err(err)? {
        let i = i.map_err(err)?;
        let e = files::entry(&i.path())?;
        if e.is_dir || e.hidden {
            continue;
        }
        let c = category(&e.kind, &e.name).to_string();
        let g = groups
            .entry(c.clone())
            .or_insert(json!({"name":c,"count":0,"size":0}));
        g["count"] = json!(g["count"].as_u64().unwrap_or(0) + 1);
        g["size"] = json!(g["size"].as_u64().unwrap_or(0) + e.size.unwrap_or(0));
    }
    Ok(json!({"groups":groups.values().collect::<Vec<_>>()}))
}
pub fn allowed_cache(core: &Core, p: &Path) -> bool {
    let rel = p.strip_prefix(&core.home_dir).unwrap_or(p);
    let s = rel.to_string_lossy();
    [
        ".cache/pip",
        ".cache/uv",
        ".cache/npm",
        ".cache/yarn",
        ".cache/pnpm",
        ".cache/thumbnails",
        ".cache/mesa_shader_cache",
        ".npm/_cacache",
        ".gradle/caches",
        ".cargo/registry/cache",
    ]
    .iter()
    .any(|x| s == *x || s.starts_with(&format!("{x}/")))
        || p.ends_with(".next/cache")
        || p.ends_with(".parcel-cache")
}
