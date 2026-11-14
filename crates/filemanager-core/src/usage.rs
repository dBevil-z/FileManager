use crate::{Core, Result, err, jobs, paths};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::{BufRead, BufReader},
};
use walkdir::WalkDir;
#[derive(Default, Clone, Serialize, Deserialize)]
struct Tokens {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
}
impl Tokens {
    fn add(&mut self, v: &Self) {
        self.input += v.input;
        self.output += v.output;
        self.cache_read += v.cache_read;
        self.cache_write += v.cache_write;
    }
    fn delta(&self, last: &Self) -> Self {
        Self {
            input: self.input.saturating_sub(last.input),
            output: self.output.saturating_sub(last.output),
            cache_read: self.cache_read.saturating_sub(last.cache_read),
            cache_write: self.cache_write.saturating_sub(last.cache_write),
        }
    }
    fn max(&mut self, v: &Self) {
        self.input = self.input.max(v.input);
        self.output = self.output.max(v.output);
        self.cache_read = self.cache_read.max(v.cache_read);
        self.cache_write = self.cache_write.max(v.cache_write);
    }
}
fn n(v: &Value, keys: &[&str]) -> u64 {
    keys.iter().find_map(|k| v[*k].as_u64()).unwrap_or(0)
}
fn tokens(v: &Value) -> Tokens {
    Tokens {
        input: n(
            v,
            &["input_tokens", "input", "prompt_tokens", "promptTokenCount"],
        ),
        output: n(
            v,
            &[
                "output_tokens",
                "output",
                "completion_tokens",
                "candidatesTokenCount",
            ],
        ),
        cache_read: n(
            v,
            &[
                "cached_input_tokens",
                "cache_read_input_tokens",
                "cached",
                "cachedContentTokenCount",
            ],
        ) + v["cache"]["read"].as_u64().unwrap_or(0),
        cache_write: n(v, &["cache_creation_input_tokens", "cache_write"])
            + v["cache"]["write"].as_u64().unwrap_or(0),
    }
}
#[derive(Clone)]
struct Event {
    provider: String,
    model: String,
    project: String,
    date: String,
    session: String,
    t: Tokens,
}
fn timestamp(v: &Value) -> String {
    v["timestamp"]
        .as_str()
        .or(v["time"].as_str())
        .map(|v| v.chars().take(10).collect())
        .unwrap_or_else(|| "未知日期".into())
}
pub fn scan(core: &Core, args: &Value) -> Result<Value> {
    let roots = args["roots"]
        .as_array()
        .map(|v| {
            v.iter()
                .filter_map(Value::as_str)
                .map(|p| paths::decode(core, p))
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_else(|| {
            vec![
                core.home_dir.join(".codex/sessions"),
                core.home_dir.join(".codex/archived_sessions"),
                core.home_dir.join(".claude/projects"),
                core.home_dir.join(".gemini/tmp"),
                core.home_dir.join(".copilot/session-state"),
                core.home_dir.join(".local/share/opencode/storage/message"),
            ]
        });
    let mut events = Vec::new();
    let mut cumulative = HashMap::<String, Tokens>::new();
    let mut codex_events = Vec::<(String, Event)>::new();
    let mut messages = HashMap::<String, Event>::new();
    let mut malformed = 0;
    let mut files = 0;
    let mut bytes = 0u64;
    let mut truncated = false;
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    for root in roots {
        if !root.exists() {
            continue;
        }
        for f in WalkDir::new(root)
            .follow_links(false)
            .max_depth(8)
            .max_open(12)
            .into_iter()
            .flatten()
        {
            jobs::checkpoint()?;
            let p = f.path();
            if !f.file_type().is_file()
                || !matches!(
                    p.extension().and_then(|s| s.to_str()),
                    Some("json" | "jsonl")
                )
            {
                continue;
            }
            if !seen.insert(p.to_owned()) {
                continue;
            }
            let m = fs::metadata(p).map_err(err)?;
            if m.len() > 64 * 1024 * 1024 || bytes + m.len() > 512 * 1024 * 1024 || files >= 6000 {
                truncated = true;
                continue;
            }
            bytes += m.len();
            files += 1;
            jobs::progress(&p.to_string_lossy(), bytes, files, 0);
            let mut session = paths::encode(p);
            let mut project = "未知项目".to_string();
            let mut model = "未知模型".to_string();
            let reader = match fs::File::open(p) {
                Ok(v) => BufReader::new(v),
                Err(e) => {
                    errors.push(e.to_string());
                    continue;
                }
            };
            if p.extension().is_some_and(|s| s == "json") {
                let value: Value = match serde_json::from_reader(reader) {
                    Ok(v) => v,
                    Err(_) => {
                        malformed += 1;
                        continue;
                    }
                };
                let provider = if p.to_string_lossy().contains("gemini") {
                    "Gemini"
                } else if p.to_string_lossy().contains("copilot") {
                    "Copilot"
                } else {
                    "OpenCode"
                };
                let rows = value["messages"]
                    .as_array()
                    .cloned()
                    .unwrap_or_else(|| vec![value.clone()]);
                for v in rows {
                    let u = v
                        .get("tokens")
                        .or(v.get("usage"))
                        .or(v.get("usageMetadata"));
                    if let Some(u) = u {
                        let t = tokens(u);
                        let id = v["id"].as_str().unwrap_or(p.to_str().unwrap_or("unknown"));
                        let model = v["model"]
                            .as_str()
                            .or(v["modelID"].as_str())
                            .unwrap_or("未知模型");
                        let ev = Event {
                            provider: provider.into(),
                            model: model.into(),
                            project: value["projectHash"].as_str().unwrap_or("未知项目").into(),
                            date: timestamp(&v),
                            session: value["sessionId"].as_str().unwrap_or(&session).into(),
                            t,
                        };
                        messages
                            .entry(format!("{provider}:{id}"))
                            .and_modify(|last| last.t.max(&ev.t))
                            .or_insert(ev);
                    }
                }
                continue;
            }
            for line in reader.lines() {
                jobs::checkpoint()?;
                let Ok(line) = line else {
                    malformed += 1;
                    continue;
                };
                if line.len() > 8 * 1024 * 1024 {
                    malformed += 1;
                    continue;
                }
                if line.trim().is_empty() {
                    continue;
                }
                let v: Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(_) => {
                        malformed += 1;
                        continue;
                    }
                };
                if v["type"] == "session_meta" {
                    session = v["payload"]["id"].as_str().unwrap_or(&session).to_owned();
                    project = v["payload"]["cwd"].as_str().unwrap_or(&project).to_owned();
                    continue;
                }
                if let Some(cwd) = v["cwd"].as_str() {
                    project = cwd.to_owned();
                }
                if v["type"] == "turn_context" {
                    if let Some(m) = v["payload"]["model"].as_str() {
                        model = m.to_owned();
                    }
                    if let Some(c) = v["payload"]["cwd"].as_str() {
                        project = c.to_owned();
                    }
                }
                if v["payload"]["type"] == "token_count" {
                    let info = &v["payload"]["info"];
                    let total = &info["total_token_usage"];
                    if !total.is_object() {
                        continue;
                    }
                    if let Some(m) = info["model"].as_str() {
                        model = m.to_owned();
                    }
                    let t = tokens(total);
                    codex_events.push((
                        v["timestamp"].as_str().unwrap_or("").into(),
                        Event {
                            provider: "Codex".into(),
                            model: model.clone(),
                            project: project.clone(),
                            date: timestamp(&v),
                            session: session.clone(),
                            t,
                        },
                    ));
                } else if let Some(u) = v["message"].get("usage") {
                    let mut t = tokens(u);
                    t.input += t.cache_read + t.cache_write;
                    let id = v["message"]["id"]
                        .as_str()
                        .or(v["uuid"].as_str())
                        .unwrap_or("");
                    if id.is_empty() {
                        continue;
                    }
                    let model = v["message"]["model"].as_str().unwrap_or("未知模型");
                    let ev = Event {
                        provider: "Claude".into(),
                        model: model.into(),
                        project: project.clone(),
                        date: timestamp(&v),
                        session: v["sessionId"].as_str().unwrap_or(&session).into(),
                        t,
                    };
                    messages
                        .entry(format!("Claude:{id}"))
                        .and_modify(|last| last.t.max(&ev.t))
                        .or_insert(ev);
                } else if p.to_string_lossy().contains("copilot") {
                    if let Some(u) = v.get("usage").or(v["data"].get("usage")) {
                        events.push(Event {
                            provider: "Copilot".into(),
                            model: v["model"].as_str().unwrap_or("未知模型").into(),
                            project: project.clone(),
                            date: timestamp(&v),
                            session: session.clone(),
                            t: tokens(u),
                        });
                    }
                }
            }
        }
    }
    codex_events.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, mut event) in codex_events {
        let last = cumulative.entry(event.session.clone()).or_default();
        let delta = event.t.delta(last);
        last.max(&event.t);
        event.t = delta;
        events.push(event);
    }
    events.extend(messages.into_values());
    let from = args["from"].as_str().unwrap_or("");
    let to = args["to"].as_str().unwrap_or("9999");
    let tool = args["provider"].as_str().unwrap_or("all");
    let project_filter = args["project"].as_str().unwrap_or("");
    events.retain(|e| {
        (e.date == "未知日期" || e.date.as_str() >= from && e.date.as_str() <= to)
            && (tool == "all" || e.provider == tool)
            && (project_filter.is_empty() || e.project == project_filter)
    });
    let mut totals = Tokens::default();
    let mut sessions = HashSet::new();
    let mut days = BTreeMap::<String, Tokens>::new();
    let mut models = BTreeMap::<String, Tokens>::new();
    let mut projects = BTreeMap::<String, Tokens>::new();
    let mut providers = BTreeMap::<String, Tokens>::new();
    for e in &events {
        totals.add(&e.t);
        sessions.insert(&e.session);
        days.entry(e.date.clone()).or_default().add(&e.t);
        models.entry(e.model.clone()).or_default().add(&e.t);
        projects.entry(e.project.clone()).or_default().add(&e.t);
        providers.entry(e.provider.clone()).or_default().add(&e.t);
    }
    let pricing = core.get_value("pricing")?["data"].clone();
    let mut total_cost = 0f64;
    let mut cost_complete = !events.is_empty();
    for e in &events {
        let price = &pricing[&e.model];
        if let (Some(input), Some(output)) = (price["input"].as_f64(), price["output"].as_f64()) {
            total_cost += (e.t.input.saturating_sub(e.t.cache_read + e.t.cache_write) as f64
                * input
                + e.t.output as f64 * output
                + e.t.cache_read as f64 * price["cache_read"].as_f64().unwrap_or(input)
                + e.t.cache_write as f64 * price["cache_write"].as_f64().unwrap_or(input))
                / 1_000_000f64;
        } else {
            cost_complete = false;
        }
    }
    let mut t = serde_json::to_value(&totals).map_err(err)?;
    t["sessions"] = json!(sessions.len());
    t["usd"] = if cost_complete {
        json!(total_cost)
    } else {
        Value::Null
    };
    t["total"] = json!(totals.input + totals.output);
    let rows = |map: BTreeMap<String, Tokens>| {
        map.into_iter()
            .map(|(name, t)| {
                let mut v = serde_json::to_value(t).unwrap();
                v["name"] = json!(name);
                v
            })
            .collect::<Vec<_>>()
    };
    Ok(
        json!({"totals":t,"days":rows(days),"models":rows(models),"projects":rows(projects),"providers":rows(providers),"malformed":malformed,"files":files,"truncated":truncated,"errors":errors,"note":"仅统计已识别的本地日志。日志格式变化、被删除的历史或未知项目可能不完整；不包含提示词内容。估算费用需为每个模型配置价格。"}),
    )
}
