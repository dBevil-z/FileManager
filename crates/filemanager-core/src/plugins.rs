//! Capability-based extension host. Plugins are data + JavaScript, not native
//! libraries. Each invocation owns a fresh QuickJS heap and a bounded context.
use crate::{Core, Result, err, files, jobs, paths, str_arg};
use rquickjs::{Context, Function, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256, Sha512};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const MAX_PACKAGE: u64 = 512 * 1024;
const MAX_SOURCE: usize = 128 * 1024;
const API: u32 = 1;
const PERMISSIONS: &[&str] = &["context:metadata", "files:read", "directories:scan"];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub category: String,
    pub permissions: Vec<String>,
    pub placements: Vec<String>,
    #[serde(default)]
    pub inputs: Vec<Input>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub default: String,
}
#[derive(Clone)]
struct Package {
    manifest: Manifest,
    script: String,
    digest: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Installed {
    digest: String,
    version: String,
    enabled: bool,
    grants: Vec<String>,
    manifest: Manifest,
}
fn valid_id(s: &str) -> bool {
    (3..=80).contains(&s.len())
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'.')
        && !s.starts_with('.')
        && !s.contains("..")
}
fn version(s: &str) -> Option<Vec<u32>> {
    let p: Vec<_> = s
        .split('.')
        .map(str::parse::<u32>)
        .collect::<std::result::Result<_, _>>()
        .ok()?;
    (p.len() == 3).then_some(p)
}
fn package(manifest: &str, script: &str) -> Result<Package> {
    if manifest.len() > 16384 || script.len() > MAX_SOURCE {
        return Err("插件包超过大小限制".into());
    }
    let m: Manifest = serde_json::from_str(manifest).map_err(err)?;
    if m.schema_version != API
        || !valid_id(&m.id)
        || version(&m.version).is_none()
        || m.name.is_empty()
        || m.name.chars().count() > 60
        || m.description.len() > 1200
        || m.author.len() > 120
        || m.inputs.len() > 8
        || m.placements.is_empty()
        || m.placements
            .iter()
            .any(|p| !matches!(p.as_str(), "tools" | "context"))
        || !m.permissions.iter().any(|p| p == "context:metadata")
        || m.permissions
            .iter()
            .any(|p| !PERMISSIONS.contains(&p.as_str()))
    {
        return Err("插件清单版本、名称、权限或入口无效".into());
    }
    let mut ids = HashSet::new();
    for f in &m.inputs {
        if !valid_id(&f.id)
            || !ids.insert(&f.id)
            || f.label.len() > 120
            || !matches!(f.kind.as_str(), "text" | "select")
            || f.default.len() > 4096
            || f.options.len() > 32
            || f.options.iter().any(|s| s.len() > 200)
            || (f.kind == "select" && !f.options.contains(&f.default))
        {
            return Err("插件输入定义无效".into());
        }
    }
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(&m).map_err(err)?);
    hash.update([0]);
    hash.update(script.as_bytes());
    Ok(Package {
        manifest: m,
        script: script.into(),
        digest: format!("{:x}", hash.finalize()),
    })
}
fn bundled() -> Result<Vec<Package>> {
    [
        (
            include_str!("../../../plugins/official/checksums.json"),
            include_str!("../../../plugins/official/checksums.js"),
        ),
        (
            include_str!("../../../plugins/official/inventory.json"),
            include_str!("../../../plugins/official/inventory.js"),
        ),
        (
            include_str!("../../../plugins/official/text-inspector.json"),
            include_str!("../../../plugins/official/text-inspector.js"),
        ),
        (
            include_str!("../../../plugins/official/space-analyzer.json"),
            include_str!("../../../plugins/official/space-analyzer.js"),
        ),
    ]
    .into_iter()
    .map(|(a, b)| package(a, b))
    .collect()
}
fn read_package(path: &Path) -> Result<Package> {
    if !fs::metadata(path).map_err(err)?.is_file()
        || fs::metadata(path).map_err(err)?.len() > MAX_PACKAGE
    {
        return Err("请选择 512 KiB 以内的 .fmp 或 .zip 插件包".into());
    }
    let mut zip = zip::ZipArchive::new(fs::File::open(path).map_err(err)?).map_err(err)?;
    if zip.len() != 2 {
        return Err("插件包应仅包含 manifest.json 和 main.js".into());
    }
    let mut parts = HashMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(err)?;
        if !matches!(f.name(), "manifest.json" | "main.js")
            || f.is_dir()
            || f.unix_mode()
                .is_some_and(|m| m & libc::S_IFMT == libc::S_IFLNK)
            || f.size() > MAX_SOURCE as u64
            || parts.contains_key(f.name())
        {
            return Err("插件包包含非法路径、重复条目或过大文件".into());
        }
        let mut bytes = Vec::new();
        f.by_ref()
            .take(MAX_SOURCE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        if bytes.len() > MAX_SOURCE {
            return Err("插件解压大小超出限制".into());
        }
        parts.insert(f.name().to_owned(), String::from_utf8(bytes).map_err(err)?);
    }
    package(
        parts.get("manifest.json").ok_or("缺少清单")?,
        parts.get("main.js").ok_or("缺少脚本")?,
    )
}
fn source(core: &Core, args: &Value) -> Result<Package> {
    if let Some(id) = args["builtin"].as_str() {
        bundled()?
            .into_iter()
            .find(|p| p.manifest.id == id)
            .ok_or("找不到内置插件".into())
    } else {
        let p = read_package(&paths::decode(core, str_arg(args, "path")?)?)?;
        if p.manifest.id.starts_with("official.") {
            return Err("official. 是内置目录保留的命名空间，请使用自己的插件 id".into());
        }
        Ok(p)
    }
}
fn records(core: &Core) -> Result<HashMap<String, Installed>> {
    serde_json::from_value(
        core.get_value("plugins")?["data"]
            .as_object()
            .map(|v| json!(v))
            .unwrap_or(json!({})),
    )
    .map_err(err)
}
fn dir(core: &Core, id: &str, digest: &str) -> Result<PathBuf> {
    if !valid_id(id) || digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("插件记录无效".into());
    }
    Ok(core.data_dir.join("plugins").join(id).join(digest))
}
fn installed(core: &Core, id: &str, r: &Installed) -> Result<Package> {
    let base = dir(core, id, &r.digest)?;
    let read = |name: &str| -> Result<String> {
        let path = base.join(name);
        let m = fs::symlink_metadata(&path).map_err(err)?;
        if !m.is_file() || m.len() > MAX_SOURCE as u64 {
            return Err("插件文件异常".into());
        }
        fs::read_to_string(path).map_err(err)
    };
    let p = package(&read("manifest.json")?, &read("main.js")?)?;
    if p.digest != r.digest
        || p.manifest.id != id
        || p.manifest.permissions != r.grants
        || p.manifest.version != r.version
    {
        return Err("插件内容或权限已改变，请重新安装并审核".into());
    }
    Ok(p)
}
pub fn list(core: &Core) -> Result<Value> {
    let saved = records(core)?;
    let official = bundled()?;
    let mut all = Vec::new();
    for p in &official {
        let r = saved.get(&p.manifest.id);
        let broken = r.and_then(|r| installed(core, &p.manifest.id, r).err());
        all.push(json!({"manifest":r.map(|r|&r.manifest).unwrap_or(&p.manifest),"official":true,"installed":r.is_some(),"enabled":r.is_some_and(|r|r.enabled)&&broken.is_none(),"error":broken,"update":r.is_some_and(|r|version(&p.manifest.version)>version(&r.version)),"available_version":p.manifest.version}));
    }
    for (id, r) in &saved {
        if official.iter().any(|p| p.manifest.id == *id) {
            continue;
        }
        let broken = installed(core, id, r).err();
        all.push(json!({"manifest":r.manifest,"official":false,"installed":true,"enabled":r.enabled&&broken.is_none(),"error":broken,"update":false}));
    }
    all.sort_by_key(|v| v["manifest"]["id"].as_str().unwrap_or("").to_owned());
    Ok(
        json!({"plugins":all,"api_version":API,"runtime":"QuickJS","catalog":"bundled","remote_enabled":false}),
    )
}
pub fn inspect(core: &Core, args: &Value) -> Result<Value> {
    let p = source(core, args)?;
    Ok(json!({"manifest":p.manifest,"sha256":p.digest,"script_bytes":p.script.len()}))
}
pub fn install(core: &Core, args: &Value) -> Result<Value> {
    let p = source(core, args)?;
    if args["sha256"] != p.digest || args["grants"] != json!(p.manifest.permissions) {
        return Err("插件内容或权限与确认时不一致，请重新审核".into());
    }
    let mut saved = records(core)?;
    if saved.len() >= 64 && !saved.contains_key(&p.manifest.id) {
        return Err("最多安装 64 个插件".into());
    }
    if saved
        .get(&p.manifest.id)
        .is_some_and(|r| version(&p.manifest.version) < version(&r.version))
    {
        return Err("不允许降级覆盖插件；请先卸载旧版本".into());
    }
    let dst = dir(core, &p.manifest.id, &p.digest)?;
    fs::create_dir_all(dst.parent().unwrap()).map_err(err)?;
    let staging = dst.parent().unwrap().join(uuid::Uuid::new_v4().to_string());
    fs::create_dir(&staging).map_err(err)?;
    let result = (|| {
        fs::write(
            staging.join("manifest.json"),
            serde_json::to_vec_pretty(&p.manifest).map_err(err)?,
        )
        .map_err(err)?;
        fs::write(staging.join("main.js"), &p.script).map_err(err)?;
        // Identical reinstallation also verifies the existing files; never trust a
        // tampered directory just because its name matches a digest.
        if dst.exists() {
            fs::remove_dir_all(&dst).map_err(err)?;
        }
        fs::rename(&staging, &dst).map_err(err)?;
        saved.insert(
            p.manifest.id.clone(),
            Installed {
                digest: p.digest.clone(),
                version: p.manifest.version.clone(),
                enabled: true,
                grants: p.manifest.permissions.clone(),
                manifest: p.manifest.clone(),
            },
        );
        core.put_value("plugins", &json!(saved), None)?;
        Ok(json!({"ok":true,"id":p.manifest.id}))
    })();
    let _ = fs::remove_dir_all(staging);
    result
}
pub fn manage(core: &Core, args: &Value) -> Result<Value> {
    let id = str_arg(args, "id")?;
    let mut saved = records(core)?;
    let r = saved.get(id).ok_or("插件未安装")?.clone();
    if args["action"] == "remove" {
        saved.remove(id);
        core.put_value("plugins", &json!(saved), None)?;
        fs::remove_dir_all(dir(core, id, &r.digest)?.parent().unwrap()).map_err(err)?;
    } else if args["action"] == "enable" {
        if args["enabled"] == true {
            installed(core, id, &r)?;
        }
        saved.get_mut(id).unwrap().enabled = args["enabled"] == true;
        core.put_value("plugins", &json!(saved), None)?;
    } else {
        return Err("插件操作无效".into());
    }
    Ok(json!({"ok":true}))
}

