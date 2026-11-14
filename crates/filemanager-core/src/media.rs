use crate::{Core, Result, err, preview};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
};
pub struct MediaResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub mime: &'static str,
    pub range: String,
    pub length: u64,
}
pub fn response(core: &Core, id: &str, range: Option<&str>) -> Result<MediaResponse> {
    let path = preview::resolve(core, id)?;
    let m = fs::metadata(&path).map_err(err)?;
    if !m.is_file() {
        return Err("预览不是普通文件".into());
    }
    let len = m.len();
    let mime = preview::mime(&path);
    let streaming = range.is_some() || len > 16 * 1024 * 1024;
    let (start, end) = if let Some(range) = range {
        let spec = range
            .strip_prefix("bytes=")
            .ok_or("无效的预览范围")?
            .split(',')
            .next()
            .unwrap_or("");
        let (a, b) = spec.split_once('-').ok_or("预览范围无效")?;
        let start = if a.is_empty() {
            len.saturating_sub(b.parse::<u64>().map_err(err)?)
        } else {
            a.parse::<u64>().map_err(err)?
        };
        let end = if a.is_empty() {
            len.saturating_sub(1)
        } else {
            b.parse::<u64>()
                .unwrap_or(len.saturating_sub(1))
                .min(len.saturating_sub(1))
        };
        if start >= len || end < start {
            return Err("预览范围超出文件长度".into());
        }
        (start, end.min(start + 4 * 1024 * 1024 - 1))
    } else {
        (
            0,
            if streaming {
                len.saturating_sub(1).min(4 * 1024 * 1024 - 1)
            } else {
                len.saturating_sub(1)
            },
        )
    };
    let mut f = fs::File::open(&path).map_err(err)?;
    f.seek(SeekFrom::Start(start)).map_err(err)?;
    let mut body = Vec::new();
    f.take(if len == 0 { 0 } else { end - start + 1 })
        .read_to_end(&mut body)
        .map_err(err)?;
    Ok(MediaResponse {
        status: if streaming { 206 } else { 200 },
        body,
        mime,
        range: format!("bytes {start}-{end}/{len}"),
        length: len,
    })
}
