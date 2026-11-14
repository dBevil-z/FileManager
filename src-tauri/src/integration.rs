use crate::{Core, NativeState};
use filemanager_core::{files, paths};
use serde_json::{Value, json};
use std::{collections::HashMap, ffi::OsString, os::unix::process::CommandExt, path::Path};
use tauri::{Emitter, Manager};

/// Normalize paths before the single-instance plugin reads process arguments.
/// AppRun changes directory to find its bundled WebKit helpers; OWD is only
/// used for resolving user paths, never as a new process working directory.
fn invocation_directory() -> std::path::PathBuf {
    if std::env::var_os("APPIMAGE").is_some() {
        if let Some(path) = std::env::var_os("OWD").filter(|p| Path::new(p).is_absolute()) {
            return path.into();
        }
    }
    std::env::current_dir().unwrap_or_else(|error| {
        eprintln!("无法读取调用方的工作目录：{error}");
        std::process::exit(1);
    })
}

fn normalize_arguments(args: &[OsString], cwd: &Path) -> Vec<String> {
    let mut normalized = vec![args[0].to_string_lossy().into_owned()];
    let mut i = 1;
    let path = |input: &OsString| {
        if let Some(text) = input.to_str() {
            if text == "~"
                || text.starts_with("~/")
                || text.starts_with("filemanager:")
                || text.starts_with("fileos:bytes:")
                || text.starts_with("file://")
            {
                return text.to_owned();
            }
        }
        paths::encode(&cwd.join(input))
    };
    while let Some(arg) = args.get(i) {
        let text = arg.to_string_lossy();
        match text.as_ref() {
            "--open" | "--reveal" => {
                normalized.push(text.into_owned());
                i += 1;
                if let Some(value) = args.get(i) {
                    normalized.push(path(value));
                }
            }
            "--bookmark" => {
                normalized.push(text.into_owned());
                i += 1;
                if let Some(value) = args.get(i) {
                    normalized.push(value.to_string_lossy().into_owned());
                }
            }
            "--search" => {
                normalized.push(text.into_owned());
                i += 1;
                if let Some(value) = args.get(i) {
                    normalized.push(value.to_string_lossy().into_owned());
                }
                i += 1;
                if let Some(value) = args.get(i) {
                    normalized.push(path(value));
                }
            }
            _ if text.starts_with('-') => normalized.push(text.into_owned()),
            _ => normalized.push(path(arg)),
        }
        i += 1;
    }
    normalized
}

pub fn arguments() -> Vec<String> {
    let args: Vec<OsString> = std::env::args_os().collect();
    let normalized = normalize_arguments(&args, &invocation_directory());
    if args
        .iter()
        .skip(1)
        .zip(normalized.iter().skip(1))
        .any(|(raw, converted)| raw != std::ffi::OsStr::new(converted))
    {
        let error =
            std::process::Command::new(std::env::current_exe().expect("FileManager executable"))
                .args(&normalized[1..])
                .exec();
        eprintln!("{error}");
        std::process::exit(1);
    }
    normalized
}

pub fn static_cli(args: &[String]) -> bool {
    match args.get(1).map(String::as_str) {
        Some("--help" | "-h") => println!(
            "FileManager\n  --open PATH\n  --reveal PATH\n  --search QUERY [ROOT]\n  --new-workspace\n  --bookmark URL\n  --shelf\n  --clean\n  --background\n  --version\n  filemanager://open?path=PATH"
        ),
        Some("--version" | "-V") => println!("FileManager {}", env!("CARGO_PKG_VERSION")),
        _ => return false,
    }
    true
}

