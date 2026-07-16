//! История распознаваний: локальный SQLite в каталоге данных приложения.
//! Диктовки бывают чувствительными: история отключается в настройках,
//! лимит записей поддерживается автоматически.

use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub id: i64,
    /// Unix-время в миллисекундах.
    pub ts: i64,
    pub raw_text: String,
    pub clean_text: Option<String>,
    /// Приложение-получатель вставки.
    pub app_name: Option<String>,
    pub duration_ms: Option<i64>,
    pub model: Option<String>,
    /// injected | left_in_clipboard | cancelled | error
    pub status: String,
}

pub struct History {
    conn: Mutex<Connection>,
}

impl History {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("открытие истории {}", path.display()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                raw_text TEXT NOT NULL,
                clean_text TEXT,
                app_name TEXT,
                duration_ms INTEGER,
                model TEXT,
                status TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_history_ts ON history(ts DESC);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                raw_text TEXT NOT NULL,
                clean_text TEXT,
                app_name TEXT,
                duration_ms INTEGER,
                model TEXT,
                status TEXT NOT NULL
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Добавляет запись и обрезает историю до лимита.
    pub fn insert(&self, entry: &HistoryEntry, limit: u32) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO history (ts, raw_text, clean_text, app_name, duration_ms, model, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.ts,
                entry.raw_text,
                entry.clean_text,
                entry.app_name,
                entry.duration_ms,
                entry.model,
                entry.status
            ],
        )?;
        conn.execute(
            "DELETE FROM history WHERE id NOT IN
             (SELECT id FROM history ORDER BY id DESC LIMIT ?1)",
            params![limit as i64],
        )?;
        Ok(())
    }

    pub fn list(&self, limit: u32) -> Result<Vec<HistoryEntry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, ts, raw_text, clean_text, app_name, duration_ms, model, status
             FROM history ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                ts: row.get(1)?,
                raw_text: row.get(2)?,
                clean_text: row.get(3)?,
                app_name: row.get(4)?,
                duration_ms: row.get(5)?,
                model: row.get(6)?,
                status: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn
            .lock()
            .execute("DELETE FROM history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        self.conn.lock().execute("DELETE FROM history", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str) -> HistoryEntry {
        HistoryEntry {
            id: 0,
            ts: 1,
            raw_text: text.into(),
            clean_text: Some(format!("{text} (чисто)")),
            app_name: Some("Telegram".into()),
            duration_ms: Some(1500),
            model: Some("large-v3-turbo".into()),
            status: "injected".into(),
        }
    }

    #[test]
    fn insert_list_roundtrip() {
        let h = History::open_in_memory().unwrap();
        h.insert(&entry("привет"), 100).unwrap();
        let items = h.list(10).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].raw_text, "привет");
        assert_eq!(items[0].status, "injected");
    }

    #[test]
    fn limit_prunes_old_entries() {
        let h = History::open_in_memory().unwrap();
        for i in 0..10 {
            h.insert(&entry(&format!("текст {i}")), 3).unwrap();
        }
        let items = h.list(100).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].raw_text, "текст 9");
    }

    #[test]
    fn clear_removes_all() {
        let h = History::open_in_memory().unwrap();
        h.insert(&entry("раз"), 100).unwrap();
        h.clear().unwrap();
        assert!(h.list(10).unwrap().is_empty());
    }
}
