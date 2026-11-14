use super::{
    MAX_ENTRIES,
    batch::{Snapshot, scan_local, scan_remote},
    block, directory, profile,
};
use crate::{Core, Result, err, paths, str_arg};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::{Duration, Instant},
};

const TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Serialize)]
struct Difference {
    path: String,
    status: &'static str,
    local: Option<Snapshot>,
    remote: Option<Snapshot>,
}

struct Plan {
    updated: Instant,
    id: String,
    revision: u64,
    bucket: Option<String>,
    local_dir: String,
    path: String,
    direction: String,
    entries: Vec<Difference>,
    counts: BTreeMap<&'static str, usize>,
}
impl Plan {
    fn summary(&self, token: &str) -> Value {
        json!({"plan_id": token, "total": self.entries.len(), "counts": self.counts, "local_dir": self.local_dir, "path": self.path, "direction": self.direction})
    }
}

#[derive(Default)]
pub(super) struct Plans(HashMap<String, Plan>);
impl Plans {
    fn prune(&mut self) {
        self.0.retain(|_, p| p.updated.elapsed() < TTL);
    }
    fn insert(&mut self, key: String, plan: Plan) {
        self.prune();
        while self.0.len() >= 4
            || self.0.values().map(|p| p.entries.len()).sum::<usize>() + plan.entries.len()
                > 200_000
        {
            let Some(key) = self
                .0
                .iter()
                .min_by_key(|(_, p)| p.updated)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            self.0.remove(&key);
        }
        self.0.insert(key, plan);
    }
}

fn differences(
    local: BTreeMap<String, Snapshot>,
    remote: BTreeMap<String, Snapshot>,
) -> Vec<Difference> {
    let keys: BTreeSet<_> = local.keys().chain(remote.keys()).cloned().collect();
    let blockers: BTreeSet<_> = keys
        .iter()
        .filter(|path| {
            let l = local.get(*path);
            let r = remote.get(*path);
            l.is_some_and(|s| s.kind == "unsupported")
                || l.zip(r).is_some_and(|(l, r)| l.kind != r.kind)
        })
        .cloned()
        .collect();
    keys.into_iter()
        .map(|path| {
            let l = local.get(&path);
            let r = remote.get(&path);
            let parent_conflict = path
                .match_indices('/')
                .any(|(index, _)| blockers.contains(&path[..index]));
            let status = match (l, r) {
                _ if parent_conflict => "parent_conflict",
                (Some(l), _) if l.kind == "unsupported" => "unsupported",
                (Some(_), None) => "local_only",
                (None, Some(_)) => "remote_only",
                (Some(l), Some(r)) if l.kind != r.kind => "type_conflict",
                (Some(l), Some(_)) if l.kind == "directory" => "directory",
                (Some(l), Some(r)) if l.size != r.size => "size_differs",
                (Some(l), Some(r))
                    if l.modified
                        .zip(r.modified)
                        .is_some_and(|(a, b)| a.abs_diff(b) > 2000) =>
                {
                    "metadata_differs"
                }
                _ => "unverified",
            };
            Difference {
                path,
                status,
                local: l.cloned(),
                remote: r.cloned(),
            }
        })
        .collect()
}

pub(crate) fn preview(core: &Core, args: &Value) -> Result<Value> {
    let path = directory(args)?;
    let id = str_arg(args, "id")?;
    let revision = args["revision"].as_u64().ok_or("同步预览需要连接版本号")?;
    let direction = str_arg(args, "direction")?;
    if !matches!(direction, "upload" | "download") {
        return Err("传输方向无效".into());
    }
    let (config, secret) = profile::scoped(core, args, false)?;
    if config.lists_buckets() {
        return Err("请先选择 Bucket，再操作桶内文件".into());
    }
    let local_dir = paths::decode(core, str_arg(args, "local_dir")?)?
        .canonicalize()
        .map_err(err)?;
    let local = scan_local(&local_dir)?;
    let remote = block(scan_remote(core, &config, &secret, path))?;
    // Never classify missing entries from a partial/truncated scan. A connection
    // edit during scanning also invalidates this comparison.
    profile::load(core, id, Some(revision), false)?;
    let entries = differences(local, remote);
    if entries.len() > MAX_ENTRIES {
        return Err("比较超过 100,000 项，请选择较小的范围".into());
    }
    let mut counts = BTreeMap::new();
    for e in &entries {
        *counts.entry(e.status).or_insert(0) += 1;
    }
    let plan = Plan {
        updated: Instant::now(),
        id: id.into(),
        revision,
        bucket: config.bucket().map(str::to_owned),
        local_dir: paths::encode(&local_dir),
        path: path.into(),
        direction: direction.into(),
        entries,
        counts,
    };
    let token = uuid::Uuid::new_v4().to_string();
    let summary = plan.summary(&token);
    core.remote.plans.lock().map_err(err)?.insert(token, plan);
    Ok(summary)
}

