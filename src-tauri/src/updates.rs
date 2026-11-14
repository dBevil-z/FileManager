//! The updater is deliberately independent of the editable GitHub display URL.
use filemanager_core::{Core, Result};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{
    Emitter, Manager,
    utils::{config::BundleType, platform::bundle_type},
};
use tauri_plugin_updater::{Update, UpdaterExt};

const REPOSITORY: &str = "https://github.com/dBevil-z/FileManager";
const MAX_BINARY: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    User,
    Appimage,
    Deb,
    Rpm,
    Manual,
}

#[derive(Default)]
pub struct UpdateState {
    active: AtomicBool,
    installed: AtomicBool,
    pending: Mutex<Option<Update>>,
    progress: Mutex<Value>,
    error: Mutex<Option<String>>,
}

struct Busy<'a>(&'a AtomicBool);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
fn acquire(active: &AtomicBool) -> Result<Busy<'_>> {
    active
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "已有更新操作正在进行".to_string())?;
    Ok(Busy(active))
}

fn user_executable() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))?;
    if !base.is_absolute() {
        return None;
    }
    Some(base.join("filemanager-desktop/bin/filemanager"))
}

fn owned_regular(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && m.uid() == unsafe { libc::geteuid() })
}

fn mode(app: &tauri::AppHandle) -> Mode {
    let Ok(executable) = tauri::utils::platform::current_exe() else {
        return Mode::Manual;
    };
    if user_executable().is_some_and(|expected| expected == executable && owned_regular(&expected))
    {
        return Mode::User;
    }
    match bundle_type() {
        Some(BundleType::AppImage) if app.env().appimage.is_some() => Mode::Appimage,
        Some(BundleType::Deb) if executable == Path::new("/usr/bin/filemanager") => Mode::Deb,
        Some(BundleType::Rpm) if executable == Path::new("/usr/bin/filemanager") => Mode::Rpm,
        _ => Mode::Manual,
    }
}

fn target(mode: Mode) -> String {
    format!(
        "linux-{}-{}",
        std::env::consts::ARCH,
        match mode {
            Mode::User => "user",
            Mode::Deb => "deb",
            Mode::Rpm => "rpm",
            _ => "appimage",
        }
    )
}

fn metadata(update: &Update) -> Value {
    json!({"version":update.version, "notes":update.body.as_deref().unwrap_or("").chars().take(16_000).collect::<String>(), "date":update.date.map(|d| d.to_string())})
}

fn status(app: &tauri::AppHandle) -> Result<Value> {
    let state = app.state::<UpdateState>();
    Ok(
        json!({"mode":mode(app), "busy":state.active.load(Ordering::Acquire),
        "installed":state.installed.load(Ordering::Acquire),
        "update":state.pending.lock().map_err(|e|e.to_string())?.as_ref().map(metadata),
        "progress":*state.progress.lock().map_err(|e|e.to_string())?,
        "error":*state.error.lock().map_err(|e|e.to_string())?}),
    )
}

fn progress(app: &tauri::AppHandle, value: Value) {
    if let Ok(mut current) = app.state::<UpdateState>().progress.lock() {
        *current = value.clone();
    }
    let _ = app.emit_to("main", "filemanager-update-progress", value);
}

fn ensure_idle(core: &Core) -> Result<()> {
    let jobs = core.request(json!({"op":"jobs"}))?;
    if jobs.as_array().is_some_and(|jobs| {
        jobs.iter()
            .any(|j| matches!(j["status"].as_str(), Some("queued" | "running")))
    }) {
        return Err("请先等待后台任务完成，再安装更新或重启".into());
    }
    Ok(())
}

fn validate_artifact(url: &url::Url, version: &str, mode: Mode) -> Result<()> {
    let extension = match mode {
        Mode::User => ".bin",
        Mode::Deb => ".deb",
        Mode::Rpm => ".rpm",
        _ => ".AppImage",
    };
    let prefix = format!("{REPOSITORY}/releases/download/v{version}/FileManager_{version}_");
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.as_str().starts_with(&prefix)
        || !url.path().ends_with(extension)
        || url
            .path()
            .trim_start_matches("/dBevil-z/FileManager/releases/download/")
            .split('/')
            .count()
            != 2
    {
        return Err("更新包地址不属于官方发行版，已拒绝安装".into());
    }
    Ok(())
}