/// Read-only search can run without GTK or an existing instance.
pub fn cli(core: &Core, args: &[String]) -> bool {
    if args.get(1).map(String::as_str) != Some("--search") {
        return false;
    }
    let cwd = invocation_directory();
    let query = args.get(2).map(String::as_str).unwrap_or("");
    let root = args
        .get(3)
        .and_then(|arg| path_argument(arg, &cwd))
        .unwrap_or_else(|| paths::encode(&core.home_dir));
    match core.request(json!({"op":"search","args":{"query":query,"roots":[root],"limit":100}})) {
        Ok(result) => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
    true
}

fn path_argument(input: &str, cwd: &Path) -> Option<String> {
    if input.starts_with("file://") {
        return url::Url::parse(input)
            .ok()?
            .to_file_path()
            .ok()
            .map(|path| paths::encode(&path));
    }
    if input == "~" || input.starts_with("~/") || input.starts_with("fileos:bytes:") {
        return Some(input.to_owned());
    }
    Some(paths::encode(&cwd.join(input)))
}

pub fn route(app: &tauri::AppHandle, value: Value) {
    if value["action"] == "shelf" {
        let app = app.clone();
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Err(e) = crate::shelf::show(&handle, true) {
                eprintln!("Drop Shelf: {e}");
            }
        });
        return;
    }
    if let Ok(mut pending) = app.state::<NativeState>().pending_route.lock() {
        *pending = Some(value.clone());
    }
    // Until the frontend acknowledges a route it stays available through initial_route.
    let _ = app.emit_to("main", "filemanager-route", value);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn route_arguments(app: &tauri::AppHandle, args: &[String], cwd: &Path) -> bool {
    let mut routed = false;
    let mut it = args.iter().skip(1);
    while let Some(arg) = it.next() {
        let value = match arg.as_str() {
            "--open" => it
                .next()
                .and_then(|path| path_argument(path, cwd))
                .map(|path| json!({"path":path})),
            "--reveal" => it.next().and_then(|input| path_argument(input, cwd)).and_then(|path| {
                let entry = paths::decode(&app.state::<Core>(), &path).ok()?;
                Some(json!({"path":paths::encode(entry.parent()?), "select":paths::encode(&entry)}))
            }),
            "--new-workspace" => Some(json!({"action":"new-workspace"})),
            "--bookmark" => it.next().map(|url| json!({"action":"bookmark","url":url})),
            "--shelf" => Some(json!({"action":"shelf"})),
            "--clean" => Some(json!({"page":{"type":"clean"}})),
            _ if arg.starts_with("filemanager://") => parse_url(arg),
            _ if !arg.starts_with('-') => path_argument(arg, cwd).map(|path| json!({"path":path})),
            _ => None,
        };
        if let Some(value) = value {
            routed = true;
            route(app, value);
        }
    }
    routed
}

fn parse_url(input: &str) -> Option<Value> {
    let url = url::Url::parse(input).ok()?;
    if url.scheme() != "filemanager" {
        return None;
    }
    let params: HashMap<_, _> = url.query_pairs().into_owned().collect();
    match url.host_str()? {
        "open" => Some(json!({"path":params.get("path")?})),
        "workspace" => Some(json!({"page":{"type":"workspace","id":params.get("id")?}})),
        "new-workspace" => Some(json!({"action":"new-workspace"})),
        "bookmark" => Some(json!({"action":"bookmark","url":params.get("url")?})),
        "shelf" => Some(json!({"action":"shelf"})),
        "clean" => Some(json!({"page":{"type":"clean"}})),
        _ => None,
    }
}

type Match = (
    String,
    String,
    String,
    i32,
    f64,
    HashMap<String, zbus::zvariant::OwnedValue>,
);
struct Runner {
    core: Core,
    app: tauri::AppHandle,
}
#[zbus::interface(name = "org.kde.krunner1")]
impl Runner {
    #[zbus(name = "Actions")]
    fn actions(&self) -> Vec<(String, String, String)> {
        Vec::new()
    }

