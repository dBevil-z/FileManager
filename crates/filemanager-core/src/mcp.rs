use crate::{Core, Result, agents, err, operations, paths, process, str_arg};
use serde_json::{Value, json};
use std::{fs, path::Path};
use toml_edit::{DocumentMut, Item, Table, value};
fn jsonc(text: &str) -> Result<Value> {
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut string = false;
    let mut escape = false;
    while let Some(ch) = chars.next() {
        if string {
            clean.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                string = false;
            }
            continue;
        }
        if ch == '"' {
            string = true;
            clean.push(ch);
        } else if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for c in chars.by_ref() {
                if c == '\n' {
                    clean.push('\n');
                    break;
                }
            }
        } else if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            clean.push(' ');
            let mut closed = false;
            while let Some(c) = chars.next() {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    closed = true;
                    break;
                }
                if c == '\n' {
                    clean.push('\n');
                }
            }
            if !closed {
                return Err("JSONC 块注释未结束".into());
            }
        } else {
            clean.push(ch);
        }
    }
    let mut out = String::new();
    let ch: Vec<_> = clean.chars().collect();
    let mut string = false;
    let mut escape = false;
    for (i, c) in ch.iter().enumerate() {
        if string {
            out.push(*c);
            if escape {
                escape = false;
            } else if *c == '\\' {
                escape = true;
            } else if *c == '"' {
                string = false;
            }
            continue;
        }
        if *c == '"' {
            string = true;
        }
        if *c == ','
            && ch[i + 1..]
                .iter()
                .find(|c| !c.is_whitespace())
                .is_some_and(|c| *c == '}' || *c == ']')
        {
            continue;
        }
        out.push(*c);
    }
    serde_json::from_str(&out).map_err(err)
}
fn toml_json(t: &Item) -> Value {
    if let Some(v) = t.as_value() {
        if let Some(s) = v.as_str() {
            return json!(s);
        }
        if let Some(b) = v.as_bool() {
            return json!(b);
        }
        if let Some(n) = v.as_integer() {
            return json!(n);
        }
        if let Some(a) = v.as_array() {
            return json!(
                a.iter()
                    .map(|v| toml_json(&Item::Value(v.clone())))
                    .collect::<Vec<_>>()
            );
        }
        if let Some(t) = v.as_inline_table() {
            return Value::Object(
                t.iter()
                    .map(|(k, v)| (k.to_owned(), toml_json(&Item::Value(v.clone()))))
                    .collect(),
            );
        }
    }
    if let Some(t) = t.as_table() {
        return Value::Object(
            t.iter()
                .map(|(k, v)| (k.to_owned(), toml_json(v)))
                .collect(),
        );
    }
    Value::Null
}
pub fn parse(path: &Path, text: &str) -> Result<Value> {
    if path.extension().is_some_and(|s| s == "toml") {
        let doc = text.parse::<DocumentMut>().map_err(err)?;
        Ok(toml_json(
            &doc.get("mcp_servers").cloned().unwrap_or(Item::None),
        ))
    } else {
        let v = jsonc(text)?;
        Ok(v.get("mcpServers")
            .or(v.get("mcp"))
            .cloned()
            .unwrap_or(json!({})))
    }
}
fn mask(v: &mut Value) {
    if let Some(o) = v.as_object_mut() {
        for (k, v) in o {
            if matches!(k.as_str(), "env" | "headers") {
                if let Some(o) = v.as_object_mut() {
                    for (_, v) in o {
                        *v = json!("••••••");
                    }
                }
            } else if k.to_lowercase().contains("token")
                || k.to_lowercase().contains("secret")
                || k.to_lowercase().contains("password")
            {
                *v = json!("••••••");
            } else {
                mask(v);
            }
        }
    } else if let Some(a) = v.as_array_mut() {
        for v in a {
            mask(v);
        }
    }
}
pub fn list(core: &Core, args: &Value) -> Result<Value> {
    let projects = agents::projects(core, args)?;
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for project in projects["projects"].as_array().unwrap() {
        for config in project["configs"].as_array().unwrap() {
            let p = paths::decode(core, str_arg(config, "path")?)?;
            if !p.is_file()
                || !p.file_name().is_some_and(|n| {
                    let s = n.to_string_lossy();
                    s.contains("mcp")
                        || s == "config.toml"
                        || s == "settings.json"
                        || s.starts_with("opencode")
                        || s == ".claude.json"
                })
            {
                continue;
            }
            let text = match fs::read_to_string(&p) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(json!({"path":paths::encode(&p),"error":e.to_string()}));
                    continue;
                }
            };
            match parse(&p, &text) {
                Ok(v) => {
                    if let Some(o) = v.as_object() {
                        for (name, c) in o {
                            let mut config = c.clone();
                            let secret = c.get("env").is_some() || c.get("headers").is_some();
                            mask(&mut config);
                            let command = c["command"].as_str();
                            let installed = command.map(|s| {
                                Path::new(s).is_absolute() && Path::new(s).is_file()
                                    || process::which(s).is_some()
                            });
                            items.push(json!({"name":name,"path":paths::encode(&p),"tool":config_tool(&p),"project":project["name"],"config":config,"has_secrets":secret,"installed":installed,"revision":format!("{:x}",<sha2::Sha256 as sha2::Digest>::digest(text.as_bytes()))}));
                        }
                    }
                }
                Err(e) => errors.push(json!({"path":paths::encode(&p),"error":e})),
            }
        }
    }
    Ok(json!({"items":items,"errors":errors}))
}
fn config_tool(p: &Path) -> &'static str {
    let s = p.to_string_lossy();
    if s.contains(".codex") {
        "Codex"
    } else if s.contains(".cursor") {
        "Cursor"
    } else if s.contains(".gemini") {
        "Gemini"
    } else if s.contains("copilot") {
        "Copilot"
    } else if s.contains("opencode") {
        "OpenCode"
    } else {
        "Claude"
    }
}
fn to_toml(v: &Value) -> Result<Item> {
    Ok(match v {
        Value::String(s) => value(s),
        Value::Bool(b) => value(*b),
        Value::Number(n) => value(n.as_i64().ok_or("MCP 配置数字超出范围")?),
        Value::Array(a) => {
            let mut array = toml_edit::Array::new();
            for v in a {
                let item = to_toml(v)?;
                array.push(item.into_value().map_err(|_| "数组只支持基本类型")?);
            }
            value(array)
        }
        Value::Object(o) => {
            let mut t = Table::new();
            for (k, v) in o {
                t[k] = to_toml(v)?;
            }
            Item::Table(t)
        }
        Value::Null => return Err("MCP 配置不支持 null 值".into()),
    })
}
// Token boundaries keep edits away from comment bodies and escaped strings.
fn skip_trivia(bytes: &[u8], i: &mut usize) {
    while let Some(&c) = bytes.get(*i) {
        if c.is_ascii_whitespace() {
            *i += 1;
        } else if bytes.get(*i..*i + 2) == Some(b"//") {
            while *i < bytes.len() && bytes[*i] != b'\n' {
                *i += 1;
            }
        } else if bytes.get(*i..*i + 2) == Some(b"/*") {
            *i += 2;
            while *i < bytes.len() && bytes.get(*i..*i + 2) != Some(b"*/") {
                *i += 1;
            }
            *i = (*i + 2).min(bytes.len());
        } else {
            break;
        }
    }
}
fn token_end(bytes: &[u8], i: usize) -> Option<usize> {
    match *bytes.get(i)? {
        b'"' => {
            let mut j = i + 1;
            while let Some(&c) = bytes.get(j) {
                match c {
                    b'\\' => j += 2,
                    b'"' => return Some(j + 1),
                    _ => j += 1,
                }
            }
            None
        }
        b'{' | b'}' | b'[' | b']' | b':' | b',' => Some(i + 1),
        _ => {
            let mut j = i + 1;
            while let Some(&c) = bytes.get(j) {
                if c.is_ascii_whitespace() || b"{}[],:/".contains(&c) {
                    break;
                }
                j += 1;
            }
            Some(j)
        }
    }
}
fn value_end(bytes: &[u8], start: usize) -> Option<usize> {
    if !matches!(bytes.get(start)?, b'{' | b'[') {
        return token_end(bytes, start);
    }
    let mut depth = 0;
    let mut i = start;
    while let Some(&c) = bytes.get(i) {
        match c {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => (),
        }
        i = token_end(bytes, i)?;
        skip_trivia(bytes, &mut i);
    }
    None
}
fn property_span(text: &str, key: &str) -> Option<(usize, usize)> {
    let b = text.as_bytes();
    let mut i = 0;
    skip_trivia(b, &mut i);
    if b.get(i) != Some(&b'{') {
        return None;
    }
    i += 1;
    loop {
        skip_trivia(b, &mut i);
        let end = token_end(b, i)?;
        let name = serde_json::from_str::<String>(&text[i..end]).ok()?;
        i = end;
        skip_trivia(b, &mut i);
        if b.get(i) != Some(&b':') {
            return None;
        }
        i += 1;
        skip_trivia(b, &mut i);
        let end = value_end(b, i)?;
        if name == key {
            return Some((i, end));
        }
        i = end;
        skip_trivia(b, &mut i);
        if b.get(i) != Some(&b',') {
            return None;
        }
        i += 1;
    }
}
pub fn save(core: &Core, args: &Value) -> Result<Value> {
    let p = paths::decode(core, str_arg(args, "path")?)?;
    let name = str_arg(args, "name")?;
    if name.is_empty() || name.len() > 120 || name.chars().any(char::is_control) {
        return Err("MCP 名称无效".into());
    }
    let mut config = args["config"].clone();
    if !config.is_object() {
        return Err("请输入 MCP JSON 对象".into());
    }
    let text = match fs::read_to_string(&p) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && args["create"] == true => {
            if p.extension().is_some_and(|e| e == "toml") {
                String::new()
            } else {
                "{\n}\n".into()
            }
        }
        Err(e) => return Err(err(e)),
    };
    let existing = parse(&p, &text)?;
    restore_masked(&mut config, &existing[name])?;
    let mut intended = existing.clone();
    if intended.is_null() && p.extension().is_some_and(|e| e == "toml") {
        intended = json!({});
    }
    intended
        .as_object_mut()
        .ok_or("MCP 配置必须为对象")?
        .insert(name.to_owned(), config.clone());
    let next = if p.extension().is_some_and(|e| e == "toml") {
        let mut doc = text.parse::<DocumentMut>().map_err(err)?;
        if !doc.get("mcp_servers").is_some_and(Item::is_table) {
            doc["mcp_servers"] = Item::Table(Table::new());
        }
        doc["mcp_servers"][name] = to_toml(&config)?;
        doc.to_string()
    } else {
        let parsed = jsonc(&text)?;
        let field = if parsed.get("mcp").is_some() && parsed.get("mcpServers").is_none() {
            "mcp"
        } else {
            "mcpServers"
        };
        let mut expected_document = parsed;
        let root = expected_document
            .as_object_mut()
            .ok_or("配置必须为 JSON 对象")?;
        root.insert(field.to_owned(), intended.clone());
        let changed = if let Some((start, end)) = property_span(&text, field) {
            let original = &text[start..end];
            let content = serde_json::to_string_pretty(&config).map_err(err)?;
            let changed = if let Some((a, b)) = property_span(original, name) {
                format!("{}{}{}", &original[..a], content, &original[b..])
            } else {
                insert_property(original, name, &content)?
            };
            format!("{}{}{}", &text[..start], changed, &text[end..])
        } else {
            let content = serde_json::to_string_pretty(&json!({name:config})).map_err(err)?;
            insert_property(&text, field, &content)?
        };
        if jsonc(&changed)? != expected_document {
            return Err("无法在保留其余字段的情况下编辑此配置，原文件未修改".into());
        }
        changed
    };
    if parse(&p, &next)? != intended {
        return Err("MCP 配置未通过保存前校验，原文件未修改".into());
    }
    operations::write_text(
        core,
        &json!({"path":paths::encode(&p),"text":next,"revision":args["revision"]}),
    )
}

