use crate::{Core, Result, err, str_arg};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
pub struct Job {
    pub cancelled: AtomicBool,
    pub state: Mutex<Value>,
}
pub type Jobs = Arc<Mutex<HashMap<String, Arc<Job>>>>;
thread_local! { static CURRENT:RefCell<Option<Arc<Job>>>=const{RefCell::new(None)}; static LAST:RefCell<Option<Instant>>=const{RefCell::new(None)}; }
pub fn mark_running() {
    CURRENT.with(|j| {
        if let Some(job) = j.borrow().as_ref() {
            if let Ok(mut state) = job.state.lock() {
                state["status"] = json!("running");
            }
        }
    });
}
pub(crate) fn cancelled() -> bool {
    CURRENT.with(|j| {
        j.borrow()
            .as_ref()
            .is_some_and(|j| j.cancelled.load(Ordering::Relaxed))
    })
}
pub(crate) fn partial_result(value: &Value) {
    CURRENT.with(|j| {
        if let Some(job) = j.borrow().as_ref() {
            if let Ok(mut state) = job.state.lock() {
                state["result_bytes"] =
                    json!(serde_json::to_vec(value).map(|v| v.len()).unwrap_or(0));
                state["result"] = value.clone();
            }
        }
    });
}
pub fn checkpoint() -> Result<()> {
    if cancelled() {
        Err("操作已取消；已完成部分可撤销，源文件已保留".into())
    } else {
        Ok(())
    }
}
pub fn progress(current: &str, bytes: u64, completed: u64, total: u64) {
    CURRENT.with(|j| {
        if let Some(j) = j.borrow().as_ref() {
            LAST.with(|l| {
                let mut l = l.borrow_mut();
                if l.is_some_and(|v| v.elapsed().as_millis() < 80) {
                    return;
                }
                *l = Some(Instant::now());
                if let Ok(mut s) = j.state.lock() {
                    s["current"] = json!(current);
                    s["bytes"] = json!(bytes);
                    if total > 0 {
                        s["completed"] = json!(completed);
                        s["total"] = json!(total);
                    }
                }
            })
        }
    })
}
pub fn start(core: &Core, args: &Value) -> Result<Value> {
    let admission = core.job_admission.lock().map_err(err)?;
    if *admission {
        return Err("应用正在重启，无法加入新的后台任务".into());
    }
    let op = str_arg(args, "op")?;
    if !matches!(
        op,
        "operate"
            | "tidy_apply"
            | "clean_scan"
            | "clean_apply"
            | "tidy_plan"
            | "folder_size"
            | "packages"
            | "agent_usage"
            | "agent_projects"
            | "search"
            | "plugin_run"
            | "remote_upload"
            | "remote_download"
            | "remote_transfer"
            | "remote_sync_preview"
    ) {
        return Err("此操作不支持后台队列".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let job = Arc::new(Job {
        cancelled: AtomicBool::new(false),
        state: Mutex::new(
            json!({"id":id,"op":op,"created":chrono::Utc::now().timestamp_millis(),"status":"queued","completed":0,"total":0,"bytes":0,"current":""}),
        ),
    });
    {
        let mut jobs = core.jobs.lock().map_err(err)?;
        let pending = jobs
            .values()
            .filter(|job| {
                job.state
                    .lock()
                    .is_ok_and(|s| matches!(s["status"].as_str(), Some("queued" | "running")))
            })
            .count();
        if pending >= 32 {
            return Err("后台队列已满，请等待部分任务完成".into());
        }
        if jobs.len() > 100 {
            jobs.retain(|_, v| {
                v.state
                    .lock()
                    .is_ok_and(|s| matches!(s["status"].as_str(), Some("queued" | "running")))
            });
        }
        jobs.insert(id.clone(), job.clone());
    }
    drop(admission);
    let core = core.clone();
    let op = op.to_owned();
    let input = json!({"op":op,"args":args["args"]});
    std::thread::spawn(move || {
        CURRENT.with(|c| *c.borrow_mut() = Some(job.clone()));
        // At most three jobs perform work. The queue is also bounded, and a
        // cancelled queued job never reaches the filesystem operation.
        let (slots, available) = &*core.job_slots;
        let mut occupied = slots.lock().unwrap();
        while *occupied >= 3 && !job.cancelled.load(Ordering::Relaxed) {
            occupied = available
                .wait_timeout(occupied, std::time::Duration::from_millis(100))
                .unwrap()
                .0;
        }
        *occupied += 1;
        drop(occupied);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            checkpoint().and_then(|_| core.request(input))
        }));
        *slots.lock().unwrap() -= 1;
        available.notify_one();
        let mut s = job.state.lock().unwrap();
        match result {
            Ok(Ok(v)) => {
                s["status"] = json!("done");
                s["completed"] = s["total"].clone();
                s["result_bytes"] = json!(serde_json::to_vec(&v).map(|v| v.len()).unwrap_or(0));
                s["result"] = v;
            }
            Ok(Err(e)) => {
                s["status"] = json!(if job.cancelled.load(Ordering::Relaxed) {
                    "cancelled"
                } else {
                    "failed"
                });
                s["error"] = json!(e)
            }
            Err(_) => {
                s["status"] = json!("failed");
                s["error"] = json!("任务异常中止，请检查操作历史")
            }
        }
        if op == "remote_transfer" && s["result"]["total"].is_u64() {
            let summary = s["result"].clone();
            s["total"] = summary["total"].clone();
            s["completed"] = json!(
                summary["total"]
                    .as_u64()
                    .unwrap_or(0)
                    .saturating_sub(summary["pending"].as_u64().unwrap_or(0))
            );
            s["bytes"] = summary["bytes"].clone();
            if s["status"] == "done"
                && (summary["failed"].as_u64().unwrap_or(0) > 0
                    || summary["pending"].as_u64().unwrap_or(0) > 0)
            {
                s["status"] = json!("failed");
                s["error"] = json!("部分项目传输失败，请查看逐项结果");
            }
        }
        s["finished_at"] = json!(chrono::Utc::now().timestamp_millis());
        drop(s);
        let _ = prune_results(&core);
    });
    Ok(json!({"id":id}))
}
fn prune_results(core: &Core) -> Result<()> {
    let jobs = core.jobs.lock().map_err(err)?;
    let mut completed: Vec<_> = jobs
        .values()
        .filter_map(|j| {
            j.state
                .lock()
                .ok()
                .and_then(|s| s["finished_at"].as_i64().map(|t| (t, j.clone())))
        })
        .collect();
    completed.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    let mut bytes = 0u64;
    let mut retained = 0;
    for (finished, j) in completed {
        let mut state = j.state.lock().map_err(err)?;
        if state.get("result").is_none() {
            continue;
        }
        let size = state["result_bytes"].as_u64().unwrap_or(0);
        if chrono::Utc::now().timestamp_millis() - finished > 15 * 60 * 1000
            || retained >= 8
            || bytes.saturating_add(size) > 16 * 1024 * 1024
        {
            state.as_object_mut().unwrap().remove("result");
            state["result_expired"] = json!(true);
        } else {
            retained += 1;
            bytes += size;
        }
    }
    Ok(())
}
pub fn list(core: &Core, args: &Value) -> Result<Value> {
    prune_results(core)?;
    let jobs = core.jobs.lock().map_err(err)?;
    let mut states: Vec<_> = jobs
        .values()
        .filter_map(|j| {
            j.state.lock().ok().map(|s| {
                // Status polling must never clone or transport every scan/report.
                let requested = args["result_id"].as_str().is_some_and(|id| s["id"] == id);
                let mut summary: serde_json::Map<String, Value> = s
                    .as_object()
                    .unwrap()
                    .iter()
                    .filter(|(key, _)| key.as_str() != "result" || requested)
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                if !requested && s["result"]["requires_confirmation"] == true {
                    summary.insert("result".into(), json!({"requires_confirmation":true}));
                }
                Value::Object(summary)
            })
        })
        .collect();
    states.sort_by_key(|state| state["created"].as_i64().unwrap_or(0));
    Ok(json!(states))
}
pub fn cancel(core: &Core, args: &Value) -> Result<Value> {
    let jobs = core.jobs.lock().map_err(err)?;
    let j = jobs.get(str_arg(args, "id")?).ok_or("任务不存在")?;
    j.cancelled.store(true, Ordering::Relaxed);
    Ok(json!({"ok":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_admission_rejects_new_jobs_after_the_idle_check() {
        let temp = tempfile::tempdir().unwrap();
        let core = Core::with_paths(
            temp.path().join("state"),
            temp.path().to_owned(),
            temp.path().join("Trash"),
        )
        .unwrap();
        core.prepare_restart().unwrap();
        assert!(
            start(
                &core,
                &json!({"op":"folder_size","args":{"path":temp.path()}})
            )
            .is_err()
        );
    }
    #[test]
    fn restart_is_refused_for_a_queued_job_and_can_be_retried_when_it_finishes() {
        let temp = tempfile::tempdir().unwrap();
        let core = Core::with_paths(
            temp.path().join("state"),
            temp.path().to_owned(),
            temp.path().join("Trash"),
        )
        .unwrap();
        let job = Arc::new(Job {
            cancelled: AtomicBool::new(false),
            state: Mutex::new(json!({"status":"queued"})),
        });
        core.jobs
            .lock()
            .unwrap()
            .insert("pending".into(), job.clone());
        assert!(core.prepare_restart().is_err());
        job.state.lock().unwrap()["status"] = json!("done");
        assert!(core.prepare_restart().is_ok());
    }
    #[test]
    fn status_polling_excludes_heavy_results_and_retention_is_bounded() {
        let t = tempfile::tempdir().unwrap();
        let core = Core::with_paths(
            t.path().join("state"),
            t.path().to_owned(),
            t.path().join("Trash"),
        )
        .unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        for i in 0..12 {
            let id = format!("job-{i}");
            let result = json!({"report":"x".repeat(1024*1024)});
            core.jobs.lock().unwrap().insert(id.clone(),Arc::new(Job { cancelled:AtomicBool::new(false),state:Mutex::new(json!({"id":id,"status":"done","created":i,"finished_at":now+i,"result_bytes":1024*1024+20,"result":result})) }));
        }
        let summary = list(&core, &json!({})).unwrap();
        assert!(serde_json::to_vec(&summary).unwrap().len() < 12000);
        let complete = list(&core, &json!({"result_id":"job-11"})).unwrap();
        assert_eq!(
            complete
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v.get("result").is_some())
                .count(),
            1
        );
        assert_eq!(
            summary
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v["result_expired"] == true)
                .count(),
            4
        );
    }
    #[test]
    fn cancelling_a_queued_operation_never_mutates_files() {
        let temp = tempfile::tempdir().unwrap();
        let core = Core::with_paths(
            temp.path().join("state"),
            temp.path().to_owned(),
            temp.path().join("Trash"),
        )
        .unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        std::fs::write(&source, "keep me").unwrap();
        std::fs::create_dir(&target).unwrap();
        *core.job_slots.0.lock().unwrap() = 3;
        let started = start(
            &core,
            &json!({"op":"operate","args":{"kind":"move","paths":[source],"destination":target}}),
        )
        .unwrap();
        cancel(&core, &started).unwrap();
        // Keep the simulated running jobs in place until cancellation finishes.
        // Resetting their count here races with the cancelled worker releasing
        // its own slot and can underflow the counter.
        core.job_slots.1.notify_all();
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let job = list(&core, &json!({})).unwrap();
            let state = job[0]["status"].as_str().unwrap();
            if state == "cancelled" {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "queued cancellation did not complete: {job}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(*core.job_slots.0.lock().unwrap(), 3);
        assert_eq!(std::fs::read_to_string(source).unwrap(), "keep me");
        assert_eq!(std::fs::read_dir(target).unwrap().count(), 0);
    }
}