    #[zbus(name = "Match")]
    fn matches(&self, query: &str) -> Vec<Match> {
        let lower = query.to_lowercase();
        let Some(query) = lower.strip_prefix("filemanager") else {
            return Vec::new();
        };
        if !query.is_empty() && !query.starts_with(char::is_whitespace) {
            return Vec::new();
        }
        let query = query.trim();
        let mut result = Vec::new();
        let mut push = |route: Value, text: String, icon: &str, score: f64| {
            result.push((
                route.to_string(),
                text,
                icon.into(),
                100,
                score,
                HashMap::new(),
            ));
        };
        if query.is_empty() {
            push(
                json!({}),
                "打开 FileManager".into(),
                "system-file-manager",
                1.0,
            );
        }
        for (name, page) in [
            ("智能清理 clean", "clean"),
            ("软件包 packages", "packages"),
            ("设置 settings", "settings"),
            ("AI Agents", "agents"),
        ] {
            if name.to_lowercase().contains(query) {
                push(
                    json!({"page":{"type":page}}),
                    format!("FileManager · {name}"),
                    "system-file-manager",
                    0.8,
                );
            }
        }
        if let Ok(catalog) = self.core.request(json!({"op":"catalog_get"})) {
            let settings = self
                .core
                .request(json!({"op":"catalog_get","args":{"namespace":"settings"}}))
                .unwrap_or(Value::Null);
            if let Some(workspaces) = catalog["data"]["workspaces"]
                .as_array()
                .filter(|_| settings["data"]["indexWorkspaces"] != false)
            {
                for w in workspaces
                    .iter()
                    .filter(|w| {
                        w["archived"] != true
                            && w["name"]
                                .as_str()
                                .unwrap_or("")
                                .to_lowercase()
                                .contains(query)
                    })
                    .take(8)
                {
                    push(
                        json!({"page":{"type":"workspace","id":w["id"]}}),
                        format!("工作空间 · {}", w["name"].as_str().unwrap_or("")),
                        "folder",
                        0.9,
                    );
                }
            }
            if let Some(collections) = catalog["data"]["collections"]
                .as_array()
                .filter(|_| settings["data"]["indexCollections"] != false)
            {
                for collection in collections
                    .iter()
                    .filter(|c| {
                        c["name"]
                            .as_str()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(query)
                    })
                    .take(8)
                {
                    push(
                        json!({"page":{"type":"bookmarks","id":collection["id"]}}),
                        format!("集合 · {}", collection["name"].as_str().unwrap_or("")),
                        "bookmarks",
                        0.85,
                    );
                }
            }
            if let Some(bookmarks) = catalog["data"]["bookmarks"]
                .as_array()
                .filter(|_| settings["data"]["indexBookmarks"] != false)
            {
                for b in bookmarks
                    .iter()
                    .filter(|b| {
                        format!("{} {}", b["title"], b["url"])
                            .to_lowercase()
                            .contains(query)
                    })
                    .take(8)
                {
                    push(
                        json!({"url":b["url"]}),
                        format!("书签 · {}", b["title"].as_str().unwrap_or("")),
                        "internet-web-browser",
                        0.7,
                    );
                }
            }
        }
        if !query.is_empty() {
            for place in files::places(&self.core)
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| {
                    p["name"]
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(query)
                })
            {
                push(
                    json!({"path":place["path"]}),
                    format!("FileManager · {}", place["name"].as_str().unwrap_or("")),
                    "folder",
                    0.8,
                );
            }
        }
        result
    }

    #[zbus(name = "Run")]
    fn run(&self, id: &str, _action: &str) {
        if let Ok(value) = serde_json::from_str::<Value>(id) {
            if let Some(url) = value["url"].as_str() {
                let _ = filemanager_core::process::desktop(
                    &self.core,
                    &json!({"action":"open_url","url":url}),
                );
            } else {
                route(&self.app, value);
            }
        }
    }
}

