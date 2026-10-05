use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::{db::{self, Account, Group, MailMessage}, mail};

const KEYRING_IMAP: &str = "org.smartmail.desktop.imap";
const KEYRING_SMTP: &str = "org.smartmail.desktop.smtp";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    groups: Vec<Group>,
    accounts: Vec<Account>,
    messages: Vec<MailMessage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualAccountInput {
    name: String,
    email: String,
    group_id: String,
    imap_host: String,
    imap_port: u16,
    imap_username: String,
    imap_password: String,
    smtp_host: String,
    smtp_port: u16,
    smtp_security: String,
    smtp_username: String,
    smtp_password: String,
}

fn validate_name(name: &str, limit: usize, label: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > limit || name.chars().any(char::is_control) {
        return Err(format!("{label} must be 1 to {limit} characters."));
    }
    Ok(name.to_string())
}

fn validate_host(host: &str) -> Result<String, String> {
    let host = host.trim();
    if host.is_empty() || host.len() > 253 || !host.is_ascii() || host.starts_with('-') || host.ends_with('-') ||
       !host.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-') {
        return Err("Enter a server hostname without a scheme or path.".into());
    }
    Ok(host.to_string())
}

fn secret(service: &str, account_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(service, account_id).map_err(|_| "Windows credential storage is unavailable.".into())
}

fn read_secret(service: &str, account_id: &str) -> Result<String, String> {
    secret(service, account_id)?.get_password().map_err(|_| "Could not read the saved account credential.".into())
}

async fn on_worker<T, F>(work: F) -> Result<T, String>
where T: Send + 'static, F: FnOnce() -> Result<T, String> + Send + 'static {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|_| "Background task failed.".to_string())?
}

#[tauri::command]
pub async fn get_snapshot(app: AppHandle) -> Result<Snapshot, String> {
    on_worker(move || {
        let connection = db::open(&app)?;
        Ok(Snapshot {
            groups: db::list_groups(&connection)?,
            accounts: db::list_accounts(&connection)?,
            messages: db::list_messages(&connection)?,
        })
    }).await
}

#[tauri::command]
pub async fn create_group(app: AppHandle, name: String) -> Result<Group, String> {
    on_worker(move || {
        let name = validate_name(&name, 32, "Group name")?;
        let id = Uuid::new_v4().to_string();
        let connection = db::open(&app)?;
        db::create_group(&connection, &id, &name)?;
        db::list_groups(&connection)?.into_iter().find(|group| group.id == id).ok_or_else(|| "Group was not saved.".into())
    }).await
}

#[tauri::command]
pub async fn rename_group(app: AppHandle, group_id: String, name: String) -> Result<(), String> {
    on_worker(move || {
        let name = validate_name(&name, 32, "Group name")?;
        db::rename_group(&db::open(&app)?, &group_id, &name)
    }).await
}

#[tauri::command]
pub async fn delete_group(app: AppHandle, group_id: String) -> Result<(), String> {
    on_worker(move || db::delete_group(&db::open(&app)?, &group_id)).await
}

#[tauri::command]
pub async fn move_account(app: AppHandle, account_id: String, group_id: String) -> Result<(), String> {
    on_worker(move || db::move_account(&db::open(&app)?, &account_id, &group_id)).await
}

#[tauri::command]
pub async fn connect_manual_account(app: AppHandle, input: ManualAccountInput) -> Result<Account, String> {
    on_worker(move || {
        let email = input.email.trim().to_string();
        let _: lettre::Address = email.parse().map_err(|_| "Enter a valid account email address.".to_string())?;
        let imap_host = validate_host(&input.imap_host)?;
        let smtp_host = validate_host(&input.smtp_host)?;
        if input.imap_port == 0 || input.smtp_port == 0 { return Err("Enter valid mail server ports.".into()); }
        if !matches!(input.smtp_security.as_str(), "implicit_tls" | "starttls") { return Err("Choose a supported SMTP security setting.".into()); }
        if input.imap_password.is_empty() || input.imap_password.len() > 4096 { return Err("Enter an IMAP password or app password.".into()); }
        let smtp_password = if input.smtp_password.is_empty() { input.imap_password.clone() } else { input.smtp_password.clone() };
        if smtp_password.len() > 4096 { return Err("SMTP password is too long.".into()); }
        let account = Account {
            id: Uuid::new_v4().to_string(),
            name: validate_name(&input.name, 64, "Account name")?, email, group_id: input.group_id,
            color: "#7fb795".into(), imap_host, imap_port: input.imap_port,
            smtp_host, smtp_port: input.smtp_port, smtp_security: input.smtp_security,
            imap_username: validate_name(&input.imap_username, 256, "IMAP username")?,
            smtp_username: validate_name(&input.smtp_username, 256, "SMTP username")?,
        };
        let connection = db::open(&app)?;
        if !db::list_groups(&connection)?.iter().any(|group| group.id == account.group_id) { return Err("Choose an existing group.".into()); }
        let mut imap = mail::connect_imap(&account.imap_host, account.imap_port, &account.imap_username, &input.imap_password)?;
        imap.examine("INBOX").map_err(|error| format!("Connected, but could not open the inbox: {error}"))?;
        let _ = imap.logout();
        mail::test_smtp(&account, &smtp_password)?;
        let imap_entry = secret(KEYRING_IMAP, &account.id)?;
        let smtp_entry = secret(KEYRING_SMTP, &account.id)?;
        imap_entry.set_password(&input.imap_password).map_err(|_| "Could not save the IMAP credential.".to_string())?;
        if smtp_entry.set_password(&smtp_password).is_err() {
            let _ = imap_entry.delete_credential();
            return Err("Could not save the SMTP credential.".into());
        }
        if let Err(error) = db::insert_account(&connection, &account) {
            let _ = imap_entry.delete_credential();
            let _ = smtp_entry.delete_credential();
            return Err(error);
        }
        Ok(account)
    }).await
}

