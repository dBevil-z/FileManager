pub mod agents;
pub mod catalog;
pub mod clean;
mod file_copy;
pub mod files;
pub mod jobs;
pub mod mcp;
pub mod media;
pub mod operations;
pub mod organize;
pub mod packages;
pub mod paths;
pub mod plugins;
pub mod preview;
pub mod process;
pub mod store;
pub mod system;
pub mod trash;
pub mod usage;

use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
};

pub type Result<T> = std::result::Result<T, String>;
pub fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
pub fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("缺少参数：{key}"))
}

#[derive(Clone)]
pub struct Core {
    pub data_dir: PathBuf,
    pub home_dir: PathBuf,
    pub trash_dir: PathBuf,
    pub(crate) mutation: Arc<Mutex<()>>,
    pub(crate) jobs: jobs::Jobs,
    pub(crate) job_slots: Arc<(Mutex<usize>, Condvar)>,
    pub(crate) media: Arc<Mutex<std::collections::HashMap<String, PathBuf>>>,
    pub(crate) preview_gate: Arc<Mutex<()>>,
}

impl Core {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        let home = dirs::home_dir().ok_or("找不到用户主目录")?;
        let trash = dirs::data_local_dir()
            .unwrap_or_else(|| home.join(".local/share"))
            .join("Trash");
        Self::with_paths(data_dir, home, trash)
    }

    /// Explicit paths also support portable installations without changing process-wide environment.
    pub fn with_paths(data_dir: PathBuf, home_dir: PathBuf, trash_dir: PathBuf) -> Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(&data_dir).map_err(err)?;
        fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o700)).map_err(err)?;
        let core = Self {
            data_dir,
            home_dir,
            trash_dir,
            mutation: Arc::new(Mutex::new(())),
            jobs: Arc::new(Mutex::new(std::collections::HashMap::new())),
            job_slots: Arc::new((Mutex::new(0), Condvar::new())),
            media: Arc::new(Mutex::new(std::collections::HashMap::new())),
            preview_gate: Arc::new(Mutex::new(())),
        };
        core.connection()?;
        Ok(core)
    }

    pub fn request(&self, input: Value) -> Result<Value> {
        let op = str_arg(&input, "op")?;
        let args = input.get("args").cloned().unwrap_or_else(|| json!({}));
        let mutating = matches!(
            op,
            "operate"
                | "undo"
                | "defer_undo"
                | "create_folder"
                | "rename_apply"
                | "write_text"
                | "restore_trash"
                | "empty_trash"
                | "catalog_save"
                | "tidy_apply"
                | "mcp_save"
                | "agent_copy"
                | "clean_apply"
                | "plugin_install"
                | "plugin_manage"
                | "create_text"
        );
        let _guard = if mutating {
            Some(self.mutation.lock().map_err(err)?)
        } else {
            None
        };
        jobs::checkpoint()?;
        jobs::mark_running();
        match op {
            "operate_check" => {
                let mut a = args.clone();
                a["dry_run"] = json!(true);
                operations::operate(self, &a)
            }
            "plugins_list" => plugins::list(self),
            "plugin_inspect" => plugins::inspect(self, &args),
            "plugin_install" => plugins::install(self, &args),
            "plugin_manage" => plugins::manage(self, &args),
            "plugin_run" => plugins::run(self, &args),
            "bookmark_metadata" => catalog::metadata(&args),
            "packages" => packages::list(self, &args),
            "agent_projects" => agents::projects(self, &args),
            "agent_copy" => agents::copy(self, &args),
            "agent_usage" => usage::scan(self, &args),
            "mcp_list" => mcp::list(self, &args),
            "mcp_save" => mcp::save(self, &args),
            "local_servers" => agents::servers(self),
            "stop_server" => agents::stop(&args),
            "job_start" => jobs::start(self, &args),
            "jobs" => jobs::list(self, &args),
            "job_cancel" => jobs::cancel(self, &args),
            "tidy_plan" => organize::plan(self, &args),
            "tidy_apply" => organize::apply(self, &args),
            "insights" => organize::insights(self, &args),
            "clean_scan" => clean::scan(self, &args),
            "clean_apply" => clean::apply(self, &args),
            "preview" => preview::get(self, &args),
            "markdown_image" => preview::markdown_image(self, &args),
            "resource_usage" => system::resources(),
            "system_info" => system::info(self, &args),
            "mounts" => system::mounts(),
            "git_status" => process::git(self, &args),
            "desktop_action" => process::desktop(self, &args),
            "clear_cache" => preview::clear_cache(self),
            "directory_stamp" => {
                use std::os::unix::fs::MetadataExt;
                let p = paths::decode(self, str_arg(&args, "path")?)?;
                let m = fs::metadata(p).map_err(err)?;
                Ok(json!({"stamp":format!("{}:{}:{}:{}",m.dev(),m.ino(),m.mtime(),m.mtime_nsec())}))
            }
            "list_directory" => files::list(self, &args),
            "file_info" => files::info(self, &args),
            "folder_info" => files::folder_info(self, &args),
            "folder_size" => files::folder_size(self, &args),
            "path_complete" => files::complete(self, &args),
            "search" => files::search(self, &args),
            "read_text" => files::read_text(self, &args),
            "write_text" => operations::write_text(self, &args),
            "create_text" => {
                let parent = paths::decode(self, str_arg(&args, "directory")?)?;
                let name = str_arg(&args, "name")?;
                paths::check_name(name)?;
                let path = parent.join(name);
                if paths::existing(&path) {
                    return Err("同名文件已存在，请选择其他名称".into());
                }
                operations::write_text(
                    self,
                    &json!({"path":paths::encode(&path),"text":str_arg(&args,"text")?}),
                )
            }
            "operate" => operations::operate(self, &args),
            "create_folder" => operations::create_folder(self, &args),
            "rename_plan" => operations::rename_plan(self, &args),
            "rename_apply" => operations::rename_apply(self, &args),
            "undo" => operations::undo(self, &args),
            "defer_undo" => operations::defer_undo(self, &args),
            "history" => self.history(),
            "list_trash" => trash::list(self),
            "restore_trash" => trash::restore_request(self, &args),
            "empty_trash" => trash::empty(self, &args),
            "catalog_get" => self.catalog_get(&args),
            "catalog_save" => self.catalog_save(&args),
            "bootstrap" => Ok(
                json!({"home":paths::encode(&self.home_dir),"data_dir":paths::encode(&self.data_dir),"version":env!("CARGO_PKG_VERSION"),"platform":"linux","places":files::places(self),"catalog":self.get_value("catalog")?,"settings":self.get_value("settings")?,"session":self.get_value("session")?}),
            ),
            _ => Err(format!("未知操作：{op}")),
        }
    }
}
