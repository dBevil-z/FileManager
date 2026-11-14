use crate::{Core, Result, err};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

impl Core {
    pub(crate) fn connection(&self) -> Result<Connection> {
        // Keep the old database in place: undo records can contain absolute backup paths.
        let legacy = self.data_dir.join("fileos.sqlite");
        let path = if legacy.exists() {
            legacy
        } else {
            self.data_dir.join("filemanager.sqlite")
        };
        let conn = Connection::open(path).map_err(err)?;
        conn.busy_timeout(std::time::Duration::from_secs(8))
            .map_err(err)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS kv (name TEXT PRIMARY KEY, value TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 1);
            CREATE TABLE IF NOT EXISTS journal (id TEXT PRIMARY KEY,label TEXT NOT NULL,payload TEXT NOT NULL,status TEXT NOT NULL,created INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS journal_time ON journal(created);
            CREATE TABLE IF NOT EXISTS observed (path TEXT PRIMARY KEY,first_seen INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS remote_profiles (id TEXT PRIMARY KEY, value TEXT NOT NULL);").map_err(err)?;
        Ok(conn)
    }
    pub(crate) fn get_value(&self, key: &str) -> Result<Value> {
        let conn = self.connection()?;
        let row: Option<(String, i64)> = conn
            .query_row("SELECT value,revision FROM kv WHERE name=?1", [key], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()
            .map_err(err)?;
        match row {
            Some((value, revision)) => Ok(
                json!({"data":serde_json::from_str::<Value>(&value).map_err(err)?,"revision":revision}),
            ),
            None => Ok(json!({"data":null,"revision":0})),
        }
    }
    pub(crate) fn put_value(
        &self,
        key: &str,
        value: &Value,
        expected: Option<i64>,
    ) -> Result<Value> {
        if key.len() > 80
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err("无效的存储名称".into());
        }
        let text = serde_json::to_string(value).map_err(err)?;
        if text.len() > 8 * 1024 * 1024 {
            return Err("本地设置过大".into());
        }
        let mut conn = self.connection()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(err)?;
        let current: i64 = tx
            .query_row("SELECT revision FROM kv WHERE name=?1", [key], |r| r.get(0))
            .optional()
            .map_err(err)?
            .unwrap_or(0);
        if expected.is_some_and(|v| v != current) {
            return Err("数据已在其他窗口改变，请刷新后重试".into());
        }
        let revision = current + 1;
        tx.execute("INSERT INTO kv(name,value,revision) VALUES(?1,?2,?3) ON CONFLICT(name) DO UPDATE SET value=excluded.value,revision=excluded.revision",params![key,text,revision]).map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(json!({"data":value,"revision":revision}))
    }
    pub(crate) fn catalog_get(&self, args: &Value) -> Result<Value> {
        self.get_value(args["namespace"].as_str().unwrap_or("catalog"))
    }
    pub(crate) fn catalog_save(&self, args: &Value) -> Result<Value> {
        let key = args["namespace"].as_str().unwrap_or("catalog");
        if !matches!(key, "catalog" | "settings" | "session" | "pricing") {
            return Err("此存储区域不可写".into());
        }
        let before = self.get_value(key)?;
        let after = self.put_value(key, &args["data"], args["revision"].as_i64())?;
        if key == "catalog" {
            let step = crate::operations::UndoStep::Metadata {
                namespace: key.into(),
                before: before["data"].clone(),
                after_revision: after["revision"].as_i64().unwrap_or(0),
                after_value: Some(args["data"].clone()),
            };
            let journal = crate::operations::Journal::begin(self, "更新工作空间与书签")?;
            journal.finish(vec![step])?;
        }
        Ok(after)
    }
    pub fn history(&self) -> Result<Value> {
        let conn = self.connection()?;
        let mut query=conn.prepare("SELECT id,label,status,created FROM journal ORDER BY created DESC,rowid DESC LIMIT 60").map_err(err)?;
        let rows=query.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"label":r.get::<_,String>(1)?,"status":r.get::<_,String>(2)?,"created":r.get::<_,i64>(3)?}))).map_err(err)?;
        Ok(json!({"items":rows.collect::<std::result::Result<Vec<_>,_>>().map_err(err)?}))
    }
}