pub async fn action(
    app: tauri::AppHandle,
    core: Core,
    window: &str,
    action: &str,
) -> Result<Value> {
    if window != "main" {
        return Err("更新仅可从主窗口操作".into());
    }
    let state = app.state::<UpdateState>();
    match action {
        "update_status" => status(&app),
        "update_check" => {
            if state.installed.load(Ordering::Acquire) {
                return status(&app);
            }
            let busy = acquire(&state.active)?;
            *state.error.lock().map_err(|e| e.to_string())? = None;
            let selected = mode(&app);
            let result = async {
                let update = app
                    .updater_builder()
                    .target(target(selected))
                    .timeout(Duration::from_secs(30))
                    .version_comparator(|current, release| {
                        release.version > current && release.version.pre.is_empty()
                    })
                    .build()
                    .map_err(|e| e.to_string())?
                    .check()
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(ref update) = update {
                    validate_artifact(&update.download_url, &update.version, selected)?;
                }
                *state.pending.lock().map_err(|e| e.to_string())? = update;
                Ok::<_, String>(())
            }
            .await;
            drop(busy);
            if let Err(error) = result {
                *state.pending.lock().map_err(|e| e.to_string())? = None;
                *state.error.lock().map_err(|e| e.to_string())? = Some(error.clone());
                return Err(error);
            }
            status(&app)
        }
        "update_install" => {
            if state.installed.load(Ordering::Acquire) {
                return Err("更新已安装，请重启后再检查".into());
            }
            let _busy = acquire(&state.active)?;
            ensure_idle(&core)?;
            let selected = mode(&app);
            if selected == Mode::Manual {
                return Err("当前安装方式请通过 GitHub Releases 手动更新".into());
            }
            let mut update = state
                .pending
                .lock()
                .map_err(|e| e.to_string())?
                .clone()
                .ok_or("请先检查更新")?;
            validate_artifact(&update.download_url, &update.version, selected)?;
            update.timeout = Some(Duration::from_secs(15 * 60));
            *state.error.lock().map_err(|e| e.to_string())? = None;
            let result = async {
                let mut downloaded = 0u64;
                let mut last = Instant::now() - Duration::from_secs(1);
                progress(&app, json!({"phase":"downloading","downloaded":0}));
                let bytes = update.download(|chunk, total| {
                    downloaded += chunk as u64;
                    if last.elapsed() >= Duration::from_millis(100) {
                        progress(&app, json!({"phase":"downloading","downloaded":downloaded,"total":total}));
                        last = Instant::now();
                    }
                }, || progress(&app, json!({"phase":"verifying"}))).await.map_err(|e|e.to_string())?;
                ensure_idle(&core)?;
                progress(&app, json!({"phase":"installing"}));
                tauri::async_runtime::spawn_blocking(move || match selected {
                    Mode::User => install_user_binary(&user_executable().ok_or("无法确定用户安装目录")?, &bytes),
                    Mode::Deb | Mode::Rpm => install_package(selected, &bytes),
                    _ => update.install(bytes).map_err(|e|e.to_string()),
                }).await.map_err(|e|e.to_string())??;
                Ok::<_, String>(())
            }.await;
            if let Err(error) = result {
                *state.error.lock().map_err(|e| e.to_string())? = Some(error.clone());
                *state.progress.lock().map_err(|e| e.to_string())? = Value::Null;
                return Err(error);
            }
            state.installed.store(true, Ordering::Release);
            progress(&app, json!({"phase":"installed"}));
            Ok(json!({"ok":true}))
        }
        "update_restart" => {
            if !state.installed.load(Ordering::Acquire) {
                return Err("没有等待重启的更新".into());
            }
            core.prepare_restart()?;
            // Job admission remains sealed until exit, including this reply delay.
            // Session persistence has already completed in the calling window.
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio_sleep().await;
                handle.restart();
            });
            Ok(json!({"ok":true}))
        }
        _ => Err("未知更新操作".into()),
    }
}

async fn tokio_sleep() {
    tauri::async_runtime::spawn_blocking(|| std::thread::sleep(Duration::from_millis(200)))
        .await
        .ok();
}

fn install_package(selected: Mode, bytes: &[u8]) -> Result<()> {
    let (tool, arg, suffix) = if selected == Mode::Deb {
        ("/usr/bin/dpkg", "-i", ".deb")
    } else {
        ("/usr/bin/rpm", "-U", ".rpm")
    };
    let mut file = tempfile::Builder::new()
        .prefix("filemanager-update-")
        .suffix(suffix)
        .tempfile()
        .map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    // pkexec owns authentication. Never request a password or fall back to a hidden sudo prompt.
    let result = std::process::Command::new("/usr/bin/pkexec")
        .arg(tool)
        .arg(arg)
        .arg(file.path())
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("无法调用系统授权，请从 GitHub Releases 手动安装：{e}"))?;
    if !result.status.success() {
        return Err("系统未完成更新安装；可重试或从 GitHub Releases 手动安装".into());
    }
    Ok(())
}

