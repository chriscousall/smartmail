use std::{fs, path::PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use crate::mail::Attachment;

pub type DbResult<T> = Result<T, String>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub order: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub email: String,
    pub group_id: String,
    pub color: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: String,
    pub imap_username: String,
    pub smtp_username: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailMessage {
    pub id: String,
    pub account_id: String,
    pub uidvalidity: u32,
    pub uid: u32,
    pub sender: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub snippet: String,
    pub body: String,
    pub html: String,
    pub time: String,
    pub unread: bool,
    pub starred: bool,
    pub size: u32,
    pub body_loaded: bool,
    pub attachments: Vec<Attachment>,
}

#[derive(Clone, Debug)]
pub struct SyncedMessage {
    pub uid: u32,
    pub sender: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub time: String,
    pub unread: bool,
    pub starred: bool,
    pub size: u32,
}

pub fn database_path(app: &AppHandle) -> DbResult<PathBuf> {
    let directory = app.path().app_data_dir().map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("smartmail.sqlite3"))
}

pub fn open(app: &AppHandle) -> DbResult<Connection> {
    let path = database_path(app)?;
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    connection.busy_timeout(std::time::Duration::from_secs(5)).map_err(|error| error.to_string())?;
    connection.pragma_update(None, "foreign_keys", "ON").map_err(|error| error.to_string())?;
    Ok(connection)
}

pub fn initialize(app: &AppHandle) -> DbResult<()> {
    init_schema(&open(app)?)
}

