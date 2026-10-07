use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::Mutex;

#[derive(Debug, Serialize, Clone)]
pub struct Message {
    #[serde(rename = "message_id")]
    pub id: String,
    pub from: String,
    pub to: String,
    pub body: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_body: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replied_at: Option<String>,
}

pub struct Db {
    conn: Mutex<Connection>,
}

fn row_to_message(row: &rusqlite::Row) -> rusqlite::Result<Message> {
    Ok(Message {
        id: row.get(0)?,
        from: row.get(1)?,
        to: row.get(2)?,
        body: row.get(3)?,
        status: row.get(4)?,
        reply_body: row.get(5)?,
        created_at: row.get(6)?,
        replied_at: row.get(7)?,
    })
}

impl Db {
    pub fn open(path: &str) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                id          TEXT PRIMARY KEY,
                from_peer   TEXT NOT NULL,
                to_peer     TEXT NOT NULL,
                body        TEXT NOT NULL,
                status      TEXT NOT NULL DEFAULT 'pending',
                reply_body  TEXT,
                created_at  TEXT NOT NULL,
                replied_at  TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_to_status ON messages(to_peer, status);
            CREATE INDEX IF NOT EXISTS idx_created ON messages(created_at);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn send_message(&self, from: &str, to: &str, body: &str) -> anyhow::Result<Message> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (id, from_peer, to_peer, body, status, created_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
            params![id, from, to, body, created_at],
        )?;
        Ok(Message {
            id,
            from: from.into(),
            to: to.into(),
            body: body.into(),
            status: "pending".into(),
            reply_body: None,
            created_at,
            replied_at: None,
        })
    }

    pub fn check_messages(
        &self,
        for_peer: &str,
        status: Option<&str>,
    ) -> anyhow::Result<Vec<Message>> {
        let conn = self.conn.lock().unwrap();
        let status = status.unwrap_or("pending");
        if status == "all" {
            let mut stmt = conn.prepare(
                "SELECT id, from_peer, to_peer, body, status, reply_body, created_at, replied_at
                 FROM messages WHERE to_peer = ?1 ORDER BY created_at",
            )?;
            let msgs = stmt
                .query_map(params![for_peer], row_to_message)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(msgs)
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, from_peer, to_peer, body, status, reply_body, created_at, replied_at
                 FROM messages WHERE to_peer = ?1 AND status = ?2 ORDER BY created_at",
            )?;
            let msgs = stmt
                .query_map(params![for_peer, status], row_to_message)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(msgs)
        }
    }

    pub fn reply_to_message(
        &self,
        message_id: &str,
        from: &str,
        body: &str,
    ) -> anyhow::Result<Option<Message>> {
        let conn = self.conn.lock().unwrap();
        let replied_at = chrono::Utc::now().to_rfc3339();
        let updated = conn.execute(
            "UPDATE messages SET status = 'answered', reply_body = ?1, replied_at = ?2
             WHERE id = ?3 AND to_peer = ?4",
            params![body, replied_at, message_id, from],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        let mut stmt = conn.prepare(
            "SELECT id, from_peer, to_peer, body, status, reply_body, created_at, replied_at
             FROM messages WHERE id = ?1",
        )?;
        Ok(Some(stmt.query_row(params![message_id], row_to_message)?))
    }

    pub fn list_messages(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: i64,
    ) -> anyhow::Result<Vec<Message>> {
        let conn = self.conn.lock().unwrap();
        let cols =
            "id, from_peer, to_peer, body, status, reply_body, created_at, replied_at";
        let messages = match (from, to) {
            (Some(f), Some(t)) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {cols} FROM messages WHERE from_peer = ?1 AND to_peer = ?2
                     ORDER BY created_at DESC LIMIT ?3"
                ))?;
                let rows = stmt
                    .query_map(params![f, t, limit], row_to_message)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            }
            (Some(f), None) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {cols} FROM messages WHERE from_peer = ?1
                     ORDER BY created_at DESC LIMIT ?2"
                ))?;
                let rows = stmt
                    .query_map(params![f, limit], row_to_message)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            }
            (None, Some(t)) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {cols} FROM messages WHERE to_peer = ?1
                     ORDER BY created_at DESC LIMIT ?2"
                ))?;
                let rows = stmt
                    .query_map(params![t, limit], row_to_message)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            }
            (None, None) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {cols} FROM messages ORDER BY created_at DESC LIMIT ?1"
                ))?;
                let rows = stmt
                    .query_map(params![limit], row_to_message)?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            }
        };
        Ok(messages)
    }

    pub fn purge_messages(
        &self,
        older_than_hours: i64,
        status: Option<&str>,
    ) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let cutoff =
            (chrono::Utc::now() - chrono::Duration::hours(older_than_hours)).to_rfc3339();
        let count = match status {
            Some(s) => conn.execute(
                "DELETE FROM messages WHERE created_at < ?1 AND status = ?2",
                params![cutoff, s],
            )?,
            None => conn.execute(
                "DELETE FROM messages WHERE created_at < ?1",
                params![cutoff],
            )?,
        };
        Ok(count)
    }
}