fn validate_binary(bytes: &[u8]) -> Result<()> {
    let machine: u16 = if cfg!(target_arch = "x86_64") {
        62
    } else if cfg!(target_arch = "aarch64") {
        183
    } else {
        return Err("尚未提供此架构的更新包".into());
    };
    if bytes.len() < 64
        || bytes.len() > MAX_BINARY
        || &bytes[..4] != b"\x7fELF"
        || bytes[4] != 2
        || bytes[5] != 1
        || u16::from_le_bytes([bytes[18], bytes[19]]) != machine
    {
        return Err("更新包不是此架构的有效 Linux 程序".into());
    }
    Ok(())
}

fn install_user_binary(path: &Path, bytes: &[u8]) -> Result<()> {
    validate_binary(bytes)?;
    if !owned_regular(path) {
        return Err("用户安装文件不是本人拥有的普通文件".into());
    }
    let parent = path.parent().ok_or("用户安装目录无效")?;
    let directory = fs::metadata(parent).map_err(|e| e.to_string())?;
    if !directory.is_dir() || directory.uid() != unsafe { libc::geteuid() } {
        return Err("用户安装目录不属于当前用户".into());
    }
    let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    staged.write_all(bytes).map_err(|e| e.to_string())?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    staged.as_file().sync_all().map_err(|e| e.to_string())?;
    let mut backup = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut original = File::open(path).map_err(|e| e.to_string())?;
    std::io::copy(&mut original, backup.as_file_mut()).map_err(|e| e.to_string())?;
    backup
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    backup.as_file().sync_all().map_err(|e| e.to_string())?;
    backup
        .persist(path.with_extension("previous"))
        .map_err(|e| e.to_string())?;
    staged.persist(path).map_err(|e| e.to_string())?;
    let _ = File::open(parent).and_then(|directory| directory.sync_all());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_update_operation_can_run_and_errors_release_the_gate() {
        let active = AtomicBool::new(false);
        let guard = acquire(&active).unwrap();
        assert!(acquire(&active).is_err());
        drop(guard);
        assert!(acquire(&active).is_ok());
    }
    #[test]
    fn rejects_foreign_artifacts_and_mismatched_installer_types() {
        let valid = "https://github.com/dBevil-z/FileManager/releases/download/v0.2.4/FileManager_0.2.4_linux_x86_64.bin";
        assert!(validate_artifact(&valid.parse().unwrap(), "0.2.4", Mode::User).is_ok());
        for bad in [
            valid.replace("https:", "http:"),
            valid.replace("dBevil-z", "other"),
            valid.replace("v0.2.4", "v0.2.3"),
            format!("{valid}?token=hidden"),
            valid.replace(".bin", ".deb"),
            valid.replace("FileManager_0.2.4_linux", "nested/FileManager_0.2.4_linux"),
        ] {
            assert!(
                validate_artifact(&bad.parse().unwrap(), "0.2.4", Mode::User).is_err(),
                "{bad}"
            );
        }
    }
    fn elf() -> Vec<u8> {
        let mut bytes = vec![0u8; 64];
        bytes[..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[18..20].copy_from_slice(
            &(if cfg!(target_arch = "x86_64") {
                62u16
            } else {
                183u16
            })
            .to_le_bytes(),
        );
        bytes
    }
    #[test]
    fn atomic_user_update_retains_the_original_and_executable_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("filemanager");
        fs::write(&path, b"previous-version").unwrap();
        install_user_binary(&path, &elf()).unwrap();
        assert_eq!(fs::read(&path).unwrap(), elf());
        assert_eq!(
            fs::read(path.with_extension("previous")).unwrap(),
            b"previous-version"
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
    #[test]
    fn invalid_or_symlink_install_preserves_the_existing_executable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("filemanager");
        fs::write(&path, b"previous-version").unwrap();
        assert!(install_user_binary(&path, b"not-an-executable").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"previous-version");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(install_user_binary(&link, &elf()).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"previous-version");
        assert!(!path.with_extension("previous").exists());
    }

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/signed-update.json")).unwrap()
    }

    fn test_app(public_key: &str) -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.package_info_mut().version = "0.2.3".parse().unwrap();
        context.config_mut().plugins.0.insert("updater".into(), json!({
            "pubkey":public_key, "requireSignedVersion":true,
            "endpoints":["https://github.com/dBevil-z/FileManager/releases/latest/download/latest.json"]
        }));
        tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap()
    }

    fn fixture_update(payload: Vec<u8>, version: &str) -> (Update, std::thread::JoinHandle<()>) {
        let data = fixture();
        let app = test_app(data["publicKey"].as_str().unwrap());
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/latest.json", server.server_addr());
        let manifest = json!({"version":version,"platforms":{"linux-x86_64-user":{
            "url":format!("http://{}/installer.bin",server.server_addr()),"signature":data["signature"]
        }}}).to_string();
        let worker = std::thread::spawn(move || {
            for _ in 0..2 {
                if let Ok(Some(request)) = server.recv_timeout(Duration::from_secs(5)) {
                    let bytes = if request.url() == "/latest.json" {
                        manifest.as_bytes().to_vec()
                    } else {
                        payload.clone()
                    };
                    request
                        .respond(tiny_http::Response::from_data(bytes))
                        .unwrap();
                }
            }
        });
        let updater = app
            .updater_builder()
            .target("linux-x86_64-user")
            .endpoints(vec![endpoint.parse().unwrap()])
            .unwrap()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let update = tauri::async_runtime::block_on(updater.check())
            .unwrap()
            .unwrap();
        (update, worker)
    }

    #[test]
    fn a_real_signed_download_is_required_before_user_binary_replacement() {
        let payload: Vec<u8> = serde_json::from_value(fixture()["payload"].clone()).unwrap();
        let (update, worker) = fixture_update(payload.clone(), "0.2.4");
        let bytes = tauri::async_runtime::block_on(update.download(|_, _| {}, || {})).unwrap();
        assert_eq!(bytes, payload);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("filemanager");
        fs::write(&path, b"0.2.3").unwrap();
        install_user_binary(&path, &bytes).unwrap();
        assert_eq!(fs::read(path.with_extension("previous")).unwrap(), b"0.2.3");
        worker.join().unwrap();
    }

    #[test]
    fn tampered_download_or_manifest_version_cannot_replace_the_installed_program() {
        let original: Vec<u8> = serde_json::from_value(fixture()["payload"].clone()).unwrap();
        let mut tampered = original.clone();
        tampered[63] = 1;
        for (payload, version) in [(tampered, "0.2.4"), (original, "0.2.5")] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("filemanager");
            fs::write(&path, b"0.2.3").unwrap();
            let (update, worker) = fixture_update(payload, version);
            let download = tauri::async_runtime::block_on(update.download(|_, _| {}, || {}));
            assert!(download.is_err());
            if let Ok(bytes) = download {
                install_user_binary(&path, &bytes).unwrap();
            }
            assert_eq!(fs::read(&path).unwrap(), b"0.2.3");
            assert!(!path.with_extension("previous").exists());
            worker.join().unwrap();
        }
    }

    #[test]
    #[ignore = "requires the staged release assets; run before publishing"]
    fn prepared_release_signatures_match_the_embedded_public_key() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let app = test_app(config["plugins"]["updater"]["pubkey"].as_str().unwrap());
        let directory = std::env::var_os("FILEMANAGER_RELEASE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                    "../target/release/bundle/publish/v{}",
                    env!("CARGO_PKG_VERSION")
                ))
            });
        let manifest: Value =
            serde_json::from_slice(&fs::read(directory.join("latest.json")).unwrap()).unwrap();
        for selected in [Mode::User, Mode::Appimage, Mode::Deb] {
            let update = app
                .updater_builder()
                .target(target(selected))
                .build()
                .unwrap()
                .restore_update(manifest.clone())
                .unwrap()
                .unwrap();
            validate_artifact(&update.download_url, &update.version, selected).unwrap();
            let name = update
                .download_url
                .path_segments()
                .unwrap()
                .next_back()
                .unwrap();
            let bytes = fs::read(directory.join(name)).unwrap();
            update.verify(&bytes).unwrap();
        }
        println!(
            "All staged installer signatures match the embedded public key and announced version."
        );
    }

    #[test]
    #[ignore = "downloads the current official GitHub release; run explicitly after publication"]
    fn published_release_can_upgrade_an_isolated_user_install() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let app = test_app(config["plugins"]["updater"]["pubkey"].as_str().unwrap());
        let update = tauri::async_runtime::block_on(
            app.updater_builder()
                .target("linux-x86_64-user")
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap()
                .check(),
        )
        .unwrap()
        .expect("0.2.3 baseline should find the published release");
        assert_eq!(update.version, env!("CARGO_PKG_VERSION"));
        validate_artifact(&update.download_url, &update.version, Mode::User).unwrap();
        let bytes = tauri::async_runtime::block_on(update.download(|_, _| {}, || {})).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("filemanager");
        let baseline = std::env::var_os("FILEMANAGER_UPDATE_BASELINE")
            .expect("set FILEMANAGER_UPDATE_BASELINE to the local previous-version binary");
        fs::copy(&baseline, &path).unwrap();
        install_user_binary(&path, &bytes).unwrap();
        assert_eq!(
            fs::read(path.with_extension("previous")).unwrap(),
            fs::read(baseline).unwrap()
        );
        let result = std::process::Command::new(&path)
            .arg("--version")
            .output()
            .unwrap();
        assert!(result.status.success());
        assert_eq!(
            String::from_utf8(result.stdout).unwrap().trim(),
            format!("FileManager {}", env!("CARGO_PKG_VERSION"))
        );
        println!(
            "Official GitHub feed, HTTPS download, version-bound signature, atomic replacement, rollback copy and new binary version verified."
        );
    }
}
