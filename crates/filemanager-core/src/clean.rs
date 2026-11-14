use crate::{Core, Result, err, files, jobs, organize, paths};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    os::unix::fs::MetadataExt,
    time::{Instant, SystemTime},
};
use walkdir::WalkDir;
pub fn apply(core: &Core, args: &Value) -> Result<Value> {
    let items = args["items"].as_array().ok_or("未选择清理项目")?;
    let mut selected = HashSet::new();
    for item in items {
        let path = crate::str_arg(&item["entry"], "path")?;
        let p = paths::decode(core, path)?;
        let entry = files::entry(&p)?;
        if item["keeper"] == true {
            return Err("不能清理指定保留的副本".into());
        }
        let previous: files::Entry = serde_json::from_value(item["entry"].clone()).map_err(err)?;
        if entry.inode != previous.inode
            || entry.device != previous.device
            || entry.modified != previous.modified
            || entry.size != previous.size
        {
            return Err(format!("{} 已改变，请重新扫描", entry.name));
        }
        if args["category"] == "caches" && !organize::allowed_cache(core, &p) {
            return Err("此位置不属于缓存白名单".into());
        }
        selected.insert(path.to_owned());
    }
    if args["category"] == "duplicates" {
        let groups = args["groups"].as_array().ok_or("缺少重复文件核对信息")?;
        for path in &selected {
            let g = groups
                .iter()
                .find(|g| {
                    g["items"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| v["entry"]["path"] == *path))
                })
                .ok_or("文件不在去重组中")?;
            let expected = g["hash"].as_str().ok_or("缺少内容校验值")?;
            let keep = g["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|i| i["entry"]["path"].as_str())
                .find(|p| {
                    !selected.contains(*p) && paths::decode(core, p).is_ok_and(|p| p.is_file())
                })
                .ok_or("至少要保留一个仍然存在的副本")?;
            for token in [path.as_str(), keep] {
                let p = paths::decode(core, token)?;
                let mut f = fs::File::open(p).map_err(err)?;
                let mut h = Sha256::new();
                let mut buf = vec![0; 256 * 1024];
                loop {
                    jobs::checkpoint()?;
                    let n = f.read(&mut buf).map_err(err)?;
                    if n == 0 {
                        break;
                    }
                    h.update(&buf[..n]);
                }
                if format!("{:x}", h.finalize()) != expected {
                    return Err("重复文件内容已经改变，请重新扫描".into());
                }
            }
        }
    }
    crate::operations::operate(
        core,
        &json!({"kind":"trash","paths":selected.into_iter().collect::<Vec<_>>()}),
    )
}
pub fn scan(core: &Core, args: &Value) -> Result<Value> {
    let roots = args["roots"]
        .as_array()
        .map(|v| {
            v.iter()
                .filter_map(Value::as_str)
                .map(|p| paths::decode(core, p))
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_else(|| {
            vec![
                files::user_directory(core, "DOWNLOAD", "Downloads"),
                core.home_dir.join(".cache"),
                core.home_dir.join(".npm/_cacache"),
            ]
        });
    let names = [
        ("caches", "开发与应用缓存"),
        ("duplicates", "重复文件"),
        ("installers", "安装包"),
        ("mail", "邮件附件"),
        ("leftovers", "应用残留"),
        ("large", "大文件"),
        ("idle", "闲置应用"),
        ("recordings", "录屏"),
        ("old", "旧下载"),
    ];
    let mut cats:Vec<Value>=names.iter().map(|(id,name)|json!({"id":id,"name":name,"items":[],"groups":[],"size":0,"reclaimable":0})).collect();
    cats[4]["note"] = json!("应用配置可能属于便携软件，不自动判定为残留。");
    cats[6]["note"] = json!("Linux 没有统一的应用使用历史，无法可靠判定闲置应用。");
    let started = Instant::now();
    let mut truncated = false;
    let mut errors = Vec::new();
    let mut all = Vec::new();
    let mut seen = HashSet::new();
    let mut visited = 0u64;
    for root in roots {
        if !root.exists() {
            continue;
        }
        for item in WalkDir::new(&root)
            .follow_links(false)
            .max_depth(24)
            .max_open(16)
            .into_iter()
            .filter_entry(|e| {
                e.path() != core.data_dir
                    && !e.path().starts_with(&core.trash_dir)
                    && !matches!(e.file_name().to_str(), Some(".git" | "node_modules"))
            })
        {
            jobs::checkpoint()?;
            visited += 1;
            if visited > 100000 || started.elapsed().as_secs() > 25 {
                truncated = true;
                break;
            }
            if visited % 100 == 0 {
                jobs::progress("检查文件内容与可释放空间", 0, visited, 0);
            }
            let item = match item {
                Ok(v) => v,
                Err(e) => {
                    if errors.len() < 40 {
                        errors.push(e.to_string());
                    }
                    continue;
                }
            };
            let path = item.path();
            let m = match fs::symlink_metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if !m.is_file() || !seen.insert((m.dev(), m.ino())) {
                continue;
            }
            let e = match files::entry(path) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let age = m
                .modified()
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .map(|d| d.as_secs());
            let reason = if organize::allowed_cache(core, path) {
                Some((0, "已知可重新生成的缓存"))
            } else if e.kind == "installer" {
                Some((2, "安装完成后通常不再需要；请自行确认"))
            } else if m.len() > 500 * 1024 * 1024 {
                Some((5, "文件大于 500 MB"))
            } else if e.kind == "video"
                && (e.name.to_lowercase().contains("screencast") || e.name.contains("录屏"))
            {
                Some((7, "文件名符合录屏特征"))
            } else if path.starts_with(files::user_directory(core, "DOWNLOAD", "Downloads"))
                && age.is_some_and(|a| a > 90 * 86400)
            {
                Some((8, "下载目录中超过 90 天未修改；并非使用记录"))
            } else if path.to_string_lossy().contains("/akonadi/file_db_data/") {
                Some((3, "邮件本地附件，请核对邮件仍可重新下载"))
            } else {
                None
            };
            if let Some((index, reason)) = reason {
                let selected = false;
                cats[index]["items"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"entry":e,"reason":reason,"selected":selected}));
                let n = cats[index]["size"].as_u64().unwrap() + m.len();
                cats[index]["size"] = json!(n);
                cats[index]["reclaimable"] = json!(n);
            }
            if m.len() > 0 && m.len() <= 1024 * 1024 * 1024 {
                all.push((path.to_owned(), m.len(), m.modified().ok()));
            }
        }
        if truncated {
            break;
        }
    }
    let mut sizes = HashMap::<u64, Vec<_>>::new();
    for row in all {
        sizes.entry(row.1).or_default().push(row);
    }
    let mut hashed_bytes = 0u64;
    let mut hashes = HashMap::<(u64, String), Vec<Value>>::new();
    for (size, rows) in sizes {
        if rows.len() < 2 {
            continue;
        }
        for (p, _, mtime) in rows {
            jobs::checkpoint()?;
            if hashed_bytes + size > 3 * 1024 * 1024 * 1024 || started.elapsed().as_secs() > 45 {
                truncated = true;
                break;
            }
            let mut f = match fs::File::open(&p) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let mut hash = Sha256::new();
            let mut buf = vec![0; 256 * 1024];
            loop {
                jobs::checkpoint()?;
                let n = f.read(&mut buf).map_err(err)?;
                if n == 0 {
                    break;
                }
                hash.update(&buf[..n]);
                hashed_bytes += n as u64;
            }
            let after = f.metadata().map_err(err)?;
            if after.len() != size || after.modified().ok() != mtime {
                continue;
            }
            hashes
                .entry((size, format!("{:x}", hash.finalize())))
                .or_default()
                .push(json!(files::entry(&p)?));
        }
    }
    let mut groups = Vec::new();
    let mut dupitems = Vec::new();
    let mut reclaim = 0;
    for ((size, hash), mut entries) in hashes {
        if entries.len() < 2 {
            continue;
        }
        entries.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
        let mut items = Vec::new();
        for (i, entry) in entries.into_iter().enumerate() {
            let item = json!({"entry":entry,"selected":i>0,"keeper":i==0,"hash":hash,"reason":if i==0{"保留此副本"}else{"内容 SHA-256 相同的额外副本"}});
            items.push(item.clone());
            dupitems.push(item);
        }
        reclaim += size * (items.len() as u64 - 1);
        groups.push(json!({"hash":hash,"size":size,"items":items}));
    }
    groups.sort_by(|a, b| b["size"].as_u64().cmp(&a["size"].as_u64()));
    cats[1]["groups"] = json!(groups);
    cats[1]["items"] = json!(dupitems);
    cats[1]["size"] = json!(reclaim);
    cats[1]["reclaimable"] = json!(reclaim);
    Ok(
        json!({"categories":cats,"visited":visited,"truncated":truncated,"errors":errors,"seconds":started.elapsed().as_secs_f64()}),
    )
}
