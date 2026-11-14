use crate::{
    Core, Result, err, jobs,
    operations::{self, Journal, UndoStep},
    paths, process, str_arg,
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;
const CONFIGS: &[(&str, &str)] = &[
    ("CLAUDE.md", "Claude"),
    (".claude/settings.json", "Claude"),
    (".claude/settings.local.json", "Claude"),
    (".mcp.json", "Claude"),
    ("AGENTS.md", "Codex"),
    (".codex/config.toml", "Codex"),
    (".cursor/rules", "Cursor"),
    (".cursor/mcp.json", "Cursor"),
    (".cursorrules", "Cursor"),
    ("GEMINI.md", "Gemini"),
    (".gemini/settings.json", "Gemini"),
    (".github/copilot-instructions.md", "Copilot"),
    ("opencode.json", "OpenCode"),
    ("opencode.jsonc", "OpenCode"),
    (".opencode", "OpenCode"),
    (".agent/rules", "Antigravity"),
    (".agents/skills", "Agent Skills"),
];
pub fn roots(core: &Core, args: &Value) -> Result<Vec<PathBuf>> {
    if let Some(r) = args["roots"].as_array() {
        return r
            .iter()
            .filter_map(Value::as_str)
            .map(|p| paths::decode(core, p))
            .collect();
    }
    let c = core.get_value("catalog")?["data"].clone();
    let mut roots = Vec::new();
    if let Some(a) = c["agentRoots"].as_array() {
        for p in a.iter().filter_map(Value::as_str) {
            roots.push(paths::decode(core, p)?);
        }
    }
    if let Some(a) = c["workspaces"].as_array() {
        for w in a {
            if let Some(p) = w["paths"].as_array() {
                for p in p.iter().filter_map(Value::as_str) {
                    let path = paths::decode(core, p)?;
                    if path.is_dir() {
                        roots.push(path)
                    }
                }
            }
        }
    }
    if roots.is_empty() {
        for name in ["My", "Projects", "IdeaProjects", "projects", "code"] {
            let p = core.home_dir.join(name);
            if p.is_dir() {
                roots.push(p);
            }
        }
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}
pub fn project(path: &Path, global: bool) -> Option<Value> {
    let mut configs = Vec::new();
    let mut tools = HashSet::new();
    for (name, tool) in CONFIGS {
        let p = path.join(name);
        if p.exists() {
            tools.insert(*tool);
            configs.push(json!({"path":paths::encode(&p),"name":name,"tool":tool,"personal":name.contains(".local."),"is_dir":p.is_dir()}));
        }
    }
    if configs.is_empty() && !global {
        return None;
    }
    Some(
        json!({"path":paths::encode(path),"name":if global{"全局配置".to_owned()}else{path.file_name().unwrap_or_default().to_string_lossy().into_owned()},"global":global,"tools":tools.into_iter().collect::<Vec<_>>(),"configs":configs,"git":path.join(".git").exists()}),
    )
}
pub fn projects(core: &Core, args: &Value) -> Result<Value> {
    let mut result = Vec::new();
    if let Some(mut global) = project(&core.home_dir, true) {
        let extras = [
            (".codex/config.toml", "Codex"),
            (".claude.json", "Claude"),
            (".gemini/settings.json", "Gemini"),
            (".config/opencode/opencode.json", "OpenCode"),
            (".config/opencode/opencode.jsonc", "OpenCode"),
            (".config/github-copilot/mcp-config.json", "Copilot"),
            (".cursor/mcp.json", "Cursor"),
        ];
        for (name, tool) in extras {
            let p = core.home_dir.join(name);
            if p.is_file()
                && !global["configs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["path"] == paths::encode(&p))
            {
                global["configs"].as_array_mut().unwrap().push(json!({"path":paths::encode(&p),"name":name,"tool":tool,"personal":false,"is_dir":false}));
                if !global["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == tool)
                {
                    global["tools"].as_array_mut().unwrap().push(json!(tool));
                }
            }
        }
        result.push(global);
    }
    let mut seen = HashSet::new();
    let mut visited = 0;
    let mut truncated = false;
    for root in roots(core, args)? {
        for item in WalkDir::new(root)
            .max_depth(5)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                e.depth() == 0
                    || !matches!(
                        e.file_name().to_str(),
                        Some(
                            "node_modules"
                                | ".git"
                                | "target"
                                | ".cache"
                                | ".venv"
                                | "dist"
                                | "build"
                        )
                    )
            })
        {
            jobs::checkpoint()?;
            visited += 1;
            if visited > 20000 {
                truncated = true;
                break;
            }
            let Ok(e) = item else { continue };
            if !e.file_type().is_dir() || !seen.insert(e.path().to_owned()) {
                continue;
            }
            if let Some(v) = project(e.path(), false) {
                result.push(v);
            }
        }
    }
    Ok(
        json!({"projects":result,"truncated":truncated,"apps":process::desktop(core,&json!({"action":"apps"}))?}),
    )
}
pub fn copy(core: &Core, args: &Value) -> Result<Value> {
    let source = paths::decode(core, str_arg(args, "source")?)?;
    let dest = paths::decode(core, str_arg(args, "destination")?)?;
    if !dest.is_dir() {
        return Err("目标项目不存在".into());
    }
    if dest == source || dest.starts_with(&source) {
        return Err("请选择其他项目目录".into());
    }
    let journal = Journal::begin(core, "复制 Agent 配置")?;
    let mut steps = Vec::new();
    let mut count = 0;
    for (name, _) in CONFIGS {
        if name.contains(".local.") || name.ends_with("mcp.json") {
            continue;
        }
        let from = source.join(name);
        if !from.is_file()
            || fs::symlink_metadata(&from)
                .map_err(err)?
                .file_type()
                .is_symlink()
        {
            continue;
        }
        let content = fs::read_to_string(&from).map_err(err)?;
        let secret=regex::Regex::new(r#"(?i)(?:api[_-]?key|access[_-]?token|secret|password)\s*["']?\s*[:=]\s*["'][^"']+["']"#).unwrap();
        if secret.is_match(&content) {
            continue;
        }
        let to = dest.join(name);
        if paths::existing(&to) {
            return Err(format!("目标已存在 {name}，请先检查并手动合并"));
        }
        if let Some(p) = to.parent() {
            fs::create_dir_all(p).map_err(err)?;
        }
        let stage = to.with_file_name(format!(".filemanager-{}.copy", uuid::Uuid::new_v4()));
        operations::copy_path(&from, &stage)?;
        steps.push(UndoStep::Remove {
            path: paths::encode(&to),
            fallback: Some(paths::encode(&stage)),
            fingerprint: operations::fingerprint(&stage)?,
        });
        journal.record(&steps)?;
        operations::rename_noreplace(&stage, &to).map_err(err)?;
        count += 1;
    }
    journal.finish(steps)?;
    Ok(json!({"count":count,"id":journal.id}))
}
fn starttime(pid: u32) -> Option<String> {
    let s = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    s.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)
        .map(str::to_owned)
}
pub fn servers(core: &Core) -> Result<Value> {
    let roots = roots(core, &json!({}))?;
    let mut ports = std::collections::HashMap::<u64, u16>::new();
    for name in ["tcp", "tcp6"] {
        let s = fs::read_to_string(format!("/proc/net/{name}")).unwrap_or_default();
        for l in s.lines().skip(1) {
            let r: Vec<_> = l.split_whitespace().collect();
            if r.len() > 9 && r[3] == "0A" {
                if let (Some(port), Some(inode)) = (
                    r[1].rsplit(':')
                        .next()
                        .and_then(|s| u16::from_str_radix(s, 16).ok()),
                    r[9].parse::<u64>().ok(),
                ) {
                    ports.insert(inode, port);
                }
            }
        }
    }
    let mut result = Vec::new();
    for entry in fs::read_dir("/proc").map_err(err)?.flatten() {
        jobs::checkpoint()?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if !fs::metadata(entry.path()).is_ok_and(|m| m.uid() == unsafe { libc::getuid() }) {
            continue;
        }
        let Ok(cwd) = fs::read_link(entry.path().join("cwd")) else {
            continue;
        };
        let cmd = fs::read_to_string(entry.path().join("comm"))
            .unwrap_or_default()
            .trim()
            .to_owned();
        let project = roots.iter().find(|r| cwd.starts_with(r));
        if project.is_none()
            && !matches!(
                cmd.as_str(),
                "node" | "python" | "python3" | "java" | "php" | "ruby" | "cargo" | "filemanager"
            )
        {
            continue;
        }
        let mut seen = HashSet::new();
        for fd in fs::read_dir(entry.path().join("fd"))
            .into_iter()
            .flatten()
            .flatten()
        {
            if let Ok(p) = fs::read_link(fd.path()) {
                let s = p.to_string_lossy();
                let inode = s
                    .strip_prefix("socket:[")
                    .and_then(|v| v.strip_suffix(']'))
                    .and_then(|s| s.parse::<u64>().ok());
                if let Some(port) = inode.and_then(|i| ports.get(&i)) {
                    if seen.insert(*port) {
                        result.push(json!({"pid":pid,"name":cmd,"cwd":paths::encode(&cwd),"port":port,"url":format!("http://localhost:{port}"),"start_time":starttime(pid),"can_stop":pid!=std::process::id()}));
                    }
                }
            }
        }
    }
    result.sort_by_key(|v| v["port"].as_u64());
    Ok(json!(result))
}
pub fn stop(args: &Value) -> Result<Value> {
    if args["confirmed"] != true {
        return Err("停止服务需要明确确认".into());
    }
    let pid = args["pid"]
        .as_u64()
        .filter(|p| *p > 1 && *p != std::process::id() as u64 && *p <= u32::MAX as u64)
        .ok_or("进程无效")? as u32;
    let m = fs::metadata(format!("/proc/{pid}")).map_err(err)?;
    if m.uid() != unsafe { libc::getuid() }
        || starttime(pid).as_deref() != args["start_time"].as_str()
    {
        return Err("进程已改变或属于其他用户，请刷新".into());
    }
    if unsafe { libc::kill(pid as i32, libc::SIGINT) } != 0 {
        return Err(err(std::io::Error::last_os_error()));
    }
    Ok(json!({"ok":true}))
}
