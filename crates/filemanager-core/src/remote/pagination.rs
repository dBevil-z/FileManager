use super::{Backend, Entry, Listing, State, block, check_cancel, directory, profile};
use crate::{Core, Result, err, str_arg};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const TTL: Duration = Duration::from_secs(5 * 60);
const CAPACITY: usize = 16;

pub(super) enum Step {
    Entry(Entry),
    Skipped,
}

pub(super) fn entry(
    dir: &str,
    name: &str,
    is_dir: bool,
    size: Option<u64>,
    modified: Option<i64>,
) -> Step {
    let mut listing = Listing::default();
    listing.push(dir, name, is_dir, size, modified);
    listing.entries.pop().map_or(Step::Skipped, Step::Entry)
}

pub(super) enum Reader {
    Buckets(super::object_service::Buckets),
    Object(super::object::Reader),
    Sftp(super::sftp::Reader),
    Ftp(Box<super::ftp::Reader>),
}

impl Reader {
    pub async fn open(
        core: &Core,
        config: &profile::Config,
        secret: &profile::Secret,
        path: &str,
    ) -> Result<Self> {
        if config.lists_buckets() {
            if !path.is_empty() {
                return Err("请先选择 Bucket，再浏览桶内目录".into());
            }
            return Ok(Self::Buckets(
                super::object_service::Buckets::open(config.clone(), secret.clone()).await?,
            ));
        }
        match Backend::connect(core, config, secret).await? {
            Backend::Object(op) => Ok(Self::Object(super::object::Reader::open(op, path).await?)),
            Backend::Sftp(client) => Ok(Self::Sftp(client.reader(path)?)),
            Backend::Ftp(client) => Ok(Self::Ftp(Box::new(client.reader(path).await?))),
        }
    }

    pub async fn next(&mut self) -> Result<Option<Step>> {
        check_cancel()?;
        match self {
            Self::Buckets(reader) => reader.next().await,
            Self::Object(reader) => reader.next().await,
            Self::Sftp(reader) => reader.next(),
            Self::Ftp(reader) => reader.next().await,
        }
    }
}

struct Page {
    id: String,
    revision: u64,
    bucket: Option<String>,
    path: String,
    reader: Reader,
    pending: Option<Step>,
    updated: Instant,
}

#[derive(Default)]
pub(super) struct Pages(HashMap<String, Page>);

impl Pages {
    fn prune(&mut self) {
        self.0.retain(|_, p| p.updated.elapsed() < TTL);
        while self.0.len() >= CAPACITY {
            let Some(oldest) = self
                .0
                .iter()
                .min_by_key(|(_, p)| p.updated)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            self.0.remove(&oldest);
        }
    }
}

fn forget(state: &State, token: &str, id: &str) -> Result<()> {
    let mut pages = state.pages.lock().map_err(err)?;
    if pages.0.get(token).is_some_and(|p| p.id == id) {
        pages.0.remove(token);
    }
    Ok(())
}

pub(crate) fn close(core: &Core, args: &Value) -> Result<Value> {
    forget(&core.remote, str_arg(args, "cursor")?, str_arg(args, "id")?)?;
    Ok(json!({"ok": true}))
}

pub(super) fn list(core: &Core, args: &Value) -> Result<Value> {
    let path = directory(args)?;
    let id = str_arg(args, "id")?;
    let (config, secret) = profile::scoped(core, args, false)?;
    let bucket = config.bucket().map(str::to_owned);
    let revision = args["revision"].as_u64().ok_or("目录分页需要连接版本号")?;
    let limit = args["page_size"].as_u64().unwrap_or(100);
    if !(1..=500).contains(&limit) {
        return Err("每页条目数必须在 1 到 500 之间".into());
    }
    let cursor = args["cursor"].as_str().filter(|s| !s.is_empty());
    let mut page = if let Some(token) = cursor {
        let mut pages = core.remote.pages.lock().map_err(err)?;
        pages.0.retain(|_, p| p.updated.elapsed() < TTL);
        let p = pages.0.get(token).ok_or("目录游标已过期，请刷新目录")?;
        if p.id != id || p.revision != revision || p.path != path || p.bucket != bucket {
            return Err("目录游标不属于此连接或目录，请刷新目录".into());
        }
        pages.0.remove(token).unwrap()
    } else {
        Page {
            id: id.into(),
            revision,
            bucket,
            path: path.into(),
            reader: block(Reader::open(core, &config, &secret, path))?,
            pending: None,
            updated: Instant::now(),
        }
    };
    let result = block(async {
        let mut listing = Listing::default();
        for _ in 0..limit {
            let step = match page.pending.take() {
                Some(v) => Some(v),
                None => page.reader.next().await?,
            };
            match step {
                Some(Step::Entry(entry)) => listing.entries.push(entry),
                Some(Step::Skipped) => listing.skipped += 1,
                None => return Ok(listing),
            }
        }
        page.pending = page.reader.next().await?;
        listing.truncated = page.pending.is_some();
        Ok(listing)
    })?;
    let next = if result.truncated {
        let token = uuid::Uuid::new_v4().to_string();
        page.updated = Instant::now();
        let mut pages = core.remote.pages.lock().map_err(err)?;
        pages.prune();
        pages.0.insert(token.clone(), page);
        Some(token)
    } else {
        None
    };
    // Ordering is the server's ordering. Sorting each page separately would imply
    // a global sort that cannot be guaranteed by FTP or SFTP directory streams.
    Ok(
        json!({"entries": result.entries, "skipped": result.skipped, "truncated": result.truncated, "next_cursor": next}),
    )
}
