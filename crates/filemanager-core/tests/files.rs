use filemanager_core::Core;
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;

fn setup() -> (TempDir, Core) {
    let temp = tempfile::tempdir().unwrap();
    let core = Core::with_paths(
        temp.path().join("state"),
        temp.path().to_path_buf(),
        temp.path().join("Trash"),
    )
    .unwrap();
    (temp, core)
}
fn call(core: &Core, op: &str, args: Value) -> Value {
    core.request(json!({"op": op, "args": args})).unwrap()
}
fn put(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
}

#[test]
fn lists_real_files_and_hides_dotfiles_without_following_links() {
    let (temp, core) = setup();
    let dir = temp.path().join("files");
    fs::create_dir(&dir).unwrap();
    put(&dir.join("报告.txt"), "hello");
    put(&dir.join(".secret"), "hidden");
    fs::create_dir(dir.join("项目")).unwrap();
    std::os::unix::fs::symlink(&dir, dir.join("loop")).unwrap();
    let result = call(&core, "list_directory", json!({"path":dir}));
    let names: Vec<_> = result["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["项目", "loop", "报告.txt"]);
    assert_eq!(result["entries"][2]["size"], 5);
    assert_eq!(result["entries"][1]["is_symlink"], true);
    let result = call(
        &core,
        "list_directory",
        json!({"path":dir,"show_hidden":true}),
    );
    assert_eq!(result["total"], 4);
}

#[test]
fn moves_and_undoes_the_exact_file_even_with_shell_characters() {
    let (temp, core) = setup();
    let source = temp.path().join("source");
    let dest = temp.path().join("dest");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&dest).unwrap();
    let name = "中文 ' $(touch NEVER)\n.txt";
    put(&source.join(name), "untouched content");
    call(
        &core,
        "operate",
        json!({"kind":"move","paths":[source.join(name)],"destination":dest,"conflict":"keep_both"}),
    );
    assert!(!source.join(name).exists());
    assert_eq!(
        fs::read_to_string(dest.join(name)).unwrap(),
        "untouched content"
    );
    call(&core, "undo", json!({}));
    assert_eq!(
        fs::read_to_string(source.join(name)).unwrap(),
        "untouched content"
    );
    assert!(!dest.join(name).exists());
    assert!(!source.join("NEVER").exists());
}

#[test]
fn replace_and_undo_restore_both_originals() {
    let (temp, core) = setup();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    put(&a.join("same.txt"), "new");
    put(&b.join("same.txt"), "old");
    call(
        &core,
        "operate",
        json!({"kind":"copy","paths":[a.join("same.txt")],"destination":b,"conflict":"replace"}),
    );
    assert_eq!(fs::read_to_string(b.join("same.txt")).unwrap(), "new");
    call(&core, "undo", json!({}));
    assert_eq!(fs::read_to_string(b.join("same.txt")).unwrap(), "old");
    assert_eq!(fs::read_to_string(a.join("same.txt")).unwrap(), "new");
}

#[test]
fn undo_does_not_delete_a_copy_edited_after_the_operation() {
    let (temp, core) = setup();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    put(&a.join("file.txt"), "original");
    call(
        &core,
        "operate",
        json!({"kind":"copy","paths":[a.join("file.txt")],"destination":b,"conflict":"keep_both"}),
    );
    put(&b.join("file.txt"), "later edits must survive");
    let result = call(&core, "undo", json!({}));
    assert!(!result["conflicts"].as_array().unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(b.join("file.txt")).unwrap(),
        "later edits must survive"
    );
}

#[test]
fn prevents_moving_a_directory_into_its_own_descendant() {
    let (temp, core) = setup();
    let dir = temp.path().join("parent");
    fs::create_dir_all(dir.join("child")).unwrap();
    let result=core.request(json!({"op":"operate","args":{"kind":"move","paths":[dir],"destination":dir.join("child"),"conflict":"keep_both"}}));
    assert!(result.unwrap_err().contains("自身"));
    assert!(dir.join("child").exists());
}

