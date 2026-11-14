use filemanager_core::Core;
#[test]
fn cleanup_moves_only_a_verified_extra_copy_and_undoes() {
    let (t, c) = setup();
    let d = t.path().join("copies");
    fs::create_dir(&d).unwrap();
    old(&d.join("a.txt"), "same");
    old(&d.join("b.txt"), "same");
    let scan = call(&c, "clean_scan", json!({"roots":[d]}));
    let category = scan["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "duplicates")
        .unwrap();
    let extra = category["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["selected"] == true)
        .cloned()
        .collect::<Vec<_>>();
    call(
        &c,
        "clean_apply",
        json!({"category":"duplicates","items":extra,"groups":category["groups"]}),
    );
    assert!(d.join("a.txt").exists());
    assert!(!d.join("b.txt").exists());
    call(&c, "undo", json!({}));
    assert!(d.join("b.txt").exists());
}
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::symlink,
    time::{Duration, SystemTime},
};
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
fn old(p: &std::path::Path, text: &str) {
    fs::write(p, text).unwrap();
    fs::File::options()
        .write(true)
        .open(p)
        .unwrap()
        .set_times(
            fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(86400 * 180)),
        )
        .unwrap();
}
#[test]
fn tidy_skips_active_downloads_and_new_files_and_undoes() {
    let (t, c) = setup();
    let d = t.path().join("Downloads");
    fs::create_dir(&d).unwrap();
    old(&d.join("old.pdf"), "PDF");
    old(&d.join("active.pdf.part"), "PART");
    fs::write(d.join("new.jpg"), "NEW").unwrap();
    let p = call(&c, "tidy_plan", json!({"path":d}));
    assert_eq!(p["items"].as_array().unwrap().len(), 1);
    assert_eq!(p["skipped"].as_array().unwrap().len(), 2);
    call(&c, "tidy_apply", json!({"plan":p}));
    assert!(d.join("文档/old.pdf").is_file());
    call(&c, "undo", json!({}));
    assert!(d.join("old.pdf").is_file());
}
#[test]
fn duplicate_scan_hashes_content_and_keeps_one_real_copy() {
    let (t, c) = setup();
    let d = t.path().join("files");
    fs::create_dir(&d).unwrap();
    old(&d.join("a.txt"), "same content");
    old(&d.join("b.txt"), "same content");
    old(&d.join("c.txt"), "different !!");
    fs::hard_link(d.join("a.txt"), d.join("hard.txt")).unwrap();
    symlink(&d, d.join("loop")).unwrap();
    let v = call(&c, "clean_scan", json!({"roots":[d]}));
    let g = v["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "duplicates")
        .unwrap();
    assert_eq!(g["groups"].as_array().unwrap().len(), 1);
    assert_eq!(g["groups"][0]["items"].as_array().unwrap().len(), 2);
    assert_eq!(g["reclaimable"], 12);
    assert_eq!(
        g["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["selected"] == true)
            .count(),
        1
    );
    assert!(d.join("b.txt").exists());
}
#[test]
fn stale_tidy_plan_does_not_move_changed_files() {
    let (t, c) = setup();
    let d = t.path().join("files");
    fs::create_dir(&d).unwrap();
    old(&d.join("file.pdf"), "old");
    let p = call(&c, "tidy_plan", json!({"path":d}));
    fs::write(d.join("file.pdf"), "new contents").unwrap();
    assert!(
        c.request(json!({"op":"tidy_apply","args":{"plan":p}}))
            .is_err()
    );
    assert!(d.join("file.pdf").exists());
}
#[test]
fn markdown_save_rejects_external_changes() {
    let (t, c) = setup();
    let p = t.path().join("note.md");
    fs::write(&p, "# Original").unwrap();
    let r = call(&c, "read_text", json!({"path":p}));
    fs::write(&p, "# External").unwrap();
    assert!(
        c.request(
            json!({"op":"write_text","args":{"path":p,"text":"# Mine","revision":r["revision"]}})
        )
        .is_err()
    );
    assert_eq!(fs::read_to_string(p).unwrap(), "# External");
}

#[test]
fn a_save_cannot_recreate_a_file_moved_after_opening() {
    let (t, c) = setup();
    let path = t.path().join("document.md");
    let moved = t.path().join("renamed.md");
    fs::write(&path, "original text").unwrap();
    let r = call(&c, "read_text", json!({"path":path}));
    fs::rename(&path, &moved).unwrap();
    assert!(c.request(json!({"op":"write_text","args":{"path":path,"text":"edited text","revision":r["revision"]}})).is_err());
    assert!(!path.exists());
    assert_eq!(fs::read_to_string(moved).unwrap(), "original text");
}

#[test]
fn consecutive_text_saves_can_be_undone_after_restarting() {
    let (t, c) = setup();
    let path = t.path().join("notes.md");
    fs::write(&path, "original").unwrap();
    for text in ["first revision", "second revision"] {
        let previous = call(&c, "read_text", json!({"path":path}));
        call(
            &c,
            "write_text",
            json!({"path":path,"text":text,"revision":previous["revision"]}),
        );
    }
    drop(c);
    let c = Core::with_paths(
        t.path().join("state"),
        t.path().to_owned(),
        t.path().join("Trash"),
    )
    .unwrap();
    for text in ["first revision", "original"] {
        let result = call(&c, "undo", json!({}));
        assert_eq!(result["conflicts"], json!([]));
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }
}

#[test]
fn exported_reports_do_not_overwrite_and_remain_undoable() {
    let t = tempfile::tempdir().unwrap();
    let c = filemanager_core::Core::with_paths(
        t.path().join("state"),
        t.path().to_owned(),
        t.path().join("Trash"),
    )
    .unwrap();
    let v=c.request(serde_json::json!({"op":"create_text","args":{"directory":t.path(),"name":"report.csv","text":"file,size\na.txt,12"}})).unwrap();
    assert!(v.is_object());
    assert!(c.request(serde_json::json!({"op":"create_text","args":{"directory":t.path(),"name":"report.csv","text":"must not replace"}})).is_err());
    assert_eq!(
        std::fs::read_to_string(t.path().join("report.csv")).unwrap(),
        "file,size\na.txt,12"
    );
    c.request(serde_json::json!({"op":"undo"})).unwrap();
    assert!(!t.path().join("report.csv").exists());
}
