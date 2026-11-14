use filemanager_core::Core;
use serde_json::{Value, json};
use std::fs;
fn setup() -> (tempfile::TempDir, Core) {
    let t = tempfile::tempdir().unwrap();
    let c = Core::with_paths(
        t.path().join("state"),
        t.path().to_owned(),
        t.path().join("Trash"),
    )
    .unwrap();
    (t, c)
}
fn call(c: &Core, op: &str, args: Value) -> Value {
    c.request(json!({"op":op,"args":args})).unwrap()
}
#[test]
fn codex_usage_uses_deltas_and_survives_a_truncated_record() {
    let (t, c) = setup();
    let p = t.path().join("logs");
    fs::create_dir(&p).unwrap();
    let text=[json!({"type":"session_meta","payload":{"id":"session-a","cwd":"/projects/a"}}).to_string(),json!({"timestamp":"2026-11-13T04:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-test","total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":10}}}}).to_string(),"{broken".into(),json!({"timestamp":"2026-11-13T05:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"model":"gpt-test","total_token_usage":{"input_tokens":150,"cached_input_tokens":35,"output_tokens":25}}}}).to_string()];
    fs::write(p.join("session.jsonl"), text.join("\n")).unwrap();
    let v = call(&c, "agent_usage", json!({"roots":[p]}));
    assert_eq!(v["totals"]["input"], 150);
    assert_eq!(v["totals"]["output"], 25);
    assert_eq!(v["totals"]["cache_read"], 35);
    assert_eq!(v["totals"]["sessions"], 1);
    assert_eq!(v["malformed"], 1);
    assert!(v["totals"]["usd"].is_null());
}
#[test]
fn mcp_toml_edit_keeps_other_fields_and_can_be_undone() {
    let (t, c) = setup();
    let p = t.path().join(".codex");
    fs::create_dir(&p).unwrap();
    let file = p.join("config.toml");
    let original = "# keep this comment\nmodel = \"my-model\"\n[projects.\"/work\"]\ntrust_level = \"trusted\"\n";
    fs::write(&file, original).unwrap();
    let r = call(&c, "read_text", json!({"path":file}));
    call(
        &c,
        "mcp_save",
        json!({"path":file,"name":"test-server","config":{"command":"node","args":["server.js"]},"revision":r["revision"]}),
    );
    let after = fs::read_to_string(&file).unwrap();
    assert!(after.contains("# keep this comment"));
    assert!(after.contains("trust_level = \"trusted\""));
    assert!(after.contains("[mcp_servers.test-server]"));
    call(&c, "undo", json!({}));
    assert_eq!(fs::read_to_string(file).unwrap(), original);
}
#[test]
fn agent_copy_excludes_personal_settings_and_secrets() {
    let (t, c) = setup();
    let a = t.path().join("a");
    let b = t.path().join("b");
    fs::create_dir_all(a.join(".claude")).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("CLAUDE.md"), "# Rules").unwrap();
    fs::write(a.join(".claude/settings.json"), "{\"permissions\":{}}").unwrap();
    fs::write(
        a.join(".claude/settings.local.json"),
        "{\"token\":\"do-not-copy\"}",
    )
    .unwrap();
    fs::write(a.join(".env"), "SECRET=x").unwrap();
    call(&c, "agent_copy", json!({"source":a,"destination":b}));
    assert!(b.join("CLAUDE.md").exists());
    assert!(b.join(".claude/settings.json").exists());
    assert!(!b.join(".claude/settings.local.json").exists());
    assert!(!b.join(".env").exists());
}
#[test]
fn operation_preflight_does_not_mutate() {
    let (t, c) = setup();
    let a = t.path().join("a");
    let b = t.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("file"), "content").unwrap();
    let v = call(
        &c,
        "operate_check",
        json!({"kind":"move","paths":[a.join("file")],"destination":b}),
    );
    assert_eq!(v["requires_confirmation"], false);
    assert!(a.join("file").exists());
    assert!(!b.join("file").exists());
}