#[derive(Clone)]
struct Scope {
    root: PathBuf,
    inode: u64,
    device: u64,
    recursive: bool,
}
impl Scope {
    fn new(p: &Path, recursive: bool) -> Result<Self> {
        let root = fs::canonicalize(p).map_err(err)?;
        let m = fs::metadata(&root).map_err(err)?;
        Ok(Self {
            root,
            inode: m.ino(),
            device: m.dev(),
            recursive: recursive && m.is_dir(),
        })
    }
    fn allows(&self, p: &Path, recursive: bool) -> bool {
        fs::metadata(&self.root).is_ok_and(|m| m.ino() == self.inode && m.dev() == self.device)
            && (p == self.root || (self.recursive && p.starts_with(&self.root) && recursive))
    }
}
fn deadline(start: Instant) -> Result<()> {
    jobs::checkpoint()?;
    if start.elapsed() > Duration::from_secs(30) {
        Err("插件运行超过 30 秒，请缩小范围后重试".into())
    } else {
        Ok(())
    }
}
fn host(
    core: &Core,
    selected: &[Scope],
    cwd: &Scope,
    grants: &[String],
    method: &str,
    args: &Value,
    start: Instant,
) -> Result<Value> {
    deadline(start)?;
    let permission = match method {
        "hash" | "readText" => "files:read",
        "listDirectory" => "context:metadata",
        "directoryStats" => "directories:scan",
        _ => return Err("插件 API 不存在".into()),
    };
    if !grants.iter().any(|p| p == permission) {
        return Err(format!("插件未获得权限：{permission}"));
    }
    let p = fs::canonicalize(paths::decode(core, str_arg(args, "path")?)?).map_err(err)?;
    let permitted = if method == "listDirectory" || method == "directoryStats" {
        cwd.allows(&p, false) || selected.iter().any(|s| s.allows(&p, true))
    } else {
        selected.iter().any(|s| s.allows(&p, true))
    };
    if !permitted {
        return Err("路径超出本次选择范围，或选中文件已被替换".into());
    }
    match method {
        "hash" => {
            let mut file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(&p)
                .map_err(err)?;
            let before = file.metadata().map_err(err)?;
            if !before.is_file() {
                return Err("只能校验普通文件".into());
            }
            let algo = args["algorithm"].as_str().unwrap_or("SHA-256");
            let mut a = Sha256::new();
            let mut b = Sha512::new();
            let mut buffer = [0u8; 64 * 1024];
            let mut bytes = 0;
            loop {
                deadline(start)?;
                let n = file.read(&mut buffer).map_err(err)?;
                if n == 0 {
                    break;
                }
                if algo == "SHA-256" {
                    a.update(&buffer[..n]);
                } else if algo == "SHA-512" {
                    b.update(&buffer[..n]);
                } else {
                    return Err("不支持此校验算法".into());
                }
                bytes += n as u64;
                jobs::progress(&paths::encode(&p), bytes, 0, 0);
            }
            let after = file.metadata().map_err(err)?;
            if before.len() != after.len()
                || before.mtime() != after.mtime()
                || before.mtime_nsec() != after.mtime_nsec()
                || before.ctime() != after.ctime()
                || before.ctime_nsec() != after.ctime_nsec()
            {
                return Err("校验期间文件发生改变".into());
            }
            Ok(
                json!({"hash":if algo=="SHA-256" {format!("{:x}",a.finalize())}else{format!("{:x}",b.finalize())},"bytes":bytes}),
            )
        }
        "readText" => {
            let mut file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(&p)
                .map_err(err)?;
            if !file.metadata().map_err(err)?.is_file() {
                return Err("只能读取普通文件".into());
            }
            let mut data = Vec::new();
            file.by_ref()
                .take(1024 * 1024 + 1)
                .read_to_end(&mut data)
                .map_err(err)?;
            if data.len() > 1024 * 1024 {
                return Err("文本插件只读取 1 MiB 以内的文件".into());
            }
            if data.contains(&0) {
                return Err("文件不是纯文本".into());
            }
            Ok(json!({"text":String::from_utf8(data).map_err(err)?}))
        }
        "listDirectory" => {
            if !p.is_dir() {
                return Err("请选择文件夹".into());
            }
            let mut entries = Vec::new();
            for entry in fs::read_dir(&p).map_err(err)?.take(5001) {
                deadline(start)?;
                if entries.len() >= 5000 {
                    return Err("清单超过 5000 项，请选择较小范围".into());
                }
                let e = entry.map_err(err)?;
                if let Ok(e) = files::entry(&e.path()) {
                    entries.push(e);
                }
            }
            entries.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(json!(entries))
        }
        "directoryStats" => {
            if !p.is_dir() {
                return Err("请选择文件夹".into());
            }
            let mut rows = Vec::new();
            let mut total = 0u64;
            let mut count = 0;
            let mut skipped = 0;
            let mut truncated = false;
            let mut visited = 0;
            for entry in walkdir::WalkDir::new(&p)
                .follow_links(false)
                .same_file_system(true)
                .max_depth(24)
                .into_iter()
            {
                deadline(start)?;
                let e = match entry {
                    Ok(e) => e,
                    Err(_) => {
                        skipped += 1;
                        continue;
                    }
                };
                visited += 1;
                if visited > 100000 {
                    truncated = true;
                    break;
                }
                if e.file_type().is_file() {
                    if let Ok(m) = e.metadata() {
                        total = total.saturating_add(m.len());
                        count += 1;
                        rows.push(json!({"name":e.file_name().to_string_lossy(),"path":paths::encode(e.path()),"size":m.len()}));
                    }
                }
                if rows.len() > 128 {
                    rows.sort_by_key(|v| std::cmp::Reverse(v["size"].as_u64().unwrap_or(0)));
                    rows.truncate(32);
                }
            }
            rows.sort_by_key(|v| std::cmp::Reverse(v["size"].as_u64().unwrap_or(0)));
            rows.truncate(32);
            Ok(
                json!({"files":count,"bytes":total,"largest":rows,"skipped":skipped,"truncated":truncated}),
            )
        }
        _ => unreachable!(),
    }
}
pub fn run(core: &Core, args: &Value) -> Result<Value> {
    let id = str_arg(args, "id")?;
    let saved = records(core)?;
    let r = saved.get(id).ok_or("插件未安装")?;
    if !r.enabled {
        return Err("插件已停用".into());
    }
    let p = installed(core, id, r)?;
    let cwd = Scope::new(&paths::decode(core, str_arg(args, "directory")?)?, false)?;
    if !cwd.root.is_dir() {
        return Err("当前目录无效".into());
    }
    let mut selection = Vec::new();
    let mut scopes = Vec::new();
    for value in args["selection"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .take(257)
    {
        if selection.len() >= 256 {
            return Err("每次最多处理 256 个选中项目".into());
        }
        let path = paths::decode(core, value.as_str().ok_or("所选路径无效")?)?;
        // Only the host supplies this context; scripts cannot add capabilities.
        let scope = Scope::new(&path, true)?;
        selection.push(files::entry(&path)?);
        scopes.push(scope);
    }
    let inputs = args["inputs"].as_object().cloned().unwrap_or_default();
    for (key, val) in &inputs {
        let f = p
            .manifest
            .inputs
            .iter()
            .find(|f| &f.id == key)
            .ok_or("未知插件输入")?;
        let s = val.as_str().ok_or("插件输入必须为文本")?;
        if s.len() > 4096 || (f.kind == "select" && !f.options.iter().any(|v| v == s)) {
            return Err("插件输入无效".into());
        }
    }
    let mut merged = serde_json::Map::new();
    for f in &p.manifest.inputs {
        merged.insert(
            f.id.clone(),
            inputs.get(&f.id).cloned().unwrap_or(json!(f.default)),
        );
    }
    let context = json!({"directory":paths::encode(&cwd.root),"selection":selection,"inputs":merged,"api_version":API});
    let rt = Runtime::new().map_err(err)?;
    rt.set_memory_limit(32 * 1024 * 1024);
    rt.set_max_stack_size(512 * 1024);
    rt.set_gc_threshold(4 * 1024 * 1024);
    let start = Instant::now();
    rt.set_interrupt_handler(Some(Box::new(move || deadline(start).is_err())));
    // Do not expose Atomics.wait, shared buffers, promises or host event loops:
    // a blocking intrinsic would bypass the bytecode interrupt/deadline checks.
    use rquickjs::context::intrinsic;
    let ctx = Context::custom::<(
        intrinsic::Date,
        intrinsic::Eval,
        intrinsic::Json,
        intrinsic::RegExpCompiler,
        intrinsic::RegExp,
        intrinsic::MapSet,
    )>(&rt)
    .map_err(err)?;
    let core = core.clone();
    let grants = r.grants.clone();
    let result = ctx.with(|ctx| -> Result<Value> {
        let bridge = Function::new(ctx.clone(), move |method: String, body: String| -> String {
            let value = serde_json::from_str::<Value>(&body)
                .map_err(err)
                .and_then(|v| host(&core, &scopes, &cwd, &grants, &method, &v, start));
            match value {
                Ok(v) => json!({"data":v}).to_string(),
                Err(e) => json!({"error":e}).to_string(),
            }
        })
        .map_err(err)?;
        ctx.globals().set("__host", bridge).map_err(err)?;
        ctx.globals()
            .set("__context", context.to_string())
            .map_err(err)?;
        let script = format!(
            "'use strict';\n{}\n{}\nJSON.stringify(main(JSON.parse(__context), api));",
            SDK, p.script
        );
        let output: String = ctx.eval(script).map_err(|e| {
            let exception = ctx.catch();
            let message = exception
                .as_object()
                .and_then(|o| o.get::<_, String>("message").ok())
                .unwrap_or_else(|| e.to_string());
            format!("插件执行失败：{message}")
        })?;
        if output.len() > 2 * 1024 * 1024 {
            return Err("插件输出超过 2 MiB".into());
        }
        let value: Value = serde_json::from_str(&output).map_err(err)?;
        if !value.is_object() {
            return Err("插件应返回结果对象，异步任务不属于 API v1".into());
        }
        Ok(value)
    })?;
    validate_output(&result)?;
    Ok(
        json!({"output":result,"elapsed_ms":start.elapsed().as_millis(),"plugin":p.manifest,"runtime":"QuickJS · 32 MiB"}),
    )
}
fn validate_output(value: &Value) -> Result<()> {
    if value["title"].as_str().is_none_or(|s| s.len() > 200) {
        return Err("插件结果缺少标题".into());
    }
    if let Some(rows) = value["rows"].as_array() {
        if rows.len() > 5000
            || rows.iter().any(|r| {
                !r.is_array()
                    || r.as_array().unwrap().len() > 12
                    || r.as_array().unwrap().iter().any(|c| {
                        !c.is_string() && !c.is_number() && !c.is_null() && !c.is_boolean()
                    })
            })
        {
            return Err("插件结果表格无效".into());
        }
    }
    if let Some(cols) = value["columns"].as_array() {
        if cols.len() > 12
            || cols
                .iter()
                .any(|v| v.as_str().is_none_or(|s| s.len() > 120))
        {
            return Err("插件表头无效".into());
        }
    }
    if let Some(reports) = value["reports"].as_array() {
        if reports.len() > 8
            || reports.iter().any(|v| {
                v["name"]
                    .as_str()
                    .is_none_or(|s| paths::check_name(s).is_err())
                    || v["content"].as_str().is_none_or(|s| s.len() > 1024 * 1024)
            })
        {
            return Err("插件报告无效".into());
        }
    }
    Ok(())
}
const SDK: &str = r#"
const api = Object.freeze({
  call(method, args) { const r=JSON.parse(__host(method,JSON.stringify(args))); if(r.error) throw new Error(r.error); return r.data; },
  hash(path, algorithm='SHA-256') { return this.call('hash',{path,algorithm}); },
  readText(path) { return this.call('readText',{path}).text; },
  listDirectory(path) { return this.call('listDirectory',{path}); },
  directoryStats(path) { return this.call('directoryStats',{path}); }
});
"#;