#[test]
fn batch_rename_previews_collisions_and_preserves_extensions() {
    let (temp, core) = setup();
    let dir = temp.path().join("files");
    fs::create_dir(&dir).unwrap();
    put(&dir.join("a.txt"), "a");
    put(&dir.join("b.txt"), "b");
    let args = json!({"paths":[dir.join("a.txt"),dir.join("b.txt")],"mode":"number","text":"报告","start":1,"padding":2});
    let plan = call(&core, "rename_plan", args);
    assert_eq!(plan["items"][0]["name"], "报告 01.txt");
    call(&core, "rename_apply", json!({"items":plan["items"]}));
    assert_eq!(fs::read_to_string(dir.join("报告 01.txt")).unwrap(), "a");
    call(&core, "undo", json!({}));
    assert!(dir.join("a.txt").exists());
    assert!(dir.join("b.txt").exists());
    let collision = call(
        &core,
        "rename_plan",
        json!({"paths":[dir.join("a.txt"),dir.join("b.txt")],"mode":"replace","find":"a","text":"b"}),
    );
    assert_eq!(collision["valid"], false);
}

#[test]
fn non_utf8_names_have_reversible_path_tokens() {
    use std::os::unix::ffi::OsStringExt;
    let (temp, core) = setup();
    let dir = temp.path().join("files");
    fs::create_dir(&dir).unwrap();
    let original = dir.join(std::ffi::OsString::from_vec(vec![
        b'a', 0xff, b'.', b't', b'x', b't',
    ]));
    put(&original, "bytes");
    let entries = call(&core, "list_directory", json!({"path":dir}));
    let token = entries["entries"][0]["path"].as_str().unwrap();
    let result = call(&core, "read_text", json!({"path":token}));
    assert_eq!(result["text"], "bytes");
}

#[test]
fn cyclic_rename_undo_restores_each_file_and_survives_restart() {
    let (temp, core) = setup();
    let dir = temp.path().join("files");
    fs::create_dir(&dir).unwrap();
    put(&dir.join("a.txt"), "content A");
    put(&dir.join("b.txt"), "content B");
    call(
        &core,
        "rename_apply",
        json!({"items":[{"path":dir.join("a.txt"),"name":"b.txt"},{"path":dir.join("b.txt"),"name":"a.txt"}]}),
    );
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "content B");
    let restarted = Core::with_paths(
        core.data_dir.clone(),
        core.home_dir.clone(),
        core.trash_dir.clone(),
    )
    .unwrap();
    let result = call(&restarted, "undo", json!({}));
    assert_eq!(result["conflicts"], json!([]));
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "content A");
    assert_eq!(fs::read_to_string(dir.join("b.txt")).unwrap(), "content B");
}

#[test]
fn rename_can_explicitly_include_the_extension() {
    let (temp, core) = setup();
    let file = temp.path().join("report.txt");
    put(&file, "content");
    let plan = call(
        &core,
        "rename_plan",
        json!({"paths":[file],"mode":"replace","find":".txt","text":".md","keep_extension":false}),
    );
    assert_eq!(plan["items"][0]["name"], "report.md");
}

#[test]
fn a_created_folder_can_be_undone_after_its_contents_were_undone() {
    let (temp, core) = setup();
    let source = temp.path().join("note.txt");
    put(&source, "original");
    call(
        &core,
        "create_folder",
        json!({"parent":temp.path(),"name":"new"}),
    );
    let directory = temp.path().join("new");
    call(
        &core,
        "operate",
        json!({"kind":"copy","paths":[source],"destination":directory}),
    );
    assert_eq!(call(&core, "undo", json!({}))["conflicts"], json!([]));
    assert_eq!(call(&core, "undo", json!({}))["conflicts"], json!([]));
    assert!(!directory.exists());
}

#[test]
fn conflicts_can_be_resolved_per_file_and_replaced_originals_are_undoable() {
    let (temp, core) = setup();
    let source = temp.path().join("source");
    let dest = temp.path().join("dest");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&dest).unwrap();
    for name in ["keep", "replace", "skip"] {
        put(&source.join(name), "source");
        put(&dest.join(name), "original");
    }
    let mut policies = serde_json::Map::new();
    policies.insert(
        source.join("keep").to_string_lossy().into(),
        json!("keep_both"),
    );
    policies.insert(
        source.join("replace").to_string_lossy().into(),
        json!("replace"),
    );
    policies.insert(source.join("skip").to_string_lossy().into(), json!("skip"));
    let paths: Vec<_> = ["keep", "replace", "skip"].map(|n| source.join(n)).into();
    let result = call(
        &core,
        "operate",
        json!({"kind":"copy","paths":paths,"destination":dest,"resolutions":policies}),
    );
    assert_eq!(result["count"], 2);
    assert_eq!(fs::read_to_string(dest.join("skip")).unwrap(), "original");
    assert_eq!(fs::read_to_string(dest.join("keep")).unwrap(), "original");
    assert_eq!(fs::read_to_string(dest.join("replace")).unwrap(), "source");
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 4);
    let result = call(&core, "undo", json!({}));
    assert_eq!(result["conflicts"], json!([]));
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 3);
    assert_eq!(
        fs::read_to_string(dest.join("replace")).unwrap(),
        "original"
    );
}