#[test]
fn mcp_jsonc_edit_preserves_neighbor_comments_and_trailing_commas() {
    let (t, c) = setup();
    let file = t.path().join("mcp.json");
    let original = "{\n // outer note\n \"custom\": true,\n \"mcpServers\": {\n   // keep neighbor\n   \"old\": {\"command\":\"node\",},\n },\n}\n";
    fs::write(&file, original).unwrap();
    let r = call(&c, "read_text", json!({"path":file}));
    call(
        &c,
        "mcp_save",
        json!({"path":file,"name":"new","config":{"command":"python3"},"revision":r["revision"]}),
    );
    let after = fs::read_to_string(&file).unwrap();
    assert!(after.contains("// keep neighbor"));
    assert!(after.contains("\"old\": {\"command\":\"node\",}"));
    assert_eq!(
        filemanager_core::mcp::parse(&file, &after).unwrap()["new"]["command"],
        "python3"
    );
}

#[test]
fn mcp_jsonc_edit_keeps_comments_around_keys_and_the_root_closing_brace() {
    let (t, c) = setup();
    let file = t.path().join("mcp.json");
    for (original, expected) in [
        (
            "{\n \"mcp\" /* group */ : /* servers */ {\n \"old\" /* keep */ : {\"command\":\"node\"}\n }\n} // trailing }\n",
            json!({"old":{"command":"node"},"new":{"command":"python3"}}),
        ),
        (
            "{\n \"other\": true // trailing }\n} // after the root }\n",
            json!({"new":{"command":"python3"}}),
        ),
    ] {
        fs::write(&file, original).unwrap();
        let r = call(&c, "read_text", json!({"path":file}));
        call(
            &c,
            "mcp_save",
            json!({"path":file,"name":"new","config":{"command":"python3"},"revision":r["revision"]}),
        );
        let changed = fs::read_to_string(&file).unwrap();
        assert!(changed.contains("// trailing }"));
        assert_eq!(
            filemanager_core::mcp::parse(&file, &changed).unwrap(),
            expected
        );
    }
}

#[test]
fn codex_usage_is_chronological_across_copied_session_logs() {
    let (t, c) = setup();
    let early = t.path().join("early");
    let later = t.path().join("later");
    fs::create_dir(&early).unwrap();
    fs::create_dir(&later).unwrap();
    for (dir, date, total) in [(&later, "2026-11-14", 150), (&early, "2026-11-13", 100)] {
        let rows = [
            json!({"type":"session_meta","payload":{"id":"same-session"}}),
            json!({"timestamp":format!("{date}T01:00:00Z"),"payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":total,"output_tokens":10}}}}),
        ];
        fs::write(
            dir.join("session.jsonl"),
            rows.iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
    }
    let v = call(&c, "agent_usage", json!({"roots":[later,early]}));
    assert_eq!(v["totals"]["input"], 150);
    let days = v["days"].as_array().unwrap();
    assert_eq!(days[0]["input"], 100);
    assert_eq!(days[1]["input"], 50);
}

#[test]
fn claude_input_includes_cache_tokens_without_double_billing() {
    let (t, c) = setup();
    let dir = t.path().join("logs");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("claude.jsonl"),json!({"timestamp":"2026-11-14T00:00:00Z","message":{"id":"msg1","model":"test","usage":{"input_tokens":50,"output_tokens":10,"cache_read_input_tokens":20,"cache_creation_input_tokens":5}}}).to_string()).unwrap();
    call(
        &c,
        "catalog_save",
        json!({"namespace":"pricing","data":{"test":{"input":2,"output":4,"cache_read":1,"cache_write":3}}}),
    );
    let v = call(&c, "agent_usage", json!({"roots":[dir]}));
    assert_eq!(v["totals"]["input"], 75);
    assert!((v["totals"]["usd"].as_f64().unwrap() - 0.000175).abs() < 1e-10);
}
