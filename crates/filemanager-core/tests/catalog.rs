use filemanager_core::Core;
use serde_json::{Value, json};
#[test]
fn workspace_bookmark_links_persist_and_undo() {
    let t = tempfile::tempdir().unwrap();
    let c = Core::with_paths(
        t.path().join("state"),
        t.path().to_owned(),
        t.path().join("Trash"),
    )
    .unwrap();
    let call = |op: &str, args: Value| c.request(json!({"op":op,"args":args})).unwrap();
    let data = json!({"workspaces":[{"id":"w","name":"Work","links":["b"]}],"bookmarks":[{"id":"b","url":"https://example.com","collections":["c"]}],"collections":[{"id":"c","name":"Links"}]});
    let r = call(
        "catalog_save",
        json!({"namespace":"catalog","data":data,"revision":0}),
    );
    assert_eq!(r["revision"], 1);
    assert!(
        c.request(
            json!({"op":"catalog_save","args":{"namespace":"catalog","data":{},"revision":0}})
        )
        .is_err()
    );
    call("undo", json!({}));
    assert!(call("catalog_get", json!({"namespace":"catalog"}))["data"].is_null());
}

#[test]
fn consecutive_catalog_undos_restore_each_prior_state() {
    let t = tempfile::tempdir().unwrap();
    let c = Core::with_paths(
        t.path().join("state"),
        t.path().to_owned(),
        t.path().join("Trash"),
    )
    .unwrap();
    for data in [
        json!({"workspaces":[{"id":"a"}]}),
        json!({"workspaces":[{"id":"a"},{"id":"b"}]}),
    ] {
        c.request(json!({"op":"catalog_save","args":{"data":data}}))
            .unwrap();
    }
    assert_eq!(
        c.request(json!({"op":"undo"})).unwrap()["conflicts"],
        json!([])
    );
    assert_eq!(
        c.request(json!({"op":"catalog_get"})).unwrap()["data"]["workspaces"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        c.request(json!({"op":"undo"})).unwrap()["conflicts"],
        json!([])
    );
    assert!(c.request(json!({"op":"catalog_get"})).unwrap()["data"].is_null());
}

#[test]
fn legacy_database_is_opened_in_place_with_settings_and_history_intact() {
    let t = tempfile::tempdir().unwrap();
    let state = t.path().join("state");
    let c = filemanager_core::Core::with_paths(
        state.clone(),
        t.path().to_owned(),
        t.path().join("Trash"),
    )
    .unwrap();
    c.request(serde_json::json!({"op":"catalog_save","args":{"namespace":"settings","data":{"theme":"dark","skin":"forest"}}})).unwrap();
    drop(c);
    std::fs::rename(
        state.join("filemanager.sqlite"),
        state.join("fileos.sqlite"),
    )
    .unwrap();
    let c = filemanager_core::Core::with_paths(
        state.clone(),
        t.path().to_owned(),
        t.path().join("Trash"),
    )
    .unwrap();
    let v = c.request(serde_json::json!({"op":"bootstrap"})).unwrap();
    assert_eq!(v["settings"]["data"]["skin"], "forest");
    assert!(!state.join("filemanager.sqlite").exists());
}