struct Actions {
    app: tauri::AppHandle,
}
#[zbus::interface(name = "io.filemanager.Actions")]
impl Actions {
    #[zbus(name = "Open")]
    fn open(&self, path: &str) {
        route(&self.app, json!({"path":path}));
    }
    #[zbus(name = "NewWorkspace")]
    fn new_workspace(&self) {
        route(&self.app, json!({"action":"new-workspace"}));
    }
    #[zbus(name = "AddBookmark")]
    fn add_bookmark(&self, url: &str) {
        route(&self.app, json!({"action":"bookmark","url":url}));
    }
    #[zbus(name = "Shelf")]
    fn shelf(&self) {
        route(&self.app, json!({"action":"shelf"}));
    }
    #[zbus(name = "HideShelf")]
    fn hide_shelf(&self) {
        let app = self.app.clone();
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(window) = handle.get_webview_window("shelf") {
                let _ = window.hide();
            }
        });
    }
    #[zbus(name = "Clean")]
    fn clean(&self) {
        route(&self.app, json!({"page":{"type":"clean"}}));
    }
    #[zbus(name = "Version")]
    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }
}

pub fn start_dbus(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let result = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name("io.filemanager.Runner"))
            .and_then(|b| {
                b.serve_at(
                    "/runner",
                    Runner {
                        core: app.state::<Core>().inner().clone(),
                        app: app.clone(),
                    },
                )
            })
            .and_then(|b| b.serve_at("/io/filemanager/FileManager", Actions { app: app.clone() }))
            .and_then(|b| b.build());
        match result {
            Ok(connection) => {
                let _connection = connection;
                loop {
                    std::thread::park();
                }
            }
            Err(e) => eprintln!("KRunner session bus: {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_arguments_keep_paths_absolute_and_leave_queries_and_urls_intact() {
        use std::os::unix::ffi::OsStringExt;
        let cwd = Path::new("/tmp/caller");
        for (input, expected) in [
            (
                vec!["filemanager", "--open", "中文 folder"],
                vec!["filemanager", "--open", "/tmp/caller/中文 folder"],
            ),
            (
                vec!["filemanager", "--search", "filemanager docs", "project"],
                vec![
                    "filemanager",
                    "--search",
                    "filemanager docs",
                    "/tmp/caller/project",
                ],
            ),
            (
                vec!["filemanager", "--bookmark", "https://example.com"],
                vec!["filemanager", "--bookmark", "https://example.com"],
            ),
            (
                vec!["filemanager", "file:///tmp/hello"],
                vec!["filemanager", "file:///tmp/hello"],
            ),
            (
                vec!["filemanager", "--reveal", "~/文档"],
                vec!["filemanager", "--reveal", "~/文档"],
            ),
        ] {
            let args: Vec<_> = input.into_iter().map(OsString::from).collect();
            assert_eq!(normalize_arguments(&args, cwd), expected);
        }
        let args = vec![
            OsString::from("filemanager"),
            OsString::from("--open"),
            OsString::from_vec(b"raw-\xff".to_vec()),
        ];
        let converted = normalize_arguments(&args, cwd);
        assert_eq!(converted[2], paths::encode(&cwd.join(&args[2])));
        let stable: Vec<_> = converted.iter().map(OsString::from).collect();
        assert_eq!(normalize_arguments(&stable, cwd), converted);
    }

    #[test]
    fn cli_paths_preserve_calling_directory_and_non_utf8_uri_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let cwd = Path::new("/tmp/current");
        assert_eq!(
            path_argument("docs/中文", cwd).as_deref(),
            Some("/tmp/current/docs/中文")
        );
        assert_eq!(
            path_argument("file:///tmp/%E4%B8%AD%20%27a%27", cwd).as_deref(),
            Some("/tmp/中 'a'")
        );
        let expected = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/raw-\xff"));
        assert_eq!(
            path_argument("file:///tmp/raw-%FF", cwd),
            Some(paths::encode(expected))
        );
        assert!(path_argument("file://other-host/tmp/a", cwd).is_none());
    }
    #[test]
    fn deep_links_decode_paths_and_reject_unknown_routes() {
        assert_eq!(
            parse_url("filemanager://open?path=%2Ftmp%2F%E4%B8%AD%E6%96%87%20%27a%27").unwrap()["path"],
            "/tmp/中文 'a'"
        );
        assert!(parse_url("filemanager://execute?command=rm").is_none());
    }
}
