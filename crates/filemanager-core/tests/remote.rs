use filemanager_core::{Core, Result};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Cursor, Read},
    path::Path,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tiny_http::{Header, Method, Response, Server, StatusCode};

fn core(root: &Path) -> Core {
    Core::with_paths(root.join("state"), root.to_owned(), root.join("Trash")).unwrap()
}

fn request(core: &Core, op: &str, args: Value) -> Result<Value> {
    core.request(json!({ "op": op, "args": args }))
}

fn s3(endpoint: &str) -> Value {
    json!({
        "label": "Fixture",
        "endpoint": endpoint,
        "root": "/test/",
        "provider": {
            "kind": "s3",
            "bucket": "fixture-bucket",
            "region": "us-east-1",
            "access_key": "fixture-id"
        }
    })
}

fn minio(endpoint: &str) -> Value {
    json!({"label":"MinIO account", "endpoint":endpoint, "provider":{"kind":"minio", "user":"fixture-user"}})
}

fn assert_s3_login(request: &tiny_http::Request, region: &str) {
    let auth = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("authorization"))
        .unwrap()
        .value
        .as_str();
    assert!(auth.starts_with("AWS4-HMAC-SHA256 "));
    assert!(auth.contains("Credential=fixture-user/"));
    assert!(auth.contains(&format!("/{region}/s3/aws4_request")));
    assert!(!auth.contains("fixture-password"));
}

