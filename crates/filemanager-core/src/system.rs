use crate::{Core, Result, err, process};
use serde_json::{Value, json};
use std::{fs, os::unix::ffi::OsStrExt, path::Path};
pub fn disk(path: &Path) -> Option<Value> {
    let p = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut s = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(p.as_ptr(), s.as_mut_ptr()) } != 0 {
        return None;
    }
    let s = unsafe { s.assume_init() };
    Some(
        json!({"path":path,"total":s.f_blocks*s.f_frsize,"free":s.f_bavail*s.f_frsize,"used":(s.f_blocks-s.f_bfree)*s.f_frsize}),
    )
}
pub fn mounts() -> Result<Value> {
    let content = fs::read_to_string("/proc/self/mountinfo").map_err(err)?;
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in content.lines() {
        let Some((a, b)) = line.split_once(" - ") else {
            continue;
        };
        let left: Vec<_> = a.split_whitespace().collect();
        let right: Vec<_> = b.split_whitespace().collect();
        if left.len() < 6 || right.len() < 2 {
            continue;
        }
        let path = left[4]
            .replace("\\040", " ")
            .replace("\\011", "\t")
            .replace("\\134", "\\");
        if !(path == "/"
            || path == "/home"
            || path.starts_with("/media/")
            || path.starts_with("/mnt/")
            || path.starts_with("/run/media/"))
            || !seen.insert(path.clone())
        {
            continue;
        }
        rows.push(json!({"path":path,"name":Path::new(&path).file_name().unwrap_or(std::ffi::OsStr::new("文件系统")).to_string_lossy(),"filesystem":right[0],"source":right[1],"read_only":left[5].split(',').any(|s|s=="ro"),"disk":disk(Path::new(&path))}));
    }
    Ok(json!(rows))
}
pub fn info(core: &Core, args: &Value) -> Result<Value> {
    let hide = args["hide_names"] == true;
    let os = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let value = |name: &str| {
        os.lines()
            .find_map(|l| l.strip_prefix(&format!("{name}=")))
            .map(|s| s.trim_matches('"').to_owned())
    };
    let cpu = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let model = cpu.lines().find_map(|l| {
        l.strip_prefix("model name")
            .and_then(|s| s.split_once(':'))
            .map(|(_, s)| s.trim())
    });
    let mem = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let kb = |key: &str| {
        mem.lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse::<u64>().ok())
            .map(|n| n * 1024)
    };
    let batteries:Vec<_>=fs::read_dir("/sys/class/power_supply").into_iter().flatten().filter_map(|v|v.ok()).filter(|v|fs::read_to_string(v.path().join("type")).is_ok_and(|s|s.trim()=="Battery")).map(|v|json!({"name":v.file_name().to_string_lossy(),"capacity":fs::read_to_string(v.path().join("capacity")).ok().and_then(|v|v.trim().parse::<u64>().ok()),"status":fs::read_to_string(v.path().join("status")).ok().map(|s|s.trim().to_owned())})).collect();
    let displays = process::run("xrandr", ["--current"], None, 3)
        .ok()
        .map(|s| {
            s.lines()
                .filter(|l| l.contains(" connected"))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        });
    let secureboot = fs::read_dir("/sys/firmware/efi/efivars")
        .ok()
        .and_then(|v| {
            v.filter_map(|v| v.ok())
                .find(|e| e.file_name().to_string_lossy().starts_with("SecureBoot-"))
        })
        .and_then(|v| fs::read(v.path()).ok())
        .and_then(|v| v.get(4).copied())
        .map(|b| b == 1);
    let disks = mounts()?;
    Ok(
        json!({"os":value("PRETTY_NAME"),"kernel":fs::read_to_string("/proc/sys/kernel/osrelease").ok().map(|s|s.trim().to_owned()),"hostname":if hide{None}else{fs::read_to_string("/etc/hostname").ok().map(|s|s.trim().to_owned())},"user":if hide{None}else{core.home_dir.file_name().map(|s|s.to_string_lossy().into_owned())},"cpu":model,"threads":std::thread::available_parallelism().map(|v|v.get()).unwrap_or(1),"memory":{"total":kb("MemTotal:"),"available":kb("MemAvailable:"),"swap_total":kb("SwapTotal:"),"swap_free":kb("SwapFree:")},"disks":disks,"home_disk":disk(&core.home_dir),"batteries":batteries,"displays":displays,"security":{"secure_boot":secureboot,"apparmor":fs::read_to_string("/sys/module/apparmor/parameters/enabled").ok().map(|s|s.trim()=="Y"),"disk_encryption":process::run("lsblk",["-J","-o","NAME,TYPE,FSTYPE,MOUNTPOINTS"],None,3).ok().and_then(|s|serde_json::from_str::<Value>(&s).ok())},"uptime_seconds":fs::read_to_string("/proc/uptime").ok().and_then(|s|s.split_whitespace().next().and_then(|s|s.parse::<f64>().ok()))}),
    )
}

/// PSS accounts for shared pages once; RSS is also returned for task-manager comparison.
/// Read only this application's process tree, not other applications' command lines.
pub fn resources() -> Result<Value> {
    let mut pending = vec![std::process::id()];
    let mut seen = std::collections::HashSet::new();
    let mut rows = Vec::new();
    while let Some(pid) = pending.pop() {
        if !seen.insert(pid) || seen.len() > 64 {
            continue;
        }
        let base = std::path::PathBuf::from(format!("/proc/{pid}"));
        let Ok(status) = fs::read_to_string(base.join("status")) else {
            continue;
        };
        let field = |key: &str| {
            status
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        };
        if field("Uid:") != Some(unsafe { libc::getuid() } as u64) {
            continue;
        }
        let pss = fs::read_to_string(base.join("smaps_rollup"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .find_map(|l| l.strip_prefix("Pss:"))
                    .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
            });
        let rss = field("VmRSS:").unwrap_or(0) * 1024;
        let name = status
            .lines()
            .find_map(|l| l.strip_prefix("Name:"))
            .unwrap_or("")
            .trim()
            .to_owned();
        rows.push(json!({"pid":pid,"name":name,"pss":pss.map(|v|v*1024),"rss":rss,"threads":field("Threads:")}));
        for task in fs::read_dir(base.join("task"))
            .into_iter()
            .flatten()
            .flatten()
        {
            if let Ok(children) = fs::read_to_string(task.path().join("children")) {
                pending.extend(
                    children
                        .split_whitespace()
                        .filter_map(|v| v.parse::<u32>().ok()),
                );
            }
        }
    }
    Ok(
        json!({"pss":if rows.iter().any(|r|r["pss"].is_null()){None}else{Some(rows.iter().filter_map(|r|r["pss"].as_u64()).sum::<u64>())},"rss":rows.iter().filter_map(|r|r["rss"].as_u64()).sum::<u64>(),"processes":rows.len(),"rows":rows}),
    )
}
