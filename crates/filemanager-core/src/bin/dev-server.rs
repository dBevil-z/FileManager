use filemanager_core::{Core, media};
use serde_json::json;
use std::{
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tiny_http::{Header, Method, Response, Server, StatusCode};
fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k, v).unwrap()
}
fn serve(mut req: tiny_http::Request, core: &Core) {
    let origin = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Origin"))
        .map(|h| h.value.as_str());
    if origin.is_some_and(|v| !matches!(v, "http://127.0.0.1:1420" | "http://localhost:1420")) {
        let _ = req.respond(Response::empty(StatusCode(403)));
        return;
    }
    let url = req.url().to_owned();
    if req.method() == &Method::Get && url.starts_with("/api/media/") {
        let id = url
            .trim_start_matches("/api/media/")
            .split('?')
            .next()
            .unwrap_or("");
        let range = req
            .headers()
            .iter()
            .find(|h| h.field.equiv("Range"))
            .map(|h| h.value.as_str());
        let result = media::response(core, id, range);
        match result {
            Ok(r) => {
                let mut response = Response::from_data(r.body)
                    .with_status_code(r.status)
                    .with_header(header("Content-Type", r.mime))
                    .with_header(header("Accept-Ranges", "bytes"))
                    .with_header(header("X-Content-Type-Options", "nosniff"))
                    .with_header(header(
                        "Content-Security-Policy",
                        "default-src 'none'; sandbox;",
                    ));
                if r.status == 206 {
                    response.add_header(header("Content-Range", &r.range));
                }
                let _ = req.respond(response);
            }
            Err(e) => {
                let _ = req.respond(Response::from_string(e).with_status_code(404));
            }
        }
        return;
    }
    if req.method() != &Method::Post
        || url != "/api/request"
        || !req
            .headers()
            .iter()
            .any(|h| h.field.equiv("X-FileManager-Client") && h.value.as_str() == "local-ui")
    {
        let _ = req.respond(Response::empty(StatusCode(403)));
        return;
    }
    let mut body = Vec::new();
    let data = req
        .as_reader()
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())
        .and_then(|_| {
            if body.len() > 8 * 1024 * 1024 {
                return Err("请求过大".into());
            }
            serde_json::from_slice(&body).map_err(|e| e.to_string())
        })
        .and_then(|v| core.request(v));
    let (status, value) = match data {
        Ok(v) => (200, json!({"data":v})),
        Err(e) => (400, json!({"error":e})),
    };
    let _ = req.respond(
        Response::from_string(value.to_string())
            .with_status_code(status)
            .with_header(header("Content-Type", "application/json; charset=utf-8"))
            .with_header(header("Cache-Control", "no-store")),
    );
}
fn main() {
    let data = std::env::var_os("FILEMANAGER_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::data_local_dir().unwrap().join("filemanager"));
    let core = if let Some(home) = std::env::var_os("FILEMANAGER_TEST_HOME") {
        let home = PathBuf::from(home);
        Core::with_paths(data, home.clone(), home.join(".local/share/Trash"))
    } else {
        Core::new(data)
    }
    .expect("Initialize FileManager");
    let server = Server::http("127.0.0.1:1421").expect("Bind loopback development server");
    let (tx, rx) = std::sync::mpsc::sync_channel::<tiny_http::Request>(32);
    let rx = Arc::new(Mutex::new(rx));
    for _ in 0..6 {
        let rx = rx.clone();
        let core = core.clone();
        std::thread::spawn(move || {
            loop {
                let req = rx.lock().unwrap().recv();
                match req {
                    Ok(req) => serve(req, &core),
                    Err(_) => break,
                }
            }
        });
    }
    println!("FileManager core listening on http://127.0.0.1:1421 (development only)");
    for req in server.incoming_requests() {
        if tx.send(req).is_err() {
            break;
        }
    }
}
