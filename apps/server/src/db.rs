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
    // 2: detailed statistics (hourly counters, addons, client versions,
    // player sketches, monthly hash salt) and revision history.
    r#"
CREATE TABLE stats_hourly(
  hour INTEGER NOT NULL,             -- UTC epoch hour
  repo_id INTEGER NOT NULL,
  bytes INTEGER NOT NULL DEFAULT 0,
  bytes_meta INTEGER NOT NULL DEFAULT 0,
  bytes_sidecar INTEGER NOT NULL DEFAULT 0,
  requests INTEGER NOT NULL DEFAULT 0,
  files INTEGER NOT NULL DEFAULT 0,
  ranges INTEGER NOT NULL DEFAULT 0,
  heads INTEGER NOT NULL DEFAULT 0,
  checks INTEGER NOT NULL DEFAULT 0,
  s2xx INTEGER NOT NULL DEFAULT 0,
  s206 INTEGER NOT NULL DEFAULT 0,
  s3xx INTEGER NOT NULL DEFAULT 0,
  s401 INTEGER NOT NULL DEFAULT 0,
  s404 INTEGER NOT NULL DEFAULT 0,
  s416 INTEGER NOT NULL DEFAULT 0,
  s4xx INTEGER NOT NULL DEFAULT 0,
  s503 INTEGER NOT NULL DEFAULT 0,
  s5xx INTEGER NOT NULL DEFAULT 0,
  completed INTEGER NOT NULL DEFAULT 0,
  aborted INTEGER NOT NULL DEFAULT 0,
  xfer_ms INTEGER NOT NULL DEFAULT 0,
  wait_ms INTEGER NOT NULL DEFAULT 0,
  c_axs INTEGER NOT NULL DEFAULT 0,
  c_a3s INTEGER NOT NULL DEFAULT 0,
  c_browser INTEGER NOT NULL DEFAULT 0,
  c_other INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(hour, repo_id)
);
ALTER TABLE stats_daily ADD COLUMN bytes_meta INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN bytes_sidecar INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN ranges INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN heads INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN checks INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN s2xx INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN s206 INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN s3xx INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN s401 INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN s404 INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN s416 INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN s4xx INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN s503 INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN s5xx INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN completed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN aborted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN xfer_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN wait_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN c_axs INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN c_a3s INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN c_browser INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN c_other INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stats_daily ADD COLUMN players BLOB;   -- HyperLogLog sketch; repo_id 0 = whole server
CREATE TABLE stats_addons(
  day TEXT NOT NULL,
  repo_id INTEGER NOT NULL,
  addon TEXT NOT NULL,
  bytes INTEGER NOT NULL DEFAULT 0,
  downloads INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(day, repo_id, addon)
);
CREATE TABLE stats_versions(
  day TEXT NOT NULL,
  family TEXT NOT NULL,
  version TEXT NOT NULL,
  requests INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(day, family, version)
);
CREATE TABLE stats_salt(month TEXT PRIMARY KEY, salt BLOB NOT NULL);
CREATE TABLE revision_history(
  repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  revision INTEGER NOT NULL,
  at INTEGER NOT NULL,
  kind TEXT NOT NULL,
  files INTEGER,
  total_size INTEGER,
  new_addons INTEGER,
  updated_addons INTEGER,
  deleted_addons INTEGER,
  duration_ms INTEGER,
  job_id INTEGER,
  PRIMARY KEY(repo_id, revision)
);
INSERT OR REPLACE INTO revision_history
SELECT repo_id, json_extract(summary, '$.revision'), COALESCE(finished_at, created_at), kind,
       json_extract(summary, '$.files'), json_extract(summary, '$.total_size'),
       json_array_length(summary, '$.new_addons'), json_array_length(summary, '$.updated_addons'),
       json_array_length(summary, '$.deleted_addons'), json_extract(summary, '$.duration_ms'), id
FROM jobs
WHERE status = 'succeeded' AND repo_id IS NOT NULL AND kind IN ('build', 'publish', 'mirror')
  AND json_valid(summary) AND json_extract(summary, '$.revision') IS NOT NULL
ORDER BY id;
CREATE INDEX stats_daily_day ON stats_daily(day);
CREATE INDEX jobs_finished ON jobs(finished_at);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Upgrading an existing database fills revision history from past jobs.
    #[test]
    fn upgrade_backfills_revision_history() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "BEGIN; {} PRAGMA user_version=1; COMMIT;",
            MIGRATIONS[0]
        ))
        .unwrap();
        conn.execute_batch(
            r#"
INSERT INTO repos(id, slug, name, game, mode, settings, created_at) VALUES (1, 'main', 'Main', 'arma3', 'managed', '{}', 0);
INSERT INTO stats_daily(day, repo_id, bytes, requests, files) VALUES ('2026-01-01', 1, 10, 2, 1);
INSERT INTO jobs(id, repo_id, kind, status, created_at, finished_at, summary) VALUES
  (1, 1, 'publish', 'succeeded', 0, 1000, '{"revision":3,"files":10,"total_size":500,"new_addons":["@a"],"updated_addons":[],"deleted_addons":[],"duration_ms":42}'),
  (2, 1, 'publish', 'failed', 0, 2000, NULL),
  (3, 1, 'mirror', 'succeeded', 0, 3000, '{"changed":false,"remote_revision":7}'),
  (4, 1, 'build', 'succeeded', 0, 4000, '{"revision":3,"files":11,"total_size":600,"new_addons":[],"updated_addons":["@a"],"deleted_addons":[],"duration_ms":7}'),
  (5, NULL, 'gc', 'succeeded', 0, 5000, '{"removed":1}');
"#,
        )
        .unwrap();
        Db::migrate(&conn).unwrap();
        let rows: Vec<(i64, i64, i64, i64, i64)> = conn
            .prepare("SELECT revision, files, total_size, updated_addons, job_id FROM revision_history ORDER BY revision")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        // The later job for the same revision wins; jobs without a revision are skipped.
        assert_eq!(rows, vec![(3, 11, 600, 1, 4)]);
        // Old daily rows keep their numbers; new columns default to 0.
        let (b, s): (i64, i64) = conn
            .query_row(
                "SELECT bytes, s404 FROM stats_daily WHERE repo_id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((b, s), (10, 0));
    }
}
