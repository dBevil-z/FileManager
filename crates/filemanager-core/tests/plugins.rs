use filemanager_core::Core;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};
fn setup() -> (tempfile::TempDir, Core) {
    let t = tempfile::tempdir().unwrap();
    let c = Core::with_paths(
        t.path().join("state"),
        t.path().join("home"),
        t.path().join("Trash"),
    )
    .unwrap();
    fs::create_dir_all(&c.home_dir).unwrap();
    (t, c)
}
fn call(c: &Core, op: &str, args: Value) -> Result<Value, String> {
    c.request(json!({"op":op,"args":args}))
}
fn install(c: &Core, source: Value) -> Value {
    let info = call(c, "plugin_inspect", source.clone()).unwrap();
    let mut args = source;
    args["sha256"] = info["sha256"].clone();
    args["grants"] = info["manifest"]["permissions"].clone();
    call(c, "plugin_install", args).unwrap();
    info
}
fn manifest(id: &str, permissions: &[&str]) -> Value {
    json!({"schema_version":1,"id":id,"name":"测试插件","version":"1.0.0","description":"测试能力边界","author":"Test","category":"测试","permissions":permissions,"placements":["tools"],"inputs":[]})
}
fn pack(path: &Path, m: &Value, script: &str) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(m.to_string().as_bytes()).unwrap();
    zip.start_file("main.js", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(script.as_bytes()).unwrap();
    zip.finish().unwrap();
}
#[test]
fn bundled_plugins_install_run_disable_and_remove_without_touching_sources() {
    let (_t, c) = setup();
    let f = c.home_dir.join("hello.txt");
    fs::write(&f, "abc\n").unwrap();
    let all = call(&c, "plugins_list", json!({})).unwrap();
    assert_eq!(all["plugins"].as_array().unwrap().len(), 4);
    assert_eq!(all["remote_enabled"], false);
    for (id, expected) in [
        ("official.checksums", "文件校验"),
        ("official.inventory", "文件清单"),
        ("official.text-inspector", "文本健康检查"),
        ("official.space-analyzer", "目录空间分析"),
    ] {
        install(&c, json!({"builtin":id}));
        let selection = if id == "official.space-analyzer" {
            json!([])
        } else {
            json!([f])
        };
        let result = call(
            &c,
            "plugin_run",
            json!({"id":id,"directory":c.home_dir,"selection":selection}),
        )
        .unwrap();
        assert_eq!(result["output"]["title"], expected);
        assert!(result["output"]["reports"].is_array());
    }
    let result = call(
        &c,
        "plugin_run",
        json!({"id":"official.checksums","directory":c.home_dir,"selection":[f]}),
    )
    .unwrap();
    assert_eq!(
        result["output"]["rows"][0][1],
        "edeaaff3f1774ad2888673770c6d64097e391bc362d7d6fb34982ddf0efd18cb"
    );
    call(
        &c,
        "plugin_manage",
        json!({"id":"official.checksums","action":"enable","enabled":false}),
    )
    .unwrap();
    assert!(
        call(
            &c,
            "plugin_run",
            json!({"id":"official.checksums","directory":c.home_dir})
        )
        .unwrap_err()
        .contains("停用")
    );
    call(
        &c,
        "plugin_manage",
        json!({"id":"official.checksums","action":"remove"}),
    )
    .unwrap();
    assert_eq!(fs::read_to_string(f).unwrap(), "abc\n");
}
#[test]
fn installation_requires_exact_review_and_content_integrity() {
    let (t, c) = setup();
    let p = t.path().join("test.fmp");
    pack(
        &p,
        &manifest("example.test", &["context:metadata"]),
        "function main(){return {title:'ok'};}",
    );
    let info = call(&c, "plugin_inspect", json!({"path":p})).unwrap();
    assert!(
        call(
            &c,
            "plugin_install",
            json!({"path":p,"sha256":info["sha256"],"grants":["files:read"]})
        )
        .is_err()
    );
    install(&c, json!({"path":p}));
    let script = c
        .data_dir
        .join("plugins/example.test")
        .join(info["sha256"].as_str().unwrap())
        .join("main.js");
    fs::write(script, "function main(){return {title:'changed'};}").unwrap();
    assert!(
        call(
            &c,
            "plugin_run",
            json!({"id":"example.test","directory":c.home_dir})
        )
        .unwrap_err()
        .contains("改变")
    );
}
#[test]
fn scripts_have_no_process_network_dom_or_unscoped_file_access() {
    let (t, c) = setup();
    let chosen = c.home_dir.join("chosen.txt");
    let secret = t.path().join("outside.txt");
    fs::write(&chosen, "chosen").unwrap();
    fs::write(&secret, "never disclose").unwrap();
    let p = t.path().join("test.fmp");
    let script = format!(
        r#"function main(c,api){{if(typeof process!=='undefined'||typeof require!=='undefined'||typeof fetch!=='undefined'||typeof window!=='undefined'||typeof std!=='undefined'||typeof Atomics!=='undefined'||typeof SharedArrayBuffer!=='undefined'||typeof Promise!=='undefined')throw Error('unsafe host');try{{api.readText({});throw Error('escaped');}}catch(e){{if(e.message==='escaped')throw e;}}return {{title:'isolated',rows:[[api.readText(c.selection[0].path)]]}};}}"#,
        json!(secret)
    );
    pack(
        &p,
        &manifest("example.isolation", &["context:metadata", "files:read"]),
        &script,
    );
    install(&c, json!({"path":p}));
    let result = call(
        &c,
        "plugin_run",
        json!({"id":"example.isolation","directory":c.home_dir,"selection":[chosen]}),
    )
    .unwrap();
    assert_eq!(result["output"]["rows"][0][0], "chosen");
}
#[test]
fn permission_checks_cannot_be_bypassed_through_raw_bridge() {
    let (t, c) = setup();
    let chosen = c.home_dir.join("chosen.txt");
    fs::write(&chosen, "private").unwrap();
    let p = t.path().join("test.fmp");
    pack(
        &p,
        &manifest("example.denied", &["context:metadata"]),
        "function main(c,api){return {title:api.readText(c.selection[0].path)};}",
    );
    install(&c, json!({"path":p}));
    assert!(
        call(
            &c,
            "plugin_run",
            json!({"id":"example.denied","directory":c.home_dir,"selection":[chosen]})
        )
        .is_err()
    );
}
#[test]
fn descendant_symlinks_do_not_expand_granted_directory_scope() {
    let (t, c) = setup();
    let dir = c.home_dir.join("selected");
    fs::create_dir(&dir).unwrap();
    let outside = t.path().join("outside.txt");
    fs::write(&outside, "secret").unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("link.txt")).unwrap();
    let p = t.path().join("test.fmp");
    pack(
        &p,
        &manifest("example.symlink", &["context:metadata", "files:read"]),
        "function main(c,api){return {title:api.readText(c.selection[0].path+'/link.txt')};}",
    );
    install(&c, json!({"path":p}));
    assert!(
        call(
            &c,
            "plugin_run",
            json!({"id":"example.symlink","directory":c.home_dir,"selection":[dir]})
        )
        .is_err()
    );
}
#[test]
fn packages_reject_path_traversal_extra_entries_and_unknown_permissions() {
    let (t, c) = setup();
    let p = t.path().join("test.fmp");
    pack(
        &p,
        &manifest("example.bad", &["context:metadata", "shell:execute"]),
        "function main(){return {title:'bad'}}",
    );
    assert!(call(&c, "plugin_inspect", json!({"path":p})).is_err());
    let mut zip = zip::ZipWriter::new(fs::File::create(&p).unwrap());
    zip.start_file("../main.js", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"script").unwrap();
    zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(
        manifest("example.bad", &["context:metadata"])
            .to_string()
            .as_bytes(),
    )
    .unwrap();
    zip.finish().unwrap();
    assert!(call(&c, "plugin_inspect", json!({"path":p})).is_err());
}
#[test]
fn cancelled_infinite_loop_is_interrupted_and_memory_exhaustion_is_contained() {
    let (t, c) = setup();
    let p = t.path().join("loop.fmp");
    pack(
        &p,
        &manifest("example.loop", &["context:metadata"]),
        "function main(){while(true){}}",
    );
    install(&c, json!({"path":p}));
    let id = call(
        &c,
        "job_start",
        json!({"op":"plugin_run","args":{"id":"example.loop","directory":c.home_dir}}),
    )
    .unwrap()["id"]
        .clone();
    std::thread::sleep(Duration::from_millis(100));
    call(&c, "job_cancel", json!({"id":id})).unwrap();
    let start = Instant::now();
    loop {
        let jobs = call(&c, "jobs", json!({})).unwrap();
        let j = jobs
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["id"] == id)
            .unwrap();
        if j["status"] == "cancelled" {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    pack(
        &p,
        &manifest("example.memory", &["context:metadata"]),
        "function main(){const a=[];for(let i=0;i<100000000;i++)a.push('x'.repeat(8192));return {title:'never'};}",
    );
    install(&c, json!({"path":p}));
    assert!(
        call(
            &c,
            "plugin_run",
            json!({"id":"example.memory","directory":c.home_dir})
        )
        .is_err()
    );
    let info = call(&c, "plugins_list", json!({})).unwrap();
    assert!(info.is_object());
}
#[test]
fn inventory_csv_neutralizes_spreadsheet_formulas() {
    let (_t, c) = setup();
    let f = c.home_dir.join("=1+1.txt");
    fs::write(&f, "hello").unwrap();
    install(&c, json!({"builtin":"official.inventory"}));
    let v = call(
        &c,
        "plugin_run",
        json!({"id":"official.inventory","directory":c.home_dir,"selection":[f]}),
    )
    .unwrap();
    assert!(
        v["output"]["reports"][0]["content"]
            .as_str()
            .unwrap()
            .contains("\"'=1+1.txt\"")
    );
}
