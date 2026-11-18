//! SQLite access (single connection behind a mutex, used from blocking tasks).

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

const MIGRATIONS: &[&str] = &[
    // 1: initial schema
    r#"
CREATE TABLE users(
  id INTEGER PRIMARY KEY,
  username TEXT NOT NULL UNIQUE COLLATE NOCASE,
  password_hash TEXT NOT NULL,
  role TEXT NOT NULL,                -- owner | admin | user
  disabled INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE TABLE sessions(
  token_hash TEXT PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  csrf TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  ip TEXT,
  user_agent TEXT
);
CREATE TABLE api_tokens(
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  scopes TEXT NOT NULL,              -- JSON array: "admin", "push:<slug>", "build:<slug>"
  created_at INTEGER NOT NULL,
  last_used INTEGER,
  expires_at INTEGER
);
CREATE TABLE repos(
  id INTEGER PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  game TEXT NOT NULL,
  mode TEXT NOT NULL,                -- managed | external
  path TEXT,                         -- external folder
  settings TEXT NOT NULL,            -- JSON RepoSettings
  created_at INTEGER NOT NULL
);
CREATE TABLE repo_roles(
  repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role TEXT NOT NULL,                -- maintainer
  PRIMARY KEY(repo_id, user_id)
);
CREATE TABLE repo_credentials(
  id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  login TEXT NOT NULL,
  secret TEXT NOT NULL,              -- encrypted password
  embed INTEGER NOT NULL DEFAULT 0,  -- published in the legacy autoconfig
  note TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);
CREATE TABLE schedules(
  id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,                -- build | mirror
  cron TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  last_run INTEGER,
  next_run INTEGER
);
CREATE TABLE jobs(
  id INTEGER PRIMARY KEY,
  repo_id INTEGER REFERENCES repos(id) ON DELETE SET NULL,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,              -- queued | running | succeeded | failed | cancelled | interrupted
  created_at INTEGER NOT NULL,
  started_at INTEGER,
  finished_at INTEGER,
  user_id INTEGER,
  params TEXT NOT NULL DEFAULT '{}',
  summary TEXT,
  error TEXT,
  log TEXT NOT NULL DEFAULT ''
);
CREATE TABLE changesets(
  id INTEGER PRIMARY KEY,
  repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  status TEXT NOT NULL,              -- draft | publishing | published | discarded
  base_revision INTEGER,
  note TEXT NOT NULL DEFAULT '',
  ops TEXT NOT NULL DEFAULT '[]',
  created_by INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE uploads(
  id TEXT PRIMARY KEY,
  user_id INTEGER,
  length INTEGER NOT NULL,
  offset INTEGER NOT NULL DEFAULT 0,
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at INTEGER NOT NULL
);
CREATE TABLE audit_log(
  id INTEGER PRIMARY KEY,
  at INTEGER NOT NULL,
  user_id INTEGER,
  username TEXT,
  action TEXT NOT NULL,
  target TEXT,
  details TEXT,
  ip TEXT
);
CREATE TABLE stats_daily(
  day TEXT NOT NULL,
  repo_id INTEGER NOT NULL,
  bytes INTEGER NOT NULL DEFAULT 0,
  requests INTEGER NOT NULL DEFAULT 0,
  files INTEGER NOT NULL DEFAULT 0,
  clients INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(day, repo_id)
);
CREATE INDEX jobs_repo ON jobs(repo_id, id);
CREATE INDEX audit_at ON audit_log(at);
"#,
];

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; PRAGMA synchronous=NORMAL;")?;
        Self::migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        Self::migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn migrate(conn: &Connection) -> anyhow::Result<()> {
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, m) in MIGRATIONS.iter().enumerate().skip(v as usize) {
            conn.execute_batch(&format!(
                "BEGIN; {m} PRAGMA user_version={}; COMMIT;",
                i + 1
            ))?;
        }
        Ok(())
    }

    /// Run `f` on the connection in a blocking task.
    pub async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> anyhow::Result<T> {
        let c = self.conn.clone();
        Ok(tokio::task::spawn_blocking(move || {
            let mut g = c.lock().unwrap_or_else(|p| p.into_inner());
            f(&mut g)
        })
        .await??)
    }

    /// Synchronous access (startup, tests).
    pub fn with<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
    ) -> anyhow::Result<T> {
        let mut g = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        Ok(f(&mut g)?)
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