#[test]
fn minio_account_lists_buckets_detects_region_and_binds_cursors_to_each_bucket() {
    let temp = tempfile::tempdir().unwrap();
    let app = core(temp.path());
    let server = Server::http("127.0.0.1:0").unwrap();
    let profile = request(&app, "remote_save", json!({"config":minio(&format!("http://{}",server.server_addr())), "secret":{"value":"fixture-password"}})).unwrap();
    assert_eq!(profile["config"]["provider"]["bucket"], "");
    assert_eq!(profile["config"]["provider"]["region"], "");
    let serve = thread::spawn(move || {
        let first = next_request(&server);
        assert_eq!(first.url(), "/?max-buckets=1000");
        assert_s3_login(&first, "us-east-1");
        first.respond(Response::from_string("<ListAllMyBucketsResult><Buckets><Bucket><Name>alpha-bucket</Name><CreationDate>2026-01-01T00:00:00Z</CreationDate></Bucket><Bucket><Name>beta-bucket</Name></Bucket></Buckets><ContinuationToken>next/+=</ContinuationToken></ListAllMyBucketsResult>")).unwrap();
        let second = next_request(&server);
        assert!(second.url().contains("continuation-token=next%2F%2B%3D"));
        second.respond(Response::from_string("<ListAllMyBucketsResult><Buckets><Bucket><Name>gamma-bucket</Name></Bucket></Buckets></ListAllMyBucketsResult>")).unwrap();
        let location = next_request(&server);
        assert_eq!(location.url(), "/alpha-bucket?location");
        assert_s3_login(&location, "us-east-1");
        location.respond(Response::from_string("<Error><Code>AuthorizationHeaderMalformed</Code><Region>custom-region-1</Region></Error>").with_status_code(400)).unwrap();
        let retry = next_request(&server);
        assert_eq!(retry.url(), "/alpha-bucket?location");
        assert_s3_login(&retry, "custom-region-1");
        retry.respond(Response::from_string("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<LocationConstraint xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">custom-region-1</LocationConstraint>")).unwrap();
        let objects = next_request(&server);
        assert!(objects.url().starts_with("/alpha-bucket?"));
        assert_s3_login(&objects, "custom-region-1");
        objects.respond(Response::from_string("<ListBucketResult><IsTruncated>false</IsTruncated><Contents><Key>first.txt</Key><Size>1</Size><LastModified>2026-01-01T00:00:00Z</LastModified></Contents><Contents><Key>second.txt</Key><Size>2</Size><LastModified>2026-01-01T00:00:00Z</LastModified></Contents></ListBucketResult>")).unwrap();
    });
    let conn = json!({"id":profile["id"],"revision":1,"path":"","page_size":1});
    let page1 = request(&app, "remote_list", conn.clone()).unwrap();
    assert_eq!(page1["entries"][0]["name"], "alpha-bucket");
    let mut next = conn.clone();
    next["cursor"] = page1["next_cursor"].clone();
    let mut wrong = next.clone();
    wrong["bucket"] = json!("alpha-bucket");
    assert!(
        request(&app, "remote_list", wrong)
            .unwrap_err()
            .contains("不属于此连接或目录")
    );
    let page2 = request(&app, "remote_list", next.clone()).unwrap();
    assert_eq!(page2["entries"][0]["name"], "beta-bucket");
    next["cursor"] = page2["next_cursor"].clone();
    let page3 = request(&app, "remote_list", next).unwrap();
    assert_eq!(page3["entries"][0]["name"], "gamma-bucket");
    assert!(page3["next_cursor"].is_null());
    let mut objects = conn.clone();
    objects["bucket"] = json!("alpha-bucket");
    let page = request(&app, "remote_list", objects.clone()).unwrap();
    assert_eq!(page["entries"][0]["name"], "first.txt");
    objects["cursor"] = page["next_cursor"].clone();
    objects["bucket"] = json!("beta-bucket");
    assert!(
        request(&app, "remote_list", objects.clone())
            .unwrap_err()
            .contains("不属于此连接或目录")
    );
    objects["bucket"] = json!("alpha-bucket");
    assert_eq!(
        request(&app, "remote_list", objects).unwrap()["entries"][0]["name"],
        "second.txt"
    );
    assert!(
        request(
            &app,
            "remote_mkdir",
            json!({"id":profile["id"],"revision":1,"path":"","name":"not-a-bucket"})
        )
        .unwrap_err()
        .contains("请先选择 Bucket")
    );
    serve.join().unwrap();
}

#[test]
fn minio_restricted_accounts_can_open_a_known_bucket_and_login_errors_hide_server_text() {
    let temp = tempfile::tempdir().unwrap();
    let app = core(temp.path());
    let server = Server::http("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", server.server_addr());
    let mut config = minio(&endpoint);
    config["read_only"] = json!(true);
    let draft = json!({"config":config,"secret":{"value":"fixture-password"}});
    let profile = request(&app, "remote_save", draft.clone()).unwrap();
    let serve = thread::spawn(move || {
        next_request(&server)
            .respond(
                Response::from_string(
                    "<Error><Code>AccessDenied</Code><Message>fixture-password</Message></Error>",
                )
                .with_status_code(403),
            )
            .unwrap();
        let location = next_request(&server);
        assert_eq!(location.url(), "/known-bucket?location");
        location
            .respond(
                Response::from_string("<Error><Code>AccessDenied</Code></Error>")
                    .with_status_code(403),
            )
            .unwrap();
        next_request(&server)
            .respond(Response::from_string(
                "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>",
            ))
            .unwrap();
        next_request(&server).respond(Response::from_string("<Error><Code>SignatureDoesNotMatch</Code><Message>fixture-password</Message></Error>").with_status_code(403)).unwrap();
        next_request(&server)
            .respond(Response::from_string(
                "<!doctype html><html><title>MinIO Console</title></html>",
            ))
            .unwrap();
        next_request(&server)
            .respond(Response::from_string(
                "<ListAllMyBucketsResult><Buckets/></ListAllMyBucketsResult>",
            ))
            .unwrap();
    });
    let error = request(&app, "remote_test", draft.clone()).unwrap_err();
    assert!(error.contains("没有列桶权限") && !error.contains("fixture-password"));
    let listing = request(
        &app,
        "remote_list",
        json!({"id":profile["id"],"revision":1,"bucket":"known-bucket","path":""}),
    )
    .unwrap();
    assert_eq!(listing["entries"], json!([]));
    assert!(request(&app,"remote_mkdir",json!({"id":profile["id"],"revision":1,"bucket":"known-bucket","path":"","name":"blocked"})).unwrap_err().contains("只读"));
    let bad = request(&app, "remote_test", draft.clone()).unwrap_err();
    assert!(bad.contains("登录失败") && !bad.contains("fixture-password"));
    assert!(
        request(&app, "remote_test", draft)
            .unwrap_err()
            .contains("API 端口而非控制台")
    );
    let mut config = s3(&endpoint);
    config["provider"]["bucket"] = json!("");
    config["provider"]["region"] = json!("");
    assert_eq!(
        request(
            &app,
            "remote_test",
            json!({"config":config,"secret":{"value":"fixture-password"}})
        )
        .unwrap()["scope"],
        "buckets"
    );
    let mut fixed = s3("http://127.0.0.1:1");
    fixed["provider"]["region"] = json!("");
    let fixed = request(
        &app,
        "remote_save",
        json!({"config":fixed,"secret":{"value":"fixture-password"}}),
    )
    .unwrap();
    assert!(
        request(
            &app,
            "remote_list",
            json!({"id":fixed["id"],"revision":1,"bucket":"another-bucket","path":""})
        )
        .unwrap_err()
        .contains("不属于此连接范围")
    );
    let db = rusqlite::Connection::open(app.data_dir.join("filemanager.sqlite")).unwrap();
    let stored: String = db
        .query_row("SELECT group_concat(value) FROM remote_profiles", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(!stored.contains("fixture-password"));
    serve.join().unwrap();
}

#[test]
fn connection_edits_and_restart_do_not_reveal_or_reuse_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let app = core(temp.path());
    let mut config = s3("http://127.0.0.1:1/");
    let secret = json!({ "value": "only-in-memory", "token": "only-in-memory-sts" });
    let saved = request(
        &app,
        "remote_save",
        json!({ "config": config, "secret": secret }),
    )
    .unwrap();
    assert_eq!(saved["needs_secret"], false);
    assert_eq!(saved["config"]["root"], "test");
    let id = &saved["id"];
    config["label"] = json!("Renamed");
    config["read_only"] = json!(true);
    let edited = request(
        &app,
        "remote_save",
        json!({ "id": id, "revision": 1, "config": config }),
    )
    .unwrap();
    assert_eq!(edited["revision"], 2);
    assert_eq!(edited["needs_secret"], false);
    let refs = |rev, path| json!({ "id": id, "revision": rev, "path": path });
    assert!(
        request(&app, "remote_delete", refs(1, "existing.txt"))
            .unwrap_err()
            .contains("已在其他窗口")
    );
    assert!(
        request(&app, "remote_delete", refs(2, "existing.txt"))
            .unwrap_err()
            .contains("只读")
    );
    for path in ["../escape/", "/absolute/", "a//b/", "a/./b/", "a\r\nb/"] {
        assert!(
            request(&app, "remote_list", refs(2, path))
                .unwrap_err()
                .contains("路径无效")
        );
    }

    config["endpoint"] = json!("http://127.0.0.1:2");
    for op in ["remote_test", "remote_save"] {
        assert!(
            request(
                &app,
                op,
                json!({ "id": id, "revision": 2, "config": config })
            )
            .unwrap_err()
            .contains("请输入")
        );
    }
    config["endpoint"] = json!("https://fixture-id:secret@example.test");
    assert!(
        request(
            &app,
            "remote_save",
            json!({ "config": config, "secret": secret })
        )
        .unwrap_err()
        .contains("只能包含协议")
    );

    let profiles = request(&app, "remote_profiles", json!({})).unwrap();
    assert_eq!(profiles, json!([edited]));
    let db = rusqlite::Connection::open(app.data_dir.join("filemanager.sqlite")).unwrap();
    let stored: String = db
        .query_row("SELECT value FROM remote_profiles", [], |r| r.get(0))
        .unwrap();
    assert!(!stored.contains("only-in-memory"));
    assert!(!profiles.to_string().contains("only-in-memory"));
    drop(app);

    let restarted = core(temp.path());
    let profiles = request(&restarted, "remote_profiles", json!({})).unwrap();
    assert_eq!(profiles[0]["needs_secret"], true);
    assert_eq!(profiles[0]["config"]["label"], "Renamed");
    assert!(
        request(&restarted, "remote_list", refs(2, ""))
            .unwrap_err()
            .contains("请输入")
    );
    request(
        &restarted,
        "remote_remove",
        json!({ "id": id, "revision": 2 }),
    )
    .unwrap();
    assert_eq!(
        request(&restarted, "remote_profiles", json!({})).unwrap(),
        json!([])
    );
}

// Send part of a GET, then hold the connection open until the test cancels the job.
struct SlowBody {
    first: Cursor<Vec<u8>>,
    reached: Option<mpsc::Sender<()>>,
    resume: mpsc::Receiver<()>,
    rest: Cursor<Vec<u8>>,
}

impl Read for SlowBody {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.first.read(buf)?;
        if n != 0 {
            return Ok(n);
        }
        if let Some(reached) = self.reached.take() {
            reached.send(()).unwrap();
            self.resume.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        self.rest.read(buf)
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name, value).unwrap()
}

fn next_request(server: &Server) -> tiny_http::Request {
    server
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .expect("expected another request from the storage client")
}

fn finish_job(app: &Core, id: &Value) -> Value {
    let start = Instant::now();
    loop {
        let jobs = request(app, "jobs", json!({})).unwrap();
        let job = jobs
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["id"] == *id)
            .unwrap();
        if !matches!(job["status"].as_str(), Some("queued" | "running")) {
            return job.clone();
        }
        assert!(start.elapsed() < Duration::from_secs(5), "{job}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn failed_and_cancelled_downloads_do_not_publish_or_leave_staging_files() {
    for cancel in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let app = core(temp.path());
        let output = temp.path().join("output");
        fs::create_dir(&output).unwrap();
        let server = Server::http("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", server.server_addr());
        let profile = request(
            &app,
            "remote_save",
            json!({ "config": s3(&endpoint), "secret": { "value": "fixture-secret" } }),
        )
        .unwrap();
        let (notify, downloading) = mpsc::channel();
        let (resume, wait) = mpsc::channel();
        let worker = thread::spawn(move || {
            let head = next_request(&server);
            assert_eq!(*head.method(), Method::Head);
            assert_eq!(head.url(), "/fixture-bucket/test/file.bin");
            assert!(head.headers().iter().any(|h| h.field.equiv("Authorization")
                && h.value.as_str().starts_with("AWS4-HMAC-SHA256 ")));
            head.respond(
                Response::empty(200)
                    .with_header(header("Content-Length", "131072"))
                    .with_header(header("ETag", "\"fixture\""))
                    .with_chunked_threshold(usize::MAX),
            )
            .unwrap();
            let get = next_request(&server);
            assert_eq!(*get.method(), Method::Get);
            assert!(
                get.headers()
                    .iter()
                    .any(|h| h.field.equiv("If-Match") && h.value.as_str() == "\"fixture\"")
            );
            if cancel {
                let body = SlowBody {
                    first: Cursor::new(vec![1; 65_536]),
                    reached: Some(notify),
                    resume: wait,
                    rest: Cursor::new(vec![2; 65_536]),
                };
                // The client may close after observing the cancellation.
                let _ = get.respond(
                    Response::new(StatusCode(200), vec![], body, Some(131_072), None)
                        .with_chunked_threshold(usize::MAX),
                );
            } else {
                get.respond(Response::from_data(b"short")).unwrap();
            }
        });
        let job = request(&app, "job_start", json!({
            "op": "remote_download",
            "args": { "id": profile["id"], "revision": 1, "path": "file.bin", "local_dir": output }
        })).unwrap();
        let id = &job["id"];
        if cancel {
            downloading.recv_timeout(Duration::from_secs(5)).unwrap();
            request(&app, "job_cancel", json!({ "id": id })).unwrap();
            resume.send(()).unwrap();
        }
        let status = finish_job(&app, id);
        assert_eq!(
            status["status"],
            if cancel { "cancelled" } else { "failed" },
            "{status}"
        );
        worker.join().unwrap();
        assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
    }
}

#[test]
fn cancelled_s3_upload_aborts_started_multipart_upload() {
    let temp = tempfile::tempdir().unwrap();
    let app = core(temp.path());
    let source = temp.path().join("large.bin");
    fs::File::create(&source)
        .unwrap()
        .set_len(17 * 1024 * 1024)
        .unwrap();
    let server = Server::http("127.0.0.1:0").unwrap();
    let profile = request(
        &app,
        "remote_save",
        json!({
            "config": s3(&format!("http://{}", server.server_addr())),
            "secret": { "value": "fixture-secret" }
        }),
    )
    .unwrap();
    let (notify, part_received) = mpsc::channel();
    let (resume, wait) = mpsc::channel();
    let worker = thread::spawn(move || {
        let path = "/fixture-bucket/test/large.bin";
        let head = next_request(&server);
        assert_eq!((head.method(), head.url()), (&Method::Head, path));
        head.respond(Response::empty(404)).unwrap();
        let start = next_request(&server);
        assert_eq!(*start.method(), Method::Post);
        assert!(start.url().starts_with(&format!("{path}?uploads")));
        start.respond(Response::from_string(
            "<InitiateMultipartUploadResult><UploadId>fixture-upload</UploadId></InitiateMultipartUploadResult>",
        )).unwrap();

        let mut part = next_request(&server);
        assert_eq!(*part.method(), Method::Put);
        assert!(part.url().contains("uploadId=fixture-upload"));
        assert_eq!(
            std::io::copy(part.as_reader(), &mut std::io::sink()).unwrap(),
            8 * 1024 * 1024
        );
        notify.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        part.respond(Response::empty(200).with_header(header("ETag", "\"part-1\"")))
            .unwrap();

        let abort = next_request(&server);
        assert_eq!(*abort.method(), Method::Delete);
        assert_eq!(abort.url(), format!("{path}?uploadId=fixture-upload"));
        abort.respond(Response::empty(204)).unwrap();
    });
    let job = request(
        &app,
        "job_start",
        json!({
            "op": "remote_upload",
            "args": { "id": profile["id"], "revision": 1, "path": "", "local_path": source }
        }),
    )
    .unwrap();
    part_received.recv_timeout(Duration::from_secs(5)).unwrap();
    let id = &job["id"];
    request(&app, "job_cancel", json!({ "id": id })).unwrap();
    resume.send(()).unwrap();
    worker.join().unwrap();
    let status = finish_job(&app, id);
    assert_eq!(status["status"], "cancelled", "{status}");
    assert_eq!(status["error"], "远程传输已取消");
    assert_eq!(fs::metadata(source).unwrap().len(), 17 * 1024 * 1024);
}

#[test]
fn cursor_pages_continue_the_remote_listing_and_are_bound_to_the_connection() {
    let temp = tempfile::tempdir().unwrap();
    let app = core(temp.path());
    let server = Server::http("127.0.0.1:0").unwrap();
    let config = s3(&format!("http://{}", server.server_addr()));
    let a = request(
        &app,
        "remote_save",
        json!({"config": config, "secret": {"value":"fixture-secret"}}),
    )
    .unwrap();
    let b = request(
        &app,
        "remote_save",
        json!({"config": config, "secret": {"value":"fixture-secret"}}),
    )
    .unwrap();
    let worker = thread::spawn(move || {
        let first = next_request(&server);
        assert_eq!(*first.method(), Method::Get);
        assert!(!first.url().contains("continuation-token="));
        first.respond(Response::from_string("<ListBucketResult><IsTruncated>true</IsTruncated><NextContinuationToken>page-two</NextContinuationToken><Contents><Key>test/a.txt</Key><Size>1</Size><LastModified>1970-01-01T00:00:00Z</LastModified></Contents><Contents><Key>test/b.txt</Key><Size>2</Size><LastModified>1970-01-01T00:00:00Z</LastModified></Contents></ListBucketResult>")).unwrap();
        let second = next_request(&server);
        assert!(
            second.url().contains("continuation-token=page-two"),
            "{}",
            second.url()
        );
        second.respond(Response::from_string("<ListBucketResult><IsTruncated>false</IsTruncated><Contents><Key>test/c.txt</Key><Size>3</Size><LastModified>1970-01-01T00:00:00Z</LastModified></Contents></ListBucketResult>")).unwrap();
    });
    let first = request(
        &app,
        "remote_list",
        json!({"id":a["id"],"revision":1,"path":"","page_size":1}),
    )
    .unwrap();
    assert_eq!(first["entries"][0]["name"], "a.txt");
    let token = &first["next_cursor"];
    for (id, path) in [(&b["id"], ""), (&a["id"], "other/")] {
        assert!(
            request(
                &app,
                "remote_list",
                json!({"id":id,"revision":1,"path":path,"cursor":token})
            )
            .unwrap_err()
            .contains("不属于")
        );
    }
    let second = request(
        &app,
        "remote_list",
        json!({"id":a["id"],"revision":1,"path":"","page_size":1,"cursor":token}),
    )
    .unwrap();
    assert_eq!(second["entries"][0]["name"], "b.txt");
    assert!(
        request(
            &app,
            "remote_list",
            json!({"id":a["id"],"revision":1,"path":"","cursor":token})
        )
        .unwrap_err()
        .contains("过期")
    );
    request(
        &app,
        "remote_list_close",
        json!({"id":a["id"],"cursor":second["next_cursor"]}),
    )
    .unwrap();
    assert!(
        request(
            &app,
            "remote_list",
            json!({"id":a["id"],"revision":1,"path":"","cursor":second["next_cursor"]})
        )
        .unwrap_err()
        .contains("过期")
    );
    worker.join().unwrap();
}

#[test]
fn batch_upload_reports_conflicts_and_continues_with_later_files() {
    let temp = tempfile::tempdir().unwrap();
    let app = core(temp.path());
    let old = temp.path().join("exists.txt");
    let fresh = temp.path().join("new.txt");
    fs::write(&old, b"must not overwrite").unwrap();
    fs::write(&fresh, b"new content").unwrap();
    let server = Server::http("127.0.0.1:0").unwrap();
    let profile = request(&app,"remote_save",json!({"config":s3(&format!("http://{}",server.server_addr())),"secret":{"value":"fixture-secret"}})).unwrap();
    let worker = thread::spawn(move || {
        let old = next_request(&server);
        assert_eq!(
            (old.method(), old.url()),
            (&Method::Head, "/fixture-bucket/test/exists.txt")
        );
        old.respond(Response::empty(200).with_header(header("Content-Length", "1")))
            .unwrap();
        let fresh = next_request(&server);
        assert_eq!(
            (fresh.method(), fresh.url()),
            (&Method::Head, "/fixture-bucket/test/new.txt")
        );
        fresh.respond(Response::empty(404)).unwrap();
        let mut put = next_request(&server);
        assert_eq!(*put.method(), Method::Put);
        assert!(
            put.headers()
                .iter()
                .any(|h| h.field.equiv("If-None-Match") && h.value.as_str() == "*")
        );
        let mut bytes = vec![];
        put.as_reader().read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"new content");
        put.respond(Response::empty(200).with_header(header("ETag", "\"new\"")))
            .unwrap();
    });
    let started=request(&app,"job_start",json!({"op":"remote_transfer","args":{"id":profile["id"],"revision":1,"direction":"upload","path":"","local_paths":[old,fresh]}})).unwrap();
    let job = finish_job(&app, &started["id"]);
    assert_eq!(job["status"], "failed");
    let result = request(&app, "jobs", json!({"result_id":started["id"]})).unwrap();
    let summary = &result
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"] == started["id"])
        .unwrap()["result"];
    assert_eq!(summary["completed"], 1);
    assert_eq!(summary["failed"], 1);
    assert_eq!(summary["pending"], 0);
    let report = request(
        &app,
        "remote_transfer_report",
        json!({"id":profile["id"],"report_id":summary["report_id"],"failed_only":true}),
    )
    .unwrap();
    assert_eq!(report["count"], 1);
    assert_eq!(report["items"][0]["path"], "exists.txt");
    worker.join().unwrap();
    assert_eq!(fs::read(old).unwrap(), b"must not overwrite");
}

#[test]
fn sync_preview_reads_metadata_only_and_pages_filtered_differences() {
    let temp = tempfile::tempdir().unwrap();
    let app = core(temp.path());
    let local = temp.path().join("local");
    fs::create_dir(&local).unwrap();
    fs::write(local.join("same.txt"), b"a").unwrap();
    fs::File::open(local.join("same.txt"))
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH)
        .unwrap();
    fs::write(local.join("local.txt"), b"local").unwrap();
    let server = Server::http("127.0.0.1:0").unwrap();
    let mut config = s3(&format!("http://{}", server.server_addr()));
    config["read_only"] = json!(true);
    let profile = request(
        &app,
        "remote_save",
        json!({"config":config,"secret":{"value":"fixture-secret"}}),
    )
    .unwrap();
    let worker = thread::spawn(move || {
        let list = next_request(&server);
        assert_eq!(*list.method(), Method::Get);
        assert!(list.url().contains("list-type=2"));
        list.respond(Response::from_string("<ListBucketResult><IsTruncated>false</IsTruncated><Contents><Key>test/same.txt</Key><Size>1</Size><LastModified>1970-01-01T00:00:00Z</LastModified></Contents><Contents><Key>test/remote.txt</Key><Size>2</Size><LastModified>1970-01-01T00:00:00Z</LastModified></Contents></ListBucketResult>")).unwrap();
    });
    let preview = request(
        &app,
        "remote_sync_preview",
        json!({"id":profile["id"],"revision":1,"path":"","direction":"upload","local_dir":local}),
    )
    .unwrap();
    assert_eq!(preview["counts"]["local_only"], 1);
    assert_eq!(preview["counts"]["remote_only"], 1);
    assert_eq!(preview["counts"]["unverified"], 1);
    let page=request(&app,"remote_sync_page",json!({"id":profile["id"],"revision":1,"plan_id":preview["plan_id"],"status":"local_only","query":"local"})).unwrap();
    assert_eq!(page["count"], 1);
    assert_eq!(page["entries"][0]["path"], "local.txt");
    assert!(request(&app,"remote_transfer",json!({"id":profile["id"],"revision":1,"direction":"upload","path":"","local_paths":[local.join("local.txt")]})).unwrap_err().contains("只读"));
    worker.join().unwrap();
    assert_eq!(fs::read(local.join("same.txt")).unwrap(), b"a");
}