pub(crate) fn page(core: &Core, args: &Value) -> Result<Value> {
    let id = str_arg(args, "id")?;
    let revision = args["revision"].as_u64().ok_or("同步预览需要连接版本号")?;
    let (config, _) = profile::scoped(core, args, false)?;
    let mut plans = core.remote.plans.lock().map_err(err)?;
    plans.prune();
    let token = str_arg(args, "plan_id")?;
    let plan = plans.0.get(token).ok_or("同步预览已过期，请重新扫描")?;
    if plan.id != id || plan.revision != revision || plan.bucket.as_deref() != config.bucket() {
        return Err("同步预览不属于此连接版本，请重新扫描".into());
    }
    let status = args["status"].as_str().unwrap_or("");
    let query = args["query"].as_str().unwrap_or("").to_lowercase();
    if query.len() > 4096 {
        return Err("筛选文字过长".into());
    }
    let offset = args["offset"].as_u64().unwrap_or(0).min(MAX_ENTRIES as u64) as usize;
    let matched: Vec<_> = plan
        .entries
        .iter()
        .filter(|e| {
            (status.is_empty() || e.status == status) && e.path.to_lowercase().contains(&query)
        })
        .collect();
    Ok(
        json!({"summary": plan.summary(token), "count": matched.len(), "entries": matched.into_iter().skip(offset).take(100).collect::<Vec<_>>()}),
    )
}

pub(crate) fn close(core: &Core, args: &Value) -> Result<Value> {
    let mut plans = core.remote.plans.lock().map_err(err)?;
    let token = str_arg(args, "plan_id")?;
    let id = str_arg(args, "id")?;
    if plans.0.get(token).is_some_and(|p| p.id == id) {
        plans.0.remove(token);
    }
    Ok(json!({"ok": true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(kind: &'static str, size: u64, modified: i64) -> Snapshot {
        // Use a real local snapshot through the scanner for identity-free tests.
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("file"), b"x").unwrap();
        let mut result = scan_local(t.path()).unwrap().remove("file").unwrap();
        result.kind = kind;
        result.size = Some(size);
        result.modified = Some(modified);
        result
    }
    #[test]
    fn comparison_never_calls_equal_sizes_verified_content() {
        let mut local = BTreeMap::new();
        let mut remote = BTreeMap::new();
        for (name, l, r) in [
            ("same", snapshot("file", 1, 1000), snapshot("file", 1, 1000)),
            (
                "changed",
                snapshot("file", 1, 1000),
                snapshot("file", 2, 1000),
            ),
            ("time", snapshot("file", 1, 1000), snapshot("file", 1, 5000)),
            (
                "conflict",
                snapshot("file", 0, 1000),
                snapshot("directory", 0, 1000),
            ),
        ] {
            local.insert(name.into(), l);
            remote.insert(name.into(), r);
        }
        local.insert("only-here".into(), snapshot("file", 1, 0));
        local.insert("conflict/child".into(), snapshot("file", 1, 0));
        remote.insert("only-there".into(), snapshot("file", 1, 0));
        let statuses: BTreeMap<_, _> = differences(local, remote)
            .into_iter()
            .map(|e| (e.path, e.status))
            .collect();
        assert_eq!(statuses["same"], "unverified");
        assert_eq!(statuses["changed"], "size_differs");
        assert_eq!(statuses["time"], "metadata_differs");
        assert_eq!(statuses["conflict"], "type_conflict");
        assert_eq!(statuses["conflict/child"], "parent_conflict");
        assert_eq!(statuses["only-here"], "local_only");
        assert_eq!(statuses["only-there"], "remote_only");
    }
}