#[test]
fn moving_and_trashing_across_devices_preserves_and_restores_exact_contents() {
    use std::os::unix::fs::MetadataExt;
    let (temp, core) = setup();
    let external = tempfile::tempdir_in("/dev/shm").unwrap();
    if fs::metadata(temp.path()).unwrap().dev() == fs::metadata(external.path()).unwrap().dev() {
        return;
    }
    let source = temp.path().join("across 中文 ' line\n.txt");
    put(&source, "cross-device content");
    call(
        &core,
        "operate",
        json!({"kind":"move","paths":[source],"destination":external.path()}),
    );
    let moved = external.path().join(source.file_name().unwrap());
    assert_eq!(fs::read_to_string(&moved).unwrap(), "cross-device content");
    assert!(!source.exists());
    call(&core, "undo", json!({}));
    assert_eq!(fs::read_to_string(&source).unwrap(), "cross-device content");
    assert!(!moved.exists());
    put(&moved, "external trash content");
    let item = filemanager_core::trash::plan(&core, &moved).unwrap();
    assert_eq!(
        fs::metadata(Path::new(&item.trashed).parent().unwrap())
            .unwrap()
            .dev(),
        fs::metadata(external.path()).unwrap().dev()
    );
    call(&core, "operate", json!({"kind":"trash","paths":[moved]}));
    assert!(!moved.exists());
    let result = call(&core, "undo", json!({}));
    assert_eq!(result["conflicts"], json!([]));
    assert_eq!(fs::read_to_string(moved).unwrap(), "external trash content");
}

#[test]
fn replacing_via_a_directory_alias_never_touches_the_source() {
    let (t, c) = setup();
    let dir = t.path().join("original");
    let alias = t.path().join("alias");
    fs::create_dir(&dir).unwrap();
    std::os::unix::fs::symlink(&dir, &alias).unwrap();
    put(&dir.join("same.txt"), "original contents");
    let args = json!({"kind":"copy","paths":[alias.join("same.txt")],"destination":dir});
    let preview = call(&c, "operate_check", args.clone());
    assert_eq!(preview["conflicts"][0]["can_replace"], false);
    let mut args = args;
    args["conflict"] = json!("replace");
    assert!(c.request(json!({"op":"operate","args":args})).is_err());
    assert_eq!(
        fs::read_to_string(dir.join("same.txt")).unwrap(),
        "original contents"
    );
}

#[test]
fn a_replacement_cannot_remove_another_selected_source_or_its_parent() {
    let (t, c) = setup();
    let dir = t.path().join("folder");
    let nested = dir.join("folder");
    fs::create_dir_all(&nested).unwrap();
    put(&dir.join("note"), "sibling");
    let args = json!({"kind":"move","paths":[nested],"destination":t.path()});
    assert_eq!(
        call(&c, "operate_check", args.clone())["conflicts"][0]["can_replace"],
        false
    );
    let mut args = args;
    args["conflict"] = json!("replace");
    assert!(c.request(json!({"op":"operate","args":args})).is_err());
    assert!(nested.exists());
    assert_eq!(fs::read_to_string(dir.join("note")).unwrap(), "sibling");

    put(&t.path().join("note"), "other source");
    let args = json!({"kind":"copy","paths":[t.path().join("note"),dir.join("note")],"destination":dir,"conflict":"replace"});
    assert!(c.request(json!({"op":"operate","args":args})).is_err());
    assert_eq!(fs::read_to_string(dir.join("note")).unwrap(), "sibling");
}

#[test]
fn replacing_after_the_confirmed_target_changes_is_rejected() {
    let (t, c) = setup();
    let dest = t.path().join("dest");
    fs::create_dir(&dest).unwrap();
    let source = t.path().join("note");
    let target = dest.join("note");
    put(&source, "source");
    put(&target, "old destination");
    let mut args = json!({"kind":"copy","paths":[source],"destination":dest});
    let result = call(&c, "operate_check", args.clone());
    let shown = &result["conflicts"][0];
    let path = shown["source"]["path"].as_str().unwrap();
    args["resolutions"][path] = json!("replace");
    args["replacement_versions"][path] = shown["replacement_version"].clone();
    fs::rename(&target, dest.join("original note")).unwrap();
    put(&target, "new destination");
    assert!(c.request(json!({"op":"operate","args":args})).is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "new destination");
    assert_eq!(fs::read_to_string(source).unwrap(), "source");
}