#[test]
fn cancelled_batch_retains_completed_files_and_reports_every_unprocessed_item() {
    let temp = tempfile::tempdir().unwrap();
    let app = core(temp.path());
    let output = temp.path().join("output");
    fs::create_dir(&output).unwrap();
    let server = Server::http("127.0.0.1:0").unwrap();
    let profile = request(&app,"remote_save",json!({"config":s3(&format!("http://{}",server.server_addr())),"secret":{"value":"fixture-secret"}})).unwrap();
    let (notify, downloading) = mpsc::channel();
    let (resume, wait) = mpsc::channel();
    let worker = thread::spawn(move || {
        let first = next_request(&server);
        assert_eq!(
            (first.method(), first.url()),
            (&Method::Head, "/fixture-bucket/test/first.txt")
        );
        first
            .respond(
                Response::empty(200)
                    .with_header(header("Content-Length", "4"))
                    .with_chunked_threshold(usize::MAX),
            )
            .unwrap();
        next_request(&server)
            .respond(Response::from_data(b"keep"))
            .unwrap();
        let second = next_request(&server);
        assert_eq!(
            (second.method(), second.url()),
            (&Method::Head, "/fixture-bucket/test/second.bin")
        );
        second
            .respond(
                Response::empty(200)
                    .with_header(header("Content-Length", "131072"))
                    .with_chunked_threshold(usize::MAX),
            )
            .unwrap();
        let body = SlowBody {
            first: Cursor::new(vec![1; 65_536]),
            reached: Some(notify),
            resume: wait,
            rest: Cursor::new(vec![2; 65_536]),
        };
        let _ = next_request(&server).respond(
            Response::new(StatusCode(200), vec![], body, Some(131_072), None)
                .with_chunked_threshold(usize::MAX),
        );
    });
    let job=request(&app,"job_start",json!({"op":"remote_transfer","args":{"id":profile["id"],"revision":1,"direction":"download","paths":["first.txt","second.bin","third.txt"],"local_dir":output}})).unwrap();
    downloading.recv_timeout(Duration::from_secs(5)).unwrap();
    request(&app, "job_cancel", job.clone()).unwrap();
    resume.send(()).unwrap();
    assert_eq!(finish_job(&app, &job["id"])["status"], "cancelled");
    let results = request(&app, "jobs", json!({"result_id":job["id"]})).unwrap();
    let result = &results
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"] == job["id"])
        .unwrap()["result"];
    assert_eq!(result["completed"], 1);
    assert_eq!(result["failed"], 0);
    assert_eq!(result["pending"], 2);
    let report = request(
        &app,
        "remote_transfer_report",
        json!({"id":profile["id"],"report_id":result["report_id"]}),
    )
    .unwrap();
    assert_eq!(report["count"], 3);
    assert_eq!(report["items"][1]["status"], "unprocessed");
    assert_eq!(report["items"][2]["status"], "unprocessed");
    assert_eq!(fs::read(output.join("first.txt")).unwrap(), b"keep");
    assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
    worker.join().unwrap();
}
