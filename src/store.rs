//! SQLite index. `symbols.norm` is the column the whole tool turns on:
//! cross-language lookup is `WHERE norm = ?`.

use crate::extract::Symbol;
use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Store {
    conn: Connection,
}

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS repos (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE,
  path       TEXT NOT NULL,
  indexed_at INTEGER
);

CREATE TABLE IF NOT EXISTS files (
  id           INTEGER PRIMARY KEY,
  repo_id      INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  path         TEXT NOT NULL,
  language     TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  UNIQUE(repo_id, path)
);

CREATE TABLE IF NOT EXISTS symbols (
  id         INTEGER PRIMARY KEY,
  file_id    INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  name       TEXT NOT NULL,
  norm       TEXT NOT NULL,
  kind       TEXT NOT NULL,
  parent     TEXT,
  start_line INTEGER NOT NULL,
  end_line   INTEGER NOT NULL,
  signature  TEXT
);

CREATE INDEX IF NOT EXISTS idx_symbols_norm   ON symbols(norm);
CREATE INDEX IF NOT EXISTS idx_symbols_name   ON symbols(name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_symbols_parent ON symbols(parent);
"#;

#[derive(Debug)]
pub struct Hit {
    pub repo: String,
    pub language: String,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub parent: Option<String>,
    pub start_line: i64,
    pub signature: String,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn upsert_repo(&self, name: &str, path: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO repos (name, path, indexed_at) VALUES (?1, ?2, unixepoch())
             ON CONFLICT(name) DO UPDATE SET path = ?2, indexed_at = unixepoch()",
            params![name, path],
        )?;
        let id = self
            .conn
            .query_row("SELECT id FROM repos WHERE name = ?1", params![name], |r| {
                r.get(0)
            })?;
        Ok(id)
    }

    /// Returns the stored hash for a file, if we have indexed it before.
    pub fn file_hash(&self, repo_id: i64, path: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT content_hash FROM files WHERE repo_id = ?1 AND path = ?2")?;
        let mut rows = stmt.query(params![repo_id, path])?;
        Ok(match rows.next()? {
            Some(row) => Some(row.get(0)?),
            None => None,
        })
    }

    /// Replace a file's symbols wholesale. Cheap because unchanged files never
    /// reach here — see the hash check in `index`.
    pub fn replace_file(
        &mut self,
        repo_id: i64,
        path: &str,
        language: &str,
        hash: &str,
        symbols: &[Symbol],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM files WHERE repo_id = ?1 AND path = ?2",
            params![repo_id, path],
        )?;
        tx.execute(
            "INSERT INTO files (repo_id, path, language, content_hash) VALUES (?1, ?2, ?3, ?4)",
            params![repo_id, path, language, hash],
        )?;
        let file_id = tx.last_insert_rowid();
        {
            let mut stmt = tx.prepare(
                "INSERT INTO symbols
                   (file_id, name, norm, kind, parent, start_line, end_line, signature)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for s in symbols {
                stmt.execute(params![
                    file_id,
                    s.name,
                    s.norm,
                    s.kind,
                    s.parent,
                    s.start_line as i64,
                    s.end_line as i64,
                    s.signature,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Cross-language lookup by normalized key.
    pub fn find_by_norm(&self, norm: &str) -> Result<Vec<Hit>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.name, f.language, f.path, s.name, s.kind, s.parent, s.start_line, s.signature
             FROM symbols s
             JOIN files f ON f.id = s.file_id
             JOIN repos r ON r.id = f.repo_id
             WHERE s.norm = ?1
             ORDER BY f.language, r.name, f.path",
        )?;
        let hits = stmt
            .query_map(params![norm], |row| {
                Ok(Hit {
                    repo: row.get(0)?,
                    language: row.get(1)?,
                    path: row.get(2)?,
                    name: row.get(3)?,
                    kind: row.get(4)?,
                    parent: row.get(5)?,
                    start_line: row.get(6)?,
                    signature: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(hits)
    }

    pub fn stats(&self) -> Result<(i64, i64, i64)> {
        let repos = self.conn.query_row("SELECT count(*) FROM repos", [], |r| r.get(0))?;
        let files = self.conn.query_row("SELECT count(*) FROM files", [], |r| r.get(0))?;
        let syms = self.conn.query_row("SELECT count(*) FROM symbols", [], |r| r.get(0))?;
        Ok((repos, files, syms))
    }
}
