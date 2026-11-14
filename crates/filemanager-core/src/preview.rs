use crate::{Core, Result, err, files, paths, process, str_arg};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufReader, Read},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
pub fn register(core: &Core, path: PathBuf) -> Result<String> {
    let mut media = core.media.lock().map_err(err)?;
    if let Some((id, _)) = media.iter().find(|(_, p)| *p == &path) {
        return Ok(id.clone());
    }
    if media.len() > 12000 {
        media.clear();
    }
    let id = uuid::Uuid::new_v4().to_string();
    media.insert(id.clone(), path);
    Ok(id)
}
pub fn resolve(core: &Core, id: &str) -> Result<PathBuf> {
    core.media
        .lock()
        .map_err(err)?
        .get(id)
        .cloned()
        .ok_or_else(|| "预览已过期，请重新打开".into())
}
pub fn mime(p: &Path) -> &'static str {
    match p
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "pdf" => "application/pdf",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        _ => "application/octet-stream",
    }
}
fn image_preview(path: &Path, output: &Path, thumbnail: bool) -> Result<Value> {
    let mut reader = image::ImageReader::open(path)
        .map_err(err)?
        .with_guessed_format()
        .map_err(err)?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(96 * 1024 * 1024);
    limits.max_image_width = Some(12000);
    limits.max_image_height = Some(12000);
    reader.limits(limits);
    let mut img = reader.decode().map_err(err)?;
    if let Ok(file) = fs::File::open(path) {
        if let Ok(exif) = exif::Reader::new().read_from_container(&mut BufReader::new(file)) {
            if let Some(orientation) = exif
                .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                .and_then(|f| f.value.get_uint(0))
            {
                img = match orientation {
                    2 => img.fliph(),
                    3 => img.rotate180(),
                    4 => img.flipv(),
                    5 => img.rotate90().fliph(),
                    6 => img.rotate90(),
                    7 => img.rotate270().fliph(),
                    8 => img.rotate270(),
                    _ => img,
                };
            }
        }
    }
    let (w, h) = (img.width(), img.height());
    let rgb = if thumbnail {
        img.thumbnail(320, 240)
    } else {
        img.thumbnail(1000, 800)
    }
    .to_rgb8();
    let f = fs::File::create(output).map_err(err)?;
    image::codecs::jpeg::JpegEncoder::new_with_quality(f, 85)
        .encode_image(&image::DynamicImage::ImageRgb8(rgb))
        .map_err(err)?;
    Ok(json!({"width":w,"height":h}))
}
pub fn get(core: &Core, args: &Value) -> Result<Value> {
    // Decoders and converters allocate large temporary buffers. Serialize this work
    // across all windows, then return unused glibc pages rather than retaining arenas.
    let _gate = core.preview_gate.lock().map_err(err)?;
    let _reclaim = Reclaim;
    let thumbnail_only = args["thumbnail"] == true;
    let path = paths::decode(core, str_arg(args, "path")?)?;
    let e = files::entry(&path)?;
    if e.is_dir || e.target_is_dir {
        return files::folder_info(core, args);
    }
    let m = fs::metadata(&path).map_err(err)?;
    let page = args["page"].as_u64().unwrap_or(1).clamp(1, 100000);
    let key = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "v2:{}:{}:{}:{}:{}:{}:{}",
                paths::encode(&path),
                m.dev(),
                m.ino(),
                m.len(),
                m.mtime_nsec().wrapping_add(m.mtime() * 1_000_000_000),
                page,
                thumbnail_only
            )
            .as_bytes()
        )
    );
    let cache = core.data_dir.join("previews");
    fs::create_dir_all(&cache).map_err(err)?;
    let output = cache.join(format!("{key}.jpg"));
    let mut metadata = json!({});
    let mut warning = None::<String>;
    let kind = e.kind.as_str();
    let mut media = None;
    if matches!(kind, "image" | "video" | "audio" | "pdf") {
        media = Some(register(core, path.clone())?);
    }
    if e.extension == "svg" && !output.exists() {
        match svg_preview(&path, &output) {
            Ok(v) => metadata = v,
            Err(e) => warning = Some(e),
        }
        media = None;
    } else if kind == "image" && !output.exists() {
        if let Err(error) = image_preview(&path, &output, thumbnail_only).map(|v| metadata = v) {
            if e.extension == "svg" {
                warning = Some("SVG 通过系统应用打开，避免执行嵌入脚本".into());
                media = None;
            } else {
                warning = Some(error);
            }
        }
    } else if kind == "image" {
        if let Ok((w, h)) = image::image_dimensions(&path) {
            metadata = json!({"width":w,"height":h});
        }
    }
    if kind == "image" && !thumbnail_only {
        if let Ok(file) = fs::File::open(&path) {
            if let Ok(exif) = exif::Reader::new().read_from_container(&mut BufReader::new(file)) {
                for field in exif.fields().filter(|f| {
                    matches!(
                        f.tag,
                        exif::Tag::Make
                            | exif::Tag::Model
                            | exif::Tag::DateTimeOriginal
                            | exif::Tag::ExposureTime
                            | exif::Tag::FNumber
                            | exif::Tag::PhotographicSensitivity
                            | exif::Tag::FocalLength
                            | exif::Tag::LensModel
                            | exif::Tag::GPSLatitude
                            | exif::Tag::GPSLongitude
                    )
                }) {
                    metadata[field.tag.to_string()] =
                        json!(field.display_value().with_unit(&exif).to_string());
                }
            }
        }
    }
    if kind == "pdf" {
        if let Ok(s) = if thumbnail_only {
            Err("thumbnail".into())
        } else {
            process::run("pdfinfo", [path.as_os_str()], None, 5)
        } {
            for line in s.lines() {
                if let Some((k, v)) = line.split_once(':') {
                    if matches!(
                        k,
                        "Pages" | "Title" | "Author" | "Creator" | "Page size" | "CreationDate"
                    ) {
                        metadata[k] = json!(v.trim());
                    }
                }
            }
        }
        if !output.exists() {
            let prefix = cache.join(&key);
            let argv = vec![
                "-f".into(),
                page.to_string().into(),
                "-singlefile".into(),
                "-scale-to".into(),
                if thumbnail_only { "320" } else { "1000" }.into(),
                "-jpeg".into(),
                path.as_os_str().to_owned(),
                prefix.into_os_string(),
            ];
            if let Err(e) = process::run("pdftoppm", argv, None, 20) {
                warning = Some(e);
            }
        }
    }
    if matches!(kind, "video" | "audio") {
        if !thumbnail_only {
            if let Ok(s) = process::run(
                "ffprobe",
                [
                    std::ffi::OsStr::new("-v"),
                    "quiet".as_ref(),
                    "-print_format".as_ref(),
                    "json".as_ref(),
                    "-show_format".as_ref(),
                    "-show_streams".as_ref(),
                    path.as_os_str(),
                ],
                None,
                8,
            ) {
                metadata = serde_json::from_str(&s).unwrap_or(json!({}));
            }
        }
        if kind == "video" && !output.exists() {
            let argv = vec![
                "-v".into(),
                "error".into(),
                "-ss".into(),
                "1".into(),
                "-i".into(),
                path.as_os_str().to_owned(),
                "-frames:v".into(),
                "1".into(),
                "-threads".into(),
                "1".into(),
                "-filter_threads".into(),
                "1".into(),
                "-vf".into(),
                if thumbnail_only {
                    "scale=320:240:force_original_aspect_ratio=decrease"
                } else {
                    "scale=1000:800:force_original_aspect_ratio=decrease"
                }
                .into(),
                "-y".into(),
                output.as_os_str().to_owned(),
            ];
            if let Err(e) = process::run("ffmpeg", argv, None, 20) {
                warning = Some(e);
            }
        }
    }
    if matches!(e.extension.as_str(), "pages" | "numbers" | "key") && !output.exists() {
        if let Err(e) = iwork_preview(&path, &output) {
            warning = Some(e);
        }
    } else if !thumbnail_only
        && matches!(kind, "document" | "spreadsheet" | "presentation")
        && !output.exists()
    {
        let converted = cache.join(&key);
        fs::create_dir_all(&converted).map_err(err)?;
        let profile = converted.join("profile");
        let profile_url =
            url::Url::from_directory_path(&profile).map_err(|_| "无法创建文档预览目录")?;
        let argv = vec![
            format!("-env:UserInstallation={profile_url}").into(),
            "--headless".into(),
            "--convert-to".into(),
            "pdf".into(),
            "--outdir".into(),
            converted.as_os_str().to_owned(),
            path.as_os_str().to_owned(),
        ];
        match process::run("libreoffice", argv, None, 40) {
            Ok(_) => {
                let pdf = converted
                    .join(path.file_stem().unwrap_or_default())
                    .with_extension("pdf");
                if pdf.is_file() {
                    let prefix = cache.join(&key);
                    let argv = vec![
                        "-singlefile".into(),
                        "-scale-to".into(),
                        "1000".into(),
                        "-jpeg".into(),
                        pdf.as_os_str().to_owned(),
                        prefix.into_os_string(),
                    ];
                    if let Err(e) = process::run("pdftoppm", argv, None, 15) {
                        warning = Some(e);
                    }
                    media = Some(register(core, pdf)?);
                }
            }
            Err(e) => warning = Some(e),
        }
        let _ = fs::remove_dir_all(profile);
    }
    let text = if matches!(kind, "text" | "code" | "markdown") {
        let limit = if thumbnail_only { 2048 } else { 64 * 1024 };
        if !thumbnail_only && m.len() > limit {
            warning =
                Some("预览仅显示前 64 KiB；完整内容请打开文件查看，编辑保存不会截断原文。".into());
        }
        match if thumbnail_only {
            text_sample(&path, limit)
        } else if m.len() > limit {
            text_sample(&path, limit)
        } else {
            files::read_text(core, args)
        } {
            Ok(v) => Some(v),
            Err(e) => {
                warning = Some(e);
                None
            }
        }
    } else {
        None
    };
    let thumbnail = if output.is_file() {
        Some(register(core, output.clone())?)
    } else {
        None
    };
    if e.extension == "svg" {
        media = thumbnail.clone();
    }
    if let Err(e) = prune_cache(core, Some(&output)) {
        warning.get_or_insert(e);
    }
    let inline = if args["inline"] == true && output.is_file() {
        Some(format!(
            "data:image/jpeg;base64,{}",
            STANDARD.encode(fs::read(output).map_err(err)?)
        ))
    } else {
        None
    };
    Ok(
        json!({"entry":e,"kind":kind,"metadata":metadata,"thumbnail":thumbnail,"inline":inline,"media":media,"mime":mime(&path),"text":text,"warning":warning}),
    )
}
fn svg_preview(path: &Path, output: &Path) -> Result<Value> {
    if fs::metadata(path).map_err(err)?.len() > 16 * 1024 * 1024 {
        return Err("SVG 超过 16 MB 预览限制".into());
    }
    let mut options = resvg::usvg::Options::default();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options.fontdb_mut().load_system_fonts();
    let tree =
        resvg::usvg::Tree::from_data(&fs::read(path).map_err(err)?, &options).map_err(err)?;
    let size = tree.size();
    let scale = (1000.0 / size.width()).min(800.0 / size.height()).min(1.0);
    let mut image = resvg::tiny_skia::Pixmap::new(
        (size.width() * scale).ceil().max(1.0) as u32,
        (size.height() * scale).ceil().max(1.0) as u32,
    )
    .ok_or("SVG 尺寸无效")?;
    image.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut image.as_mut(),
    );
    let png = output.with_extension("png");
    image.save_png(&png).map_err(err)?;
    let result = image_preview(&png, output, false);
    let _ = fs::remove_file(png);
    result.map(|_| json!({"width":size.width(),"height":size.height()}))
}
fn iwork_preview(path: &Path, output: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).map_err(err)?).map_err(err)?;
    for name in [
        "preview.jpg",
        "QuickLook/Thumbnail.jpg",
        "QuickLook/Preview.jpg",
        "preview-micro.jpg",
        "QuickLook/Preview.pdf",
    ] {
        let Ok(file) = archive.by_name(name) else {
            continue;
        };
        if file.size() > 32 * 1024 * 1024 {
            continue;
        }
        let mut bytes = Vec::new();
        file.take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        if bytes.len() > 32 * 1024 * 1024 {
            continue;
        }
        let temp = output.with_extension(if name.ends_with("pdf") {
            "pdf"
        } else {
            "embedded.jpg"
        });
        fs::write(&temp, bytes).map_err(err)?;
        let result = if name.ends_with("pdf") {
            process::run(
                "pdftoppm",
                [
                    std::ffi::OsStr::new("-f"),
                    "1".as_ref(),
                    "-singlefile".as_ref(),
                    "-scale-to".as_ref(),
                    "1000".as_ref(),
                    "-jpeg".as_ref(),
                    temp.as_os_str(),
                    output.with_extension("").as_os_str(),
                ],
                None,
                15,
            )
            .map(|_| ())
        } else {
            image_preview(&temp, output, true).map(|_| ())
        };
        let _ = fs::remove_file(temp);
        return result;
    }
    Err("此 iWork 文件没有可读取的内嵌预览，可使用关联应用打开".into())
}
pub fn prune_cache(core: &Core, keep: Option<&Path>) -> Result<()> {
    let limit = core.get_value("settings")?["data"]["cacheLimitMB"]
        .as_u64()
        .unwrap_or(512)
        .clamp(32, 4096)
        * 1024
        * 1024;
    let mut entries = Vec::new();
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(core.data_dir.join("previews"))
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
    {
        if let Ok(m) = entry.metadata() {
            total += m.len();
            entries.push((m.modified().ok(), m.len(), entry.path().to_path_buf()));
        }
    }
    entries.sort_by_key(|e| e.0);
    for (_, size, path) in entries {
        if total <= limit {
            break;
        }
        if keep.is_some_and(|p| p == path) {
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}
pub fn markdown_image(core: &Core, args: &Value) -> Result<Value> {
    let document = paths::decode(core, str_arg(args, "document")?)?;
    let source = str_arg(args, "source")?;
    if source.contains("://") || source.starts_with("data:") {
        return Err("本地图片路径无效".into());
    }
    let decoded = paths::percent_decode(source.split(['?', '#']).next().unwrap_or(source))?;
    use std::os::unix::ffi::OsStringExt;
    let path = PathBuf::from(std::ffi::OsString::from_vec(decoded));
    let image = if path.is_absolute() {
        path
    } else {
        document.parent().ok_or("缺少文稿目录")?.join(path)
    };
    if !matches!(files::entry(&image)?.kind.as_str(), "image") {
        return Err("引用不是图片文件".into());
    }
    let preview = get(core, &json!({"path":paths::encode(&image)}))?;
    Ok(json!({"media":preview["thumbnail"].as_str().or(preview["media"].as_str())}))
}
pub fn clear_cache(core: &Core) -> Result<Value> {
    let _gate = core.preview_gate.lock().map_err(err)?;
    let dir = core.data_dir.join("previews");
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(err)?;
    }
    core.media.lock().map_err(err)?.clear();
    Ok(json!({"ok":true}))
}

fn text_sample(path: &Path, limit: u64) -> Result<Value> {
    let mut bytes = Vec::with_capacity(limit as usize);
    fs::File::open(path)
        .map_err(err)?
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.contains(&0) {
        return Err("此文件不是纯文本".into());
    }
    Ok(json!({"text":String::from_utf8_lossy(&bytes),"revision":""}))
}

struct Reclaim;
impl Drop for Reclaim {
    fn drop(&mut self) {
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        unsafe {
            libc::malloc_trim(0);
        }
    }
}
