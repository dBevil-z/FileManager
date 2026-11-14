use filemanager_core::{Core, media, preview};
use serde_json::json;
use std::{fs, io::Write};
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
#[test]
fn svg_is_rasterized_without_executing_scripts_or_serving_raw_xml() {
    let (t, c) = setup();
    let file = t.path().join("test.svg");
    fs::write(&file,r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="60"><script>throw new Error('never')</script><rect width="100" height="60" fill="purple"/><image href="file:///etc/passwd"/></svg>"#).unwrap();
    let v = c
        .request(json!({"op":"preview","args":{"path":file}}))
        .unwrap();
    assert!(v["thumbnail"].is_string());
    let response = media::response(&c, v["media"].as_str().unwrap(), None).unwrap();
    assert_eq!(response.mime, "image/jpeg");
    assert!(response.body.starts_with(&[0xff, 0xd8]));
}
#[test]
fn iwork_reads_embedded_preview_and_markdown_resolves_local_images() {
    let (t, c) = setup();
    let image = t.path().join("本地图.jpg");
    image::RgbImage::from_pixel(6, 4, image::Rgb([120, 80, 200]))
        .save(&image)
        .unwrap();
    let file = t.path().join("doc.pages");
    let mut zip = zip::ZipWriter::new(fs::File::create(&file).unwrap());
    zip.start_file("preview.jpg", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&fs::read(&image).unwrap()).unwrap();
    zip.finish().unwrap();
    let v = c
        .request(json!({"op":"preview","args":{"path":file}}))
        .unwrap();
    assert!(v["thumbnail"].is_string());
    let v=c.request(json!({"op":"markdown_image","args":{"document":t.path().join("README.md"),"source":"%E6%9C%AC%E5%9C%B0%E5%9B%BE.jpg"}})).unwrap();
    assert!(v["media"].is_string());
}
#[test]
fn preview_cache_limit_removes_old_files_and_keeps_the_current_preview() {
    let (t, c) = setup();
    let dir = c.data_dir.join("previews");
    fs::create_dir(&dir).unwrap();
    for name in ["old.jpg", "current.jpg"] {
        fs::File::create(dir.join(name))
            .unwrap()
            .set_len(20 * 1024 * 1024)
            .unwrap();
    }
    c.request(
        json!({"op":"catalog_save","args":{"namespace":"settings","data":{"cacheLimitMB":32}}}),
    )
    .unwrap();
    preview::prune_cache(&c, Some(&dir.join("current.jpg"))).unwrap();
    assert!(!dir.join("old.jpg").exists());
    assert!(dir.join("current.jpg").exists());
    assert!(t.path().exists());
}

#[test]
fn thumbnail_text_is_bounded_and_does_not_cache_the_full_document() {
    let (t, c) = setup();
    let path = t.path().join("large.txt");
    fs::write(&path, "abc\n".repeat(1024 * 1024)).unwrap();
    let small = c
        .request(json!({"op":"preview","args":{"path":path,"thumbnail":true}}))
        .unwrap();
    assert!(small["text"]["text"].as_str().unwrap().len() <= 2051);
    assert!(small["text"]["revision"].as_str().unwrap().is_empty());
    let editable = c
        .request(json!({"op":"read_text","args":{"path":path}}))
        .unwrap();
    assert_eq!(editable["text"].as_str().unwrap().len(), 4 * 1024 * 1024);
    let full = c
        .request(json!({"op":"preview","args":{"path":path}}))
        .unwrap();
    assert_eq!(full["text"]["text"].as_str().unwrap().len(), 64 * 1024);
}
#[test]
fn image_thumbnails_have_small_decoded_dimensions() {
    let (t, c) = setup();
    let path = t.path().join("large.jpg");
    image::RgbImage::from_pixel(1800, 1200, image::Rgb([90, 120, 150]))
        .save(&path)
        .unwrap();
    let small = c
        .request(json!({"op":"preview","args":{"path":path,"thumbnail":true}}))
        .unwrap();
    let resolved = preview::resolve(&c, small["thumbnail"].as_str().unwrap()).unwrap();
    let (w, h) = image::image_dimensions(resolved).unwrap();
    assert!(w <= 320 && h <= 240);
    let full = c
        .request(json!({"op":"preview","args":{"path":path}}))
        .unwrap();
    assert_ne!(small["thumbnail"], full["thumbnail"]);
}
