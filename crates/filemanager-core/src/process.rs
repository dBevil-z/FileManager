use crate::{Core, Result, err, paths, str_arg};
use serde_json::{Value, json};
use std::{
    ffi::{OsStr, OsString},
    io::Read,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p).map(|d| d.join(name)).find(|p| {
            p.is_file() && {
                use std::os::unix::fs::PermissionsExt;
                std::fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
            }
        })
    })
}
pub fn run<I, S>(program: &str, args: I, cwd: Option<&Path>, seconds: u64) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(program);
    cmd.args(args)
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd.spawn().map_err(err)?;
    struct Group(u32);
    impl Drop for Group {
        fn drop(&mut self) {
            unsafe {
                libc::kill(-(self.0 as i32), libc::SIGKILL);
            }
        }
    }
    let group = Group(child.id());
    let out = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    fn collect(mut reader: impl Read) -> Vec<u8> {
        let mut out = Vec::new();
        let mut b = [0u8; 8192];
        while let Ok(n) = reader.read(&mut b) {
            if n == 0 {
                break;
            }
            if out.len() < 16 * 1024 * 1024 {
                out.extend_from_slice(&b[..n.min(16 * 1024 * 1024 - out.len())]);
            }
        }
        out
    }
    let a = std::thread::spawn(move || collect(out));
    let b = std::thread::spawn(move || collect(stderr));
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().map_err(err)? {
            break s;
        }
        if start.elapsed() > Duration::from_secs(seconds) || crate::jobs::checkpoint().is_err() {
            // Shell launchers (notably LibreOffice) spawn grandchildren. Killing
            // only the launcher leaves converters and pipe-reader threads alive.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            let _ = a.join();
            let _ = b.join();
            return Err(format!("{program} 超时或被取消"));
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    drop(group);
    let out = a.join().unwrap_or_default();
    let stderr = b.join().unwrap_or_default();
    if !status.success() {
        return Err(format!(
            "{program}: {}",
            String::from_utf8_lossy(&stderr)
                .chars()
                .take(1000)
                .collect::<String>()
        ));
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}
fn launch(program: &str, args: &[OsString]) -> Result<Value> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(err)?;
    Ok(json!({"ok":true}))
}
pub fn desktop(core: &Core, args: &Value) -> Result<Value> {
    let action = str_arg(args, "action")?;
    if action == "apps" {
        return Ok(json!(
            [
                "konsole",
                "gnome-terminal",
                "kitty",
                "alacritty",
                "code",
                "cursor",
                "zed",
                "idea",
                "webstorm",
                "pycharm",
                "kate",
                "dolphin",
                "kdeconnect-cli",
                "claude",
                "codex",
                "gemini",
                "opencode",
                "copilot"
            ]
            .iter()
            .filter_map(|n| which(n).map(|p| json!({"id":n,"path":p})))
            .collect::<Vec<_>>()
        ));
    }
    if action == "share_devices" {
        let output = run(
            "kdeconnect-cli",
            ["--list-available", "--id-name-only"],
            None,
            8,
        )?;
        let devices = output
            .lines()
            .filter_map(|line| line.split_once(' '))
            .map(|(id, name)| json!({"id":id,"name":name.trim_start_matches([' ',':'])}))
            .collect::<Vec<_>>();
        return Ok(json!(devices));
    }
    if action == "open_url" {
        let url = url::Url::parse(str_arg(args, "url")?).map_err(err)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("只支持 HTTP 和 HTTPS 链接".into());
        }
        return launch("xdg-open", &[OsString::from(url.as_str())]);
    }
    let path = paths::decode(core, str_arg(args, "path")?)?;
    match action {
        "open" => launch("xdg-open", &[path.into_os_string()]),
        "reveal" => launch(
            "dolphin",
            &[OsString::from("--select"), path.into_os_string()],
        ),
        "terminal" | "agent" => {
            let dir = if path.is_dir() {
                path
            } else {
                path.parent().ok_or("找不到上级目录")?.to_owned()
            };
            let term = args["app"].as_str().unwrap_or("konsole");
            if !matches!(term, "konsole" | "gnome-terminal" | "kitty" | "alacritty") {
                return Err("请选择支持的终端".into());
            }
            let mut argv = match term {
                "konsole" => vec!["--workdir".into(), dir.into_os_string()],
                "gnome-terminal" => vec!["--working-directory".into(), dir.into_os_string()],
                "kitty" => vec!["--directory".into(), dir.into_os_string()],
                _ => vec!["--working-directory".into(), dir.into_os_string()],
            };
            if action == "agent" {
                let agent = str_arg(args, "agent")?;
                if !matches!(
                    agent,
                    "claude" | "codex" | "gemini" | "opencode" | "copilot"
                ) {
                    return Err("不支持的 Agent".into());
                }
                let exe = which(agent).ok_or("尚未安装此 Agent")?;
                argv.push(if term == "gnome-terminal" { "--" } else { "-e" }.into());
                argv.push(exe.into_os_string());
            }
            launch(term, &argv)
        }
        "editor" => {
            let app = args["app"].as_str().unwrap_or("code");
            if !matches!(
                app,
                "code" | "cursor" | "zed" | "idea" | "webstorm" | "pycharm" | "subl" | "kate"
            ) {
                return Err("不支持此编辑器".into());
            }
            launch(app, &[path.into_os_string()])
        }
        "share" => {
            let device = str_arg(args, "device")?;
            if device.is_empty() || device.len() > 200 || device.starts_with('-') {
                return Err("设备 ID 无效".into());
            }
            run(
                "kdeconnect-cli",
                [
                    std::ffi::OsStr::new("--device"),
                    device.as_ref(),
                    "--share".as_ref(),
                    path.as_os_str(),
                ],
                None,
                10,
            )?;
            Ok(json!({"ok":true}))
        }
        "email" => launch("xdg-email", &["--attach".into(), path.into_os_string()]),
        _ => Err("不支持的桌面动作".into()),
    }
}
pub fn quote(value: &OsStr) -> String {
    format!("'{}'", value.to_string_lossy().replace('\'', "'\\''"))
}
pub fn git(core: &Core, args: &Value) -> Result<Value> {
    let path = paths::decode(core, str_arg(args, "path")?)?;
    let root = match run("git", ["rev-parse", "--show-toplevel"], Some(&path), 4) {
        Ok(v) => v.trim().to_owned(),
        Err(_) => return Ok(json!({"repository":false})),
    };
    let branch = run("git", ["symbolic-ref", "--short", "HEAD"], Some(&path), 4)
        .unwrap_or_else(|_| "detached HEAD".into())
        .trim()
        .to_owned();
    let status = run(
        "git",
        [
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
        ],
        Some(&path),
        8,
    )?;
    let mut items = Vec::new();
    let mut parts = status.split('\0');
    while let Some(row) = parts.next() {
        if row.len() < 4 {
            continue;
        }
        let code = &row[..2];
        let name = &row[3..];
        items.push(json!({"status":code,"name":name,"path":paths::encode(&PathBuf::from(&root).join(name))}));
        if code.contains('R') || code.contains('C') {
            parts.next();
        }
    }
    Ok(
        json!({"repository":true,"root":root,"branch":branch,"changes":items,"count":items.len(),"untracked":items.iter().filter(|v|v["status"]=="??").count(),"deleted":items.iter().filter(|v|v["status"].as_str().is_some_and(|s|s.contains('D'))).count()}),
    )
}
