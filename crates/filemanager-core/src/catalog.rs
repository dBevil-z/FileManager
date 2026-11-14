use crate::{Result, err, str_arg};
use serde_json::{Value, json};
use std::{io::Read, time::Duration};
pub fn metadata(args: &Value) -> Result<Value> {
    let url = url::Url::parse(str_arg(args, "url")?.trim()).map_err(err)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("请输入不含登录信息的 HTTP 或 HTTPS 链接".into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("FileManager/0.1 (+local bookmark manager)")
        .build()
        .map_err(err)?;
    let mut res = client
        .get(url.clone())
        .send()
        .map_err(err)?
        .error_for_status()
        .map_err(err)?;
    let final_url = res.url().clone();
    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if !content_type.contains("html") {
        return Ok(
            json!({"url":url.as_str(),"title":url.path_segments().and_then(|p|p.last()).filter(|s|!s.is_empty()).unwrap_or(url.host_str().unwrap_or("链接")),"site":url.host_str(),"description":"","image":null}),
        );
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut res)
        .take(2 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    let html = String::from_utf8_lossy(&bytes);
    let tag = regex::Regex::new(r"(?is)<meta\s+[^>]*>").unwrap();
    let attr =
        regex::Regex::new(r#"(?is)([\w:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#).unwrap();
    let mut meta = std::collections::HashMap::new();
    for v in tag.find_iter(&html) {
        let attrs: std::collections::HashMap<String, String> = attr
            .captures_iter(v.as_str())
            .map(|c| {
                (
                    c[1].to_lowercase(),
                    c.get(2)
                        .or(c.get(3))
                        .or(c.get(4))
                        .unwrap()
                        .as_str()
                        .to_owned(),
                )
            })
            .collect();
        if let (Some(name), Some(value)) = (
            attrs.get("property").or(attrs.get("name")),
            attrs.get("content"),
        ) {
            meta.insert(name.to_lowercase(), value.to_owned());
        }
    }
    fn unescape(s: &str) -> String {
        s.replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .trim()
            .to_owned()
    }
    let title = meta
        .get("og:title")
        .cloned()
        .or_else(|| {
            regex::Regex::new(r"(?is)<title[^>]*>(.*?)</title>")
                .unwrap()
                .captures(&html)
                .map(|c| c[1].to_owned())
        })
        .unwrap_or_else(|| url.host_str().unwrap_or("链接").into());
    let image = meta
        .get("og:image")
        .or(meta.get("twitter:image"))
        .and_then(|s| final_url.join(s).ok())
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .map(|u| u.to_string());
    Ok(
        json!({"url":url.as_str(),"title":unescape(&title).chars().take(350).collect::<String>(),"description":unescape(meta.get("og:description").or(meta.get("description")).map(String::as_str).unwrap_or("")).chars().take(1000).collect::<String>(),"site":url.host_str(),"image":image}),
    )
}