fn restore_masked(config: &mut Value, existing: &Value) -> Result<()> {
    if config.as_str() == Some("••••••") {
        if existing.is_null() {
            return Err("新配置中的隐藏密钥必须填写实际值".into());
        }
        *config = existing.clone();
    } else if let Some(object) = config.as_object_mut() {
        for (key, value) in object {
            restore_masked(value, &existing[key])?;
        }
    } else if let Some(array) = config.as_array_mut() {
        for (i, value) in array.iter_mut().enumerate() {
            restore_masked(value, &existing[i])?;
        }
    }
    Ok(())
}
fn insert_property(text: &str, name: &str, content: &str) -> Result<String> {
    let b = text.as_bytes();
    let mut first = 0;
    skip_trivia(b, &mut first);
    if b.get(first) != Some(&b'{') {
        return Err("配置必须为 JSON 对象".into());
    }
    let end = value_end(b, first).ok_or("配置对象未闭合")? - 1;
    let mut last = first + 1;
    let mut i = last;
    skip_trivia(b, &mut i);
    while i < end {
        last = token_end(b, i).ok_or("配置字段不完整")?;
        i = last;
        skip_trivia(b, &mut i);
    }
    let comma = if b[last - 1] == b'{' || b[last - 1] == b',' {
        ""
    } else {
        ","
    };
    Ok(format!(
        "{}{comma}{}\n  {}: {content}\n{}",
        &text[..last],
        &text[last..end],
        serde_json::to_string(name).map_err(err)?,
        &text[end..]
    ))
}