pub fn init_schema(connection: &Connection) -> DbResult<()> {
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS groups (
           id TEXT PRIMARY KEY,
           name TEXT NOT NULL UNIQUE COLLATE NOCASE,
           sort_order INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS accounts (
           id TEXT PRIMARY KEY,
           name TEXT NOT NULL,
           email TEXT NOT NULL,
           group_id TEXT NOT NULL REFERENCES groups(id),
           color TEXT NOT NULL,
           imap_host TEXT NOT NULL,
           imap_port INTEGER NOT NULL,
           smtp_host TEXT NOT NULL,
           smtp_port INTEGER NOT NULL,
           smtp_security TEXT NOT NULL,
           imap_username TEXT NOT NULL,
           smtp_username TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS messages (
           rowid INTEGER PRIMARY KEY,
           account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
           mailbox TEXT NOT NULL DEFAULT 'INBOX',
           uidvalidity INTEGER NOT NULL,
           uid INTEGER NOT NULL,
           sender TEXT NOT NULL,
           from_address TEXT NOT NULL,
           to_address TEXT NOT NULL,
           subject TEXT NOT NULL,
           received_at TEXT NOT NULL,
           unread INTEGER NOT NULL,
           starred INTEGER NOT NULL,
           size INTEGER NOT NULL,
           body_text TEXT,
           UNIQUE(account_id, mailbox, uidvalidity, uid)
         );
         CREATE INDEX IF NOT EXISTS messages_account_time ON messages(account_id, received_at DESC);
         CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
           sender, from_address, to_address, subject, body_text,
           content='messages', content_rowid='rowid'
         );
         CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
           INSERT INTO messages_fts(rowid, sender, from_address, to_address, subject, body_text)
           VALUES (new.rowid, new.sender, new.from_address, new.to_address, new.subject, new.body_text);
         END;
         CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
           INSERT INTO messages_fts(messages_fts, rowid, sender, from_address, to_address, subject, body_text)
           VALUES ('delete', old.rowid, old.sender, old.from_address, old.to_address, old.subject, old.body_text);
         END;
         CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE OF sender, from_address, to_address, subject, body_text ON messages BEGIN
           INSERT INTO messages_fts(messages_fts, rowid, sender, from_address, to_address, subject, body_text)
           VALUES ('delete', old.rowid, old.sender, old.from_address, old.to_address, old.subject, old.body_text);
           INSERT INTO messages_fts(rowid, sender, from_address, to_address, subject, body_text)
           VALUES (new.rowid, new.sender, new.from_address, new.to_address, new.subject, new.body_text);
         END;",
    ).map_err(|error| error.to_string())?;
    let has_attachments: bool = connection.prepare("PRAGMA table_info(messages)").map_err(|error| error.to_string())?
        .query_map([], |row| row.get::<_, String>(1)).map_err(|error| error.to_string())?
        .any(|name| name.as_deref() == Ok("attachments_json"));
    if !has_attachments {
        connection.execute("ALTER TABLE messages ADD COLUMN attachments_json TEXT", []).map_err(|error| error.to_string())?;
    }
    let has_html: bool = connection.prepare("PRAGMA table_info(messages)").map_err(|error| error.to_string())?
        .query_map([], |row| row.get::<_, String>(1)).map_err(|error| error.to_string())?
        .any(|name| name.as_deref() == Ok("html_body"));
    if !has_html {
        connection.execute("ALTER TABLE messages ADD COLUMN html_body TEXT", []).map_err(|error| error.to_string())?;
    }
    let count: i64 = connection.query_row("SELECT COUNT(*) FROM groups", [], |row| row.get(0)).map_err(|error| error.to_string())?;
    if count == 0 {
        connection.execute("INSERT INTO groups (id, name, sort_order) VALUES ('personal', 'Personal', 0)", []).map_err(|error| error.to_string())?;
        connection.execute("INSERT INTO groups (id, name, sort_order) VALUES ('work', 'Work', 1)", []).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn list_groups(connection: &Connection) -> DbResult<Vec<Group>> {
    let mut statement = connection.prepare("SELECT id, name, sort_order FROM groups ORDER BY sort_order, name").map_err(|error| error.to_string())?;
    let groups = statement.query_map([], |row| Ok(Group { id: row.get(0)?, name: row.get(1)?, order: row.get(2)? }))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())?;
    Ok(groups)
}

pub fn create_group(connection: &Connection, id: &str, name: &str) -> DbResult<()> {
    let order: i64 = connection.query_row("SELECT COALESCE(MAX(sort_order), -1) + 1 FROM groups", [], |row| row.get(0)).map_err(|error| error.to_string())?;
    connection.execute("INSERT INTO groups (id, name, sort_order) VALUES (?1, ?2, ?3)", params![id, name, order]).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn rename_group(connection: &Connection, id: &str, name: &str) -> DbResult<()> {
    let updated = connection.execute("UPDATE groups SET name = ?1 WHERE id = ?2", params![name, id]).map_err(|error| error.to_string())?;
    if updated == 0 { return Err("Group not found.".into()); }
    Ok(())
}

pub fn delete_group(connection: &Connection, id: &str) -> DbResult<()> {
    let updated = connection.execute("DELETE FROM groups WHERE id = ?1", params![id]).map_err(|error| error.to_string())?;
    if updated == 0 { return Err("Group not found.".into()); }
    Ok(())
}

pub fn list_accounts(connection: &Connection) -> DbResult<Vec<Account>> {
    let mut statement = connection.prepare(
        "SELECT id, name, email, group_id, color, imap_host, imap_port, smtp_host, smtp_port, smtp_security, imap_username, smtp_username FROM accounts ORDER BY rowid"
    ).map_err(|error| error.to_string())?;
    statement.query_map([], |row| Ok(Account {
        id: row.get(0)?, name: row.get(1)?, email: row.get(2)?, group_id: row.get(3)?, color: row.get(4)?,
        imap_host: row.get(5)?, imap_port: row.get(6)?, smtp_host: row.get(7)?, smtp_port: row.get(8)?,
        smtp_security: row.get(9)?, imap_username: row.get(10)?, smtp_username: row.get(11)?,
    })).map_err(|error| error.to_string())?
      .collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())
}

pub fn get_account(connection: &Connection, id: &str) -> DbResult<Account> {
    list_accounts(connection)?.into_iter().find(|account| account.id == id).ok_or_else(|| "Account not found.".into())
}

pub fn insert_account(connection: &Connection, account: &Account) -> DbResult<()> {
    connection.execute(
        "INSERT INTO accounts (id, name, email, group_id, color, imap_host, imap_port, smtp_host, smtp_port, smtp_security, imap_username, smtp_username)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![account.id, account.name, account.email, account.group_id, account.color, account.imap_host,
                account.imap_port, account.smtp_host, account.smtp_port, account.smtp_security, account.imap_username, account.smtp_username],
    ).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn move_account(connection: &Connection, account_id: &str, group_id: &str) -> DbResult<()> {
    let updated = connection.execute("UPDATE accounts SET group_id = ?1 WHERE id = ?2", params![group_id, account_id]).map_err(|error| error.to_string())?;
    if updated == 0 { return Err("Account not found.".into()); }
    Ok(())
}

pub fn save_sync(connection: &mut Connection, account_id: &str, uidvalidity: u32, messages: &[SyncedMessage]) -> DbResult<()> {
    let transaction = connection.transaction().map_err(|error| error.to_string())?;
    transaction.execute("DELETE FROM messages WHERE account_id = ?1 AND mailbox = 'INBOX' AND uidvalidity <> ?2", params![account_id, uidvalidity]).map_err(|error| error.to_string())?;
    for message in messages {
        transaction.execute(
            "INSERT INTO messages (account_id, mailbox, uidvalidity, uid, sender, from_address, to_address, subject, received_at, unread, starred, size)
             VALUES (?1, 'INBOX', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(account_id, mailbox, uidvalidity, uid) DO UPDATE SET
               sender=excluded.sender, from_address=excluded.from_address, to_address=excluded.to_address,
               subject=excluded.subject, received_at=excluded.received_at, unread=excluded.unread,
               starred=excluded.starred, size=excluded.size",
            params![account_id, uidvalidity, message.uid, message.sender, message.from, message.to,
                    message.subject, message.time, message.unread, message.starred, message.size],
        ).map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

pub fn list_messages(connection: &Connection) -> DbResult<Vec<MailMessage>> {
    let mut statement = connection.prepare(
        "SELECT account_id, uidvalidity, uid, sender, from_address, to_address, subject, received_at, unread, starred, size, body_text, attachments_json, html_body
         FROM messages ORDER BY received_at DESC LIMIT 1000"
    ).map_err(|error| error.to_string())?;
    statement.query_map([], |row| {
        let account_id: String = row.get(0)?;
        let uidvalidity: u32 = row.get(1)?;
        let uid: u32 = row.get(2)?;
        let body: Option<String> = row.get(11)?;
        let attachment_json: Option<String> = row.get(12)?;
        let html: Option<String> = row.get(13)?;
        let snippet = body.as_deref().unwrap_or("").chars().take(120).collect();
        Ok(MailMessage {
            id: format!("{account_id}:INBOX:{uidvalidity}:{uid}"), account_id, uidvalidity, uid,
            sender: row.get(3)?, from: row.get(4)?, to: row.get(5)?, subject: row.get(6)?,
            time: row.get(7)?, unread: row.get(8)?, starred: row.get(9)?, size: row.get(10)?,
            snippet, body_loaded: body.is_some() && attachment_json.is_some() && html.is_some(), body: body.unwrap_or_default(),
            html: html.unwrap_or_default(),
            attachments: attachment_json.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default(),
        })
    }).map_err(|error| error.to_string())?
      .collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())
}

pub fn message_locator(connection: &Connection, account_id: &str, uidvalidity: u32, uid: u32) -> DbResult<(u32, Option<String>, Option<Vec<Attachment>>, Option<String>)> {
    connection.query_row(
        "SELECT size, body_text, attachments_json, html_body FROM messages WHERE account_id = ?1 AND mailbox = 'INBOX' AND uidvalidity = ?2 AND uid = ?3 LIMIT 1",
        params![account_id, uidvalidity, uid], |row| {
            let json: Option<String> = row.get(2)?;
            Ok((row.get(0)?, row.get(1)?, json.and_then(|value| serde_json::from_str(&value).ok()), row.get(3)?))
        },
    ).optional().map_err(|error| error.to_string())?.ok_or_else(|| "Message not found in this account.".into())
}

pub fn save_body(connection: &Connection, account_id: &str, uidvalidity: u32, uid: u32, body: &str, html: &str, attachments: &[Attachment]) -> DbResult<()> {
    let json = serde_json::to_string(attachments).map_err(|error| error.to_string())?;
    connection.execute(
        "UPDATE messages SET body_text = ?1, html_body = ?2, attachments_json = ?3 WHERE account_id = ?4 AND uidvalidity = ?5 AND uid = ?6",
        params![body, html, json, account_id, uidvalidity, uid],
    ).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn set_flags(connection: &Connection, account_id: &str, uidvalidity: u32, uid: u32, unread: bool, starred: bool) -> DbResult<()> {
    connection.execute("UPDATE messages SET unread = ?1, starred = ?2 WHERE account_id = ?3 AND uidvalidity = ?4 AND uid = ?5", params![unread, starred, account_id, uidvalidity, uid]).map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_and_account_membership_are_local() {
        let connection = Connection::open_in_memory().unwrap();
        init_schema(&connection).unwrap();
        assert_eq!(list_groups(&connection).unwrap().len(), 2);
        create_group(&connection, "project", "Project").unwrap();
        let account = Account {
            id: "a".into(), name: "Support".into(), email: "support@example.com".into(), group_id: "work".into(), color: "#abc".into(),
            imap_host: "imap.example.com".into(), imap_port: 993, smtp_host: "smtp.example.com".into(), smtp_port: 465,
            smtp_security: "implicit_tls".into(), imap_username: "support@example.com".into(), smtp_username: "support@example.com".into(),
        };
        insert_account(&connection, &account).unwrap();
        move_account(&connection, "a", "project").unwrap();
        assert_eq!(get_account(&connection, "a").unwrap().group_id, "project");
    }
}