#[test]
fn cross_volume_move_of_read_only_tree_preserves_metadata_and_prior_undo() {
    use rustix::fs::{XattrFlags, lgetxattr, lsetxattr};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let (t, c) = setup();
    let external = tempfile::tempdir_in("/dev/shm").unwrap();
    if fs::metadata(t.path()).unwrap().dev() == fs::metadata(external.path()).unwrap().dev() {
        return;
    }
    let folder = t.path().join("tree");
    let locked = folder.join("readonly");
    let sibling = t.path().join("same-volume");
    fs::create_dir(&sibling).unwrap();
    fs::create_dir_all(&locked).unwrap();
    put(&locked.join("one"), "unchanged");
    fs::hard_link(locked.join("one"), folder.join("two")).unwrap();
    lsetxattr(
        locked.join("one"),
        "user.filemanager-test",
        b"preserved",
        XattrFlags::empty(),
    )
    .unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let original = fs::metadata(&folder).unwrap();

    call(
        &c,
        "operate",
        json!({"kind":"move","paths":[folder],"destination":sibling}),
    );
    call(
        &c,
        "operate",
        json!({"kind":"move","paths":[sibling.join("tree")],"destination":external.path()}),
    );
    let published = external.path().join("tree");
    assert!(!sibling.join("tree").exists());
    assert_eq!(
        fs::metadata(published.join("readonly")).unwrap().mode() & 0o777,
        0o555
    );
    assert_eq!(
        fs::metadata(&published).unwrap().mtime_nsec(),
        original.mtime_nsec()
    );
    let one = published.join("readonly/one");
    assert_eq!(
        fs::metadata(&one).unwrap().ino(),
        fs::metadata(published.join("two")).unwrap().ino()
    );
    let mut attribute = [0u8; 40];
    let n = lgetxattr(&one, "user.filemanager-test", attribute.as_mut_slice()).unwrap();
    assert_eq!(&attribute[..n], b"preserved");
    assert_eq!(call(&c, "undo", json!({}))["conflicts"], json!([]));
    assert_eq!(call(&c, "undo", json!({}))["conflicts"], json!([]));
    assert!(!published.exists());
    assert_eq!(fs::metadata(&folder).unwrap().ino(), original.ino());
    assert_eq!(fs::read_to_string(locked.join("one")).unwrap(), "unchanged");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn conflicting_undo_can_be_deferred_without_removing_its_recovery_record() {
    let (t, c) = setup();
    let source = t.path().join("note.txt");
    let dest = t.path().join("copies");
    fs::create_dir(&dest).unwrap();
    put(&source, "original");
    call(
        &c,
        "create_folder",
        json!({"parent":t.path(),"name":"earlier"}),
    );
    call(
        &c,
        "operate",
        json!({"kind":"copy","paths":[source],"destination":dest}),
    );
    let copy = dest.join("note.txt");
    put(&copy, "edited after copy");
    let failed = call(&c, "undo", json!({}));
    assert!(!failed["conflicts"].as_array().unwrap().is_empty());
    assert_eq!(
        call(&c, "defer_undo", json!({"id":failed["id"]}))["ok"],
        true
    );
    assert_eq!(call(&c, "undo", json!({}))["conflicts"], json!([]));
    assert!(!t.path().join("earlier").exists());
    assert_eq!(fs::read_to_string(copy).unwrap(), "edited after copy");
    assert!(
        call(&c, "history", json!({}))["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == failed["id"] && item["status"] == "deferred")
    );
}

#[test]
fn keep_both_reserves_distinct_names_for_same_named_sources() {
    let (t, c) = setup();
    let a = t.path().join("a");
    let b = t.path().join("b");
    let out = t.path().join("out");
    for dir in [&a, &b, &out] {
        fs::create_dir(dir).unwrap();
    }
    put(&a.join("same.txt"), "first");
    put(&b.join("same.txt"), "second");
    let args = json!({"kind":"copy","paths":[a.join("same.txt"),b.join("same.txt")],"destination":out,"conflict":"keep_both"});
    let result = call(&c, "operate", args);
    assert_eq!(result["count"], 2);
    assert_eq!(fs::read_to_string(out.join("same.txt")).unwrap(), "first");
    assert_eq!(
        fs::read_to_string(out.join("same 2.txt")).unwrap(),
        "second"
    );
    call(&c, "undo", json!({}));
    assert_eq!(fs::read_dir(out).unwrap().count(), 0);
}
