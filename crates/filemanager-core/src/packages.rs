use crate::{Core, Result, process};
use serde_json::{Value, json};
use std::{collections::HashSet, fs, path::Path};
fn package(
    manager: &str,
    name: &str,
    version: &str,
    size: Option<u64>,
    reason: &str,
    description: &str,
    install: String,
    remove: String,
) -> Value {
    json!({"id":format!("{manager}:{name}:{version}"),"name":name,"version":version,"manager":manager,"size":size,"reason":reason,"description":description,"install_command":install,"uninstall_command":remove,"last_used":null})
}
pub fn list(core: &Core, args: &Value) -> Result<Value> {
    let only = args["manager"].as_str();
    let wanted = |s: &str| only.is_none_or(|v| v == "all" || v == s);
    let mut items = Vec::new();
    let mut errors = Vec::<Value>::new();
    let mut managers = Vec::new();
    if wanted("apt") && process::which("dpkg-query").is_some() {
        managers.push("apt");
        let manual: HashSet<String> = process::run("apt-mark", ["showmanual"], None, 8)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect();
        match process::run(
            "dpkg-query",
            [
                "-W",
                "-f=${binary:Package}\t${Version}\t${Installed-Size}\t${db:Status-Abbrev}\t${Synopsis}\n",
            ],
            None,
            12,
        ) {
            Ok(s) => {
                for line in s.lines() {
                    let c: Vec<_> = line.splitn(5, '\t').collect();
                    if c.len() < 5 {
                        continue;
                    }
                    let status = c[3];
                    if !status.starts_with("ii") && !status.starts_with("rc") {
                        continue;
                    }
                    let name = c[0];
                    let reason = if status.starts_with("rc") {
                        "residual"
                    } else if manual.contains(name)
                        || manual.contains(name.split(':').next().unwrap_or(name))
                    {
                        "manual"
                    } else {
                        "dependency"
                    };
                    items.push(package(
                        "apt",
                        name,
                        c[1],
                        c[2].parse::<u64>().ok().map(|v| v * 1024),
                        reason,
                        c[4],
                        format!("sudo apt install {}", process::quote(name.as_ref())),
                        format!(
                            "sudo apt {} {}",
                            if reason == "residual" {
                                "purge"
                            } else {
                                "remove"
                            },
                            process::quote(name.as_ref())
                        ),
                    ));
                }
            }
            Err(e) => errors.push(json!({"manager":"apt","error":e})),
        }
    }
    if wanted("snap") && process::which("snap").is_some() {
        managers.push("snap");
        match process::run("snap", ["list", "--all"], None, 12) {
            Ok(s) => {
                for line in s.lines().skip(1) {
                    let c: Vec<_> = line.split_whitespace().collect();
                    if c.len() >= 6 {
                        items.push(package(
                            "snap",
                            c[0],
                            c[1],
                            None,
                            if c.last().is_some_and(|s| s.contains("disabled")) {
                                "old_version"
                            } else {
                                "unknown"
                            },
                            c[2],
                            format!("sudo snap install {}", process::quote(c[0].as_ref())),
                            format!(
                                "sudo snap remove {} --revision={}",
                                process::quote(c[0].as_ref()),
                                process::quote(c[2].as_ref())
                            ),
                        ));
                    }
                }
            }
            Err(e) => errors.push(json!({"manager":"snap","error":e})),
        }
    }
    if wanted("flatpak") && process::which("flatpak").is_some() {
        managers.push("flatpak");
        match process::run(
            "flatpak",
            [
                "list",
                "--columns=application,name,version,installation,origin",
                "--all",
            ],
            None,
            10,
        ) {
            Ok(s) => {
                for line in s.lines() {
                    let c: Vec<_> = line.split('\t').collect();
                    if c.len() >= 3 {
                        items.push(package(
                            "flatpak",
                            c[0],
                            c[2],
                            None,
                            "unknown",
                            c[1],
                            format!("flatpak install {}", process::quote(c[0].as_ref())),
                            format!("flatpak uninstall {}", process::quote(c[0].as_ref())),
                        ));
                    }
                }
            }
            Err(e) => errors.push(json!({"manager":"flatpak","error":e})),
        }
    }
    for manager in ["npm", "pnpm"] {
        if !wanted(manager) || process::which(manager).is_none() {
            continue;
        }
        managers.push(manager);
        match process::run(
            manager,
            if manager == "npm" {
                vec!["list", "--global", "--depth=0", "--json"]
            } else {
                vec!["list", "--global", "--depth=0", "--json"]
            },
            None,
            15,
        ) {
            Ok(s) => {
                if let Ok(mut v) = serde_json::from_str::<Value>(&s) {
                    if v.is_array() {
                        v = v[0].clone();
                    }
                    if let Some(deps) = v["dependencies"].as_object() {
                        for (name, p) in deps {
                            items.push(package(
                                manager,
                                name,
                                p["version"].as_str().unwrap_or("未知"),
                                None,
                                "manual",
                                "全局 JavaScript 包",
                                format!("{manager} add --global {}", process::quote(name.as_ref())),
                                format!(
                                    "{manager} {} --global {}",
                                    if manager == "npm" {
                                        "uninstall"
                                    } else {
                                        "remove"
                                    },
                                    process::quote(name.as_ref())
                                ),
                            ));
                        }
                    }
                }
            }
            Err(e) => errors.push(json!({"manager":manager,"error":e})),
        }
    }
    if wanted("pip") && process::which("python3").is_some() {
        managers.push("pip");
        match process::run(
            "python3",
            [
                "-m",
                "pip",
                "list",
                "--format=json",
                "--disable-pip-version-check",
            ],
            None,
            12,
        ) {
            Ok(s) => {
                if let Ok(a) = serde_json::from_str::<Vec<Value>>(&s) {
                    for p in a {
                        let name = p["name"].as_str().unwrap_or("");
                        items.push(package(
                            "pip",
                            name,
                            p["version"].as_str().unwrap_or(""),
                            None,
                            "unknown",
                            "当前 Python 环境；部分包由 apt 管理",
                            format!("python3 -m pip install {}", process::quote(name.as_ref())),
                            format!("python3 -m pip uninstall {}", process::quote(name.as_ref())),
                        ));
                    }
                }
            }
            Err(e) => errors.push(json!({"manager":"pip","error":e})),
        }
    }
    if wanted("pipx") && process::which("pipx").is_some() {
        managers.push("pipx");
        if let Ok(s) = process::run("pipx", ["list", "--json"], None, 12) {
            if let Ok(v) = serde_json::from_str::<Value>(&s) {
                if let Some(venvs) = v["venvs"].as_object() {
                    for (name, p) in venvs {
                        items.push(package(
                            "pipx",
                            name,
                            p["metadata"]["main_package"]["package_version"]
                                .as_str()
                                .unwrap_or(""),
                            None,
                            "manual",
                            "隔离的 Python 应用",
                            format!("pipx install {}", process::quote(name.as_ref())),
                            format!("pipx uninstall {}", process::quote(name.as_ref())),
                        ));
                    }
                }
            }
        }
    }
    if wanted("cargo") && process::which("cargo").is_some() {
        managers.push("cargo");
        if let Ok(s) = process::run("cargo", ["install", "--list"], None, 10) {
            for line in s.lines().filter(|l| !l.starts_with(' ')) {
                let Some((name, version)) = line.trim_end_matches(':').split_once(" v") else {
                    continue;
                };
                items.push(package(
                    "cargo",
                    name,
                    version.split_whitespace().next().unwrap_or(version),
                    None,
                    "manual",
                    "Rust CLI",
                    format!("cargo install {}", process::quote(name.as_ref())),
                    format!("cargo uninstall {}", process::quote(name.as_ref())),
                ));
            }
        }
    }
    if wanted("gem") && process::which("gem").is_some() {
        managers.push("gem");
        if let Ok(s) = process::run("gem", ["list", "--local"], None, 10) {
            for line in s.lines() {
                let Some((name, versions)) = line.split_once(" (") else {
                    continue;
                };
                for version in versions.trim_end_matches(')').split(", ") {
                    items.push(package(
                        "gem",
                        name,
                        version,
                        None,
                        "unknown",
                        "Ruby Gem",
                        format!("gem install {}", process::quote(name.as_ref())),
                        format!(
                            "gem uninstall {} --version {}",
                            process::quote(name.as_ref()),
                            process::quote(version.as_ref())
                        ),
                    ));
                }
            }
        }
    }
    if wanted("brew") && process::which("brew").is_some() {
        managers.push("brew");
        if let Ok(s) = process::run("brew", ["list", "--versions"], None, 15) {
            for line in s.lines() {
                let c: Vec<_> = line.split_whitespace().collect();
                if c.len() > 1 {
                    for ver in &c[1..] {
                        items.push(package(
                            "brew",
                            c[0],
                            ver,
                            None,
                            "unknown",
                            "Homebrew",
                            format!("brew install {}", process::quote(c[0].as_ref())),
                            format!("brew uninstall {}", process::quote(c[0].as_ref())),
                        ));
                    }
                }
            }
        }
    }
    if wanted("go") && process::which("go").is_some() {
        managers.push("go");
        let bin = core.home_dir.join("go/bin");
        for file in fs::read_dir(bin).into_iter().flatten().flatten() {
            let p = file.path();
            if !p.is_file() {
                continue;
            }
            if let Ok(s) = process::run(
                "go",
                ["version".as_ref(), "-m".as_ref(), p.as_os_str()],
                None,
                3,
            ) {
                let module = s
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("mod\t"))
                    .unwrap_or("");
                let c: Vec<_> = module.split('\t').collect();
                let name = file.file_name().to_string_lossy().into_owned();
                items.push(package(
                    "go",
                    &name,
                    c.get(1).copied().unwrap_or("未知"),
                    fs::metadata(&p).ok().map(|m| m.len()),
                    "manual",
                    c.first().copied().unwrap_or("Go CLI"),
                    format!(
                        "go install {}@latest",
                        process::quote(c.first().copied().unwrap_or(&name).as_ref())
                    ),
                    format!("rm -- {}", process::quote(p.as_os_str())),
                ));
            }
        }
    }
    items.sort_by(|a, b| {
        a["name"]
            .as_str()
            .cmp(&b["name"].as_str())
            .then(a["manager"].as_str().cmp(&b["manager"].as_str()))
    });
    let caches: Vec<_> = [
        ("npm", ".npm/_cacache", "npm cache clean --force"),
        ("pip", ".cache/pip", "python3 -m pip cache purge"),
        ("pnpm", ".local/share/pnpm/store", "pnpm store prune"),
        ("cargo", ".cargo/registry/cache", "cargo cache --autoclean"),
    ]
    .into_iter()
    .filter_map(|(name, p, cmd)| {
        let path = core.home_dir.join(p);
        if !path.exists() {
            return None;
        }
        Some(json!({"name":name,"path":paths_string(&path),"command":cmd,"size":null}))
    })
    .collect();
    let total_size: u64 = items.iter().filter_map(|v| v["size"].as_u64()).sum();
    Ok(
        json!({"items":items,"managers":managers,"errors":errors,"caches":caches,"total_size":total_size,"total":items.len(),"note":"无统一可靠的上次使用记录；未知值不作为闲置依据。卸载与清缓存命令仅复制，请在终端核对执行。"}),
    )
}
fn paths_string(p: &Path) -> String {
    crate::paths::encode(p)
}