#[tauri::command]
pub async fn sync_account(app: AppHandle, account_id: String) -> Result<usize, String> {
    on_worker(move || {
        let mut connection = db::open(&app)?;
        let account = db::get_account(&connection, &account_id)?;
        let password = read_secret(KEYRING_IMAP, &account_id)?;
        let (uidvalidity, messages) = mail::sync_inbox(&account, &password)?;
        db::save_sync(&mut connection, &account_id, uidvalidity, &messages)?;
        Ok(messages.len())
    }).await
}

#[tauri::command]
pub async fn load_message_body(app: AppHandle, account_id: String, uidvalidity: u32, uid: u32) -> Result<mail::MessageContent, String> {
    on_worker(move || {
        let connection = db::open(&app)?;
        let (size, cached, cached_attachments, cached_html) = db::message_locator(&connection, &account_id, uidvalidity, uid)?;
        if let (Some(body), Some(attachments), Some(html)) = (cached, cached_attachments, cached_html) { return Ok(mail::MessageContent { body, html, attachments }); }
        if size > mail::MAX_BODY_BYTES { return Err("This message exceeds the current 5 MB preview limit.".into()); }
        let account = db::get_account(&connection, &account_id)?;
        let password = read_secret(KEYRING_IMAP, &account_id)?;
        let content = mail::fetch_content(&account, &password, uidvalidity, uid)?;
        db::save_body(&connection, &account_id, uidvalidity, uid, &content.body, &content.html, &content.attachments)?;
        Ok(content)
    }).await
}

#[tauri::command]
pub async fn save_attachment(app: AppHandle, account_id: String, uidvalidity: u32, uid: u32, part_id: String) -> Result<String, String> {
    on_worker(move || {
        if part_id.len() > 80 || !part_id.bytes().all(|byte| byte.is_ascii_digit() || byte == b'.') {
            return Err("Attachment was not found.".into());
        }
        let connection = db::open(&app)?;
        let (size, _, _, _) = db::message_locator(&connection, &account_id, uidvalidity, uid)?;
        if size > mail::MAX_BODY_BYTES { return Err("This message exceeds the current 5 MB preview limit.".into()); }
        let account = db::get_account(&connection, &account_id)?;
        let password = read_secret(KEYRING_IMAP, &account_id)?;
        let downloads = app.path().download_dir().map_err(|_| "Downloads folder is unavailable.".to_string())?;
        mail::save_attachment(&account, &password, uidvalidity, uid, &part_id, &downloads)
    }).await
}

#[tauri::command]
pub async fn set_message_flags(app: AppHandle, account_id: String, uidvalidity: u32, uid: u32, unread: bool, starred: bool) -> Result<(), String> {
    on_worker(move || {
        let connection = db::open(&app)?;
        db::message_locator(&connection, &account_id, uidvalidity, uid)?;
        let account = db::get_account(&connection, &account_id)?;
        let password = read_secret(KEYRING_IMAP, &account_id)?;
        mail::update_flags(&account, &password, uidvalidity, uid, unread, starred)?;
        db::set_flags(&connection, &account_id, uidvalidity, uid, unread, starred)
    }).await
}

#[tauri::command]
pub async fn send_plain_text(app: AppHandle, account_id: String, recipient: String, subject: String, body: String) -> Result<(), String> {
    on_worker(move || {
        if subject.chars().count() > 300 || body.chars().count() > 200_000 { return Err("Message is too long for this release.".into()); }
        let connection = db::open(&app)?;
        let account = db::get_account(&connection, &account_id)?;
        let password = read_secret(KEYRING_SMTP, &account_id)?;
        mail::send_plain_text(&account, &password, recipient.trim(), &subject, &body)
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_server_urls_instead_of_hostnames() {
        assert!(validate_host("https://imap.example.com").is_err());
        assert!(validate_host("imap.example.com").is_ok());
    }
}
