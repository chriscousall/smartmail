use std::{fs::{self, OpenOptions}, io::Write, net::{TcpStream, ToSocketAddrs}, path::Path, time::Duration};

use chrono::Utc;
use imap::types::Flag;
use lettre::{message::Mailbox, transport::smtp::authentication::Credentials, Message, SmtpTransport, Transport};
use mailparse::{MailAddr, MailHeaderMap, ParsedMail};
use native_tls::{TlsConnector, TlsStream};
use serde::{Deserialize, Serialize};

use crate::db::{Account, SyncedMessage};

type ImapSession = imap::Session<TlsStream<TcpStream>>;
const NETWORK_TIMEOUT: Duration = Duration::from_secs(12);
pub const MAX_BODY_BYTES: u32 = 5 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub part_id: String,
    pub name: String,
    pub size: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageContent {
    pub body: String,
    pub html: String,
    pub attachments: Vec<Attachment>,
}

fn connect_tcp(host: &str, port: u16) -> Result<TcpStream, String> {
    let addresses = (host, port).to_socket_addrs().map_err(|_| "Could not resolve the mail server.".to_string())?;
    let mut last_error = String::from("Could not connect to the mail server.");
    for address in addresses {
        match TcpStream::connect_timeout(&address, NETWORK_TIMEOUT) {
            Ok(stream) => {
                stream.set_read_timeout(Some(NETWORK_TIMEOUT)).map_err(|error| error.to_string())?;
                stream.set_write_timeout(Some(NETWORK_TIMEOUT)).map_err(|error| error.to_string())?;
                return Ok(stream);
            }
            Err(error) => last_error = format!("Could not connect to the mail server: {error}"),
        }
    }
    Err(last_error)
}

pub fn connect_imap(host: &str, port: u16, username: &str, password: &str) -> Result<ImapSession, String> {
    let stream = connect_tcp(host, port)?;
    let connector = TlsConnector::new().map_err(|_| "Could not initialize TLS.".to_string())?;
    let tls = connector.connect(host, stream).map_err(|_| "IMAP TLS connection failed. Check the server name and certificate.".to_string())?;
    let mut client = imap::Client::new(tls);
    client.read_greeting().map_err(|_| "IMAP server greeting failed.".to_string())?;
    client.login(username, password).map_err(|(error, _)| format!("IMAP sign-in failed: {error}"))
}

fn smtp_transport(account: &Account, password: &str) -> Result<SmtpTransport, String> {
    let builder = match account.smtp_security.as_str() {
        "implicit_tls" => SmtpTransport::relay(&account.smtp_host),
        "starttls" => SmtpTransport::starttls_relay(&account.smtp_host),
        _ => return Err("Unsupported SMTP security setting.".into()),
    }.map_err(|error| format!("SMTP TLS setup failed: {error}"))?;
    Ok(builder
        .port(account.smtp_port)
        .credentials(Credentials::new(account.smtp_username.clone(), password.to_string()))
        .timeout(Some(NETWORK_TIMEOUT))
        .build())
}

pub fn test_smtp(account: &Account, password: &str) -> Result<(), String> {
    let transport = smtp_transport(account, password)?;
    if transport.test_connection().map_err(|error| format!("SMTP connection failed: {error}"))? {
        Ok(())
    } else {
        Err("SMTP server did not accept the connection.".into())
    }
}

fn first_address(headers: &[mailparse::MailHeader<'_>], key: &str) -> (String, String) {
    if let Some(header) = headers.get_first_header(key) {
        if let Ok(addresses) = mailparse::addrparse_header(header) {
            if let Some(MailAddr::Single(info)) = addresses.first() {
                return (info.display_name.clone().unwrap_or_else(|| info.addr.clone()), info.addr.clone());
            }
        }
    }
    let raw = headers.get_first_value(key).unwrap_or_default();
    (raw.clone(), raw)
}

pub fn sync_inbox(account: &Account, password: &str) -> Result<(u32, Vec<SyncedMessage>), String> {
    let mut session = connect_imap(&account.imap_host, account.imap_port, &account.imap_username, password)?;
    let mailbox = session.examine("INBOX").map_err(|error| format!("Could not open inbox: {error}"))?;
    let uidvalidity = mailbox.uid_validity.ok_or_else(|| "This IMAP server did not provide UIDVALIDITY.".to_string())?;
    if mailbox.exists == 0 { let _ = session.logout(); return Ok((uidvalidity, Vec::new())); }
    let first = mailbox.exists.saturating_sub(49).max(1);
    let sequence = format!("{first}:{}", mailbox.exists);
    let fetched = session.fetch(sequence, "(UID FLAGS RFC822.SIZE INTERNALDATE BODY.PEEK[HEADER])")
        .map_err(|error| format!("Could not read inbox headers: {error}"))?;
    let mut messages = Vec::new();
    for item in fetched.iter() {
        let Some(uid) = item.uid else { continue; };
        let Some(header_bytes) = item.header() else { continue; };
        if header_bytes.len() > 256 * 1024 { continue; }
        let Ok((headers, _)) = mailparse::parse_headers(header_bytes) else { continue; };
        let (sender, from) = first_address(&headers, "From");
        let (_, to) = first_address(&headers, "To");
        let subject = headers.get_first_value("Subject").unwrap_or_else(|| "(No subject)".into());
        let time = item.internal_date().map(|date| date.to_rfc3339()).unwrap_or_else(|| Utc::now().to_rfc3339());
        messages.push(SyncedMessage {
            uid, sender, from, to, subject, time,
            unread: !item.flags().contains(&Flag::Seen),
            starred: item.flags().contains(&Flag::Flagged),
            size: item.size.unwrap_or(0),
        });
    }
    drop(fetched);
    let _ = session.logout();
    Ok((uidvalidity, messages))
}

fn plain_text(part: &ParsedMail<'_>) -> Option<String> {
    if part.subparts.is_empty() && part.ctype.mimetype.eq_ignore_ascii_case("text/plain") {
        if is_attachment(part) { return None; }
        return part.get_body().ok();
    }
    for child in &part.subparts {
        if let Some(body) = plain_text(child) { return Some(body); }
    }
    None
}

fn is_attachment(part: &ParsedMail<'_>) -> bool {
    let disposition = part.get_content_disposition();
    matches!(disposition.disposition, mailparse::DispositionType::Attachment)
        || disposition.params.contains_key("filename")
        || part.ctype.params.contains_key("name")
}

fn html_source(part: &ParsedMail<'_>) -> Option<String> {
    if part.subparts.is_empty() && part.ctype.mimetype.eq_ignore_ascii_case("text/html") && !is_attachment(part) {
        return part.get_body().ok();
    }
    for child in &part.subparts {
        if let Some(body) = html_source(child) { return Some(body); }
    }
    None
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        result.push(TABLE[(a >> 2) as usize] as char);
        result.push(TABLE[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        result.push(if chunk.len() > 1 { TABLE[(((b & 15) << 2) | (c >> 6)) as usize] as char } else { '=' });
        result.push(if chunk.len() > 2 { TABLE[(c & 63) as usize] as char } else { '=' });
    }
    result
}

fn inline_images(part: &ParsedMail<'_>, html: &mut String) {
    if part.subparts.is_empty() {
        let mime = part.ctype.mimetype.to_ascii_lowercase();
        if matches!(mime.as_str(), "image/png" | "image/jpeg" | "image/gif" | "image/webp") {
            if let Some(content_id) = part.headers.get_first_value("Content-ID") {
                let id = content_id.trim().trim_start_matches('<').trim_end_matches('>');
                if !id.is_empty() && id.len() <= 200 {
                    if let Ok(bytes) = part.get_body_raw() {
                        if bytes.len() <= 2 * 1024 * 1024 {
                            let source = format!("cid:{id}");
                            if html.contains(&source) {
                                *html = html.replace(&source, &format!("data:{mime};base64,{}", base64_encode(&bytes)));
                            }
                        }
                    }
                }
            }
        }
        return;
    }
    for child in &part.subparts { inline_images(child, html); }
}

fn sanitize_html(html: &str) -> String {
    let document = dom_query::Document::from(html);
    document.select("script, iframe, frame, frameset, object, embed, form, input, button, textarea, select, meta, link, base, title, style, svg, math, audio, video, source, track, canvas").remove();
    let elements = document.select("*");
    for node in elements.nodes() {
        let tag = node.node_name().map(|name| name.to_ascii_lowercase()).unwrap_or_default();
        if !matches!(tag.as_str(), "html" | "head" | "body" | "div" | "span" | "p" | "br" | "table" | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th" | "img" | "a" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "b" | "strong" | "i" | "em" | "u" | "s" | "ul" | "ol" | "li" | "hr" | "blockquote" | "center" | "small" | "font" | "pre" | "code") {
            node.rename("span");
        }
        for attribute in node.attrs() {
            let name = attribute.name.local.to_string();
            if name == "src" && tag == "img" {
                let source = attribute.value.as_ref();
                if source.starts_with("//") {
                    node.set_attr("src", &format!("https:{source}"));
                } else if !source.starts_with("https://") && !source.starts_with("data:image/png;base64,") && !source.starts_with("data:image/jpeg;base64,") && !source.starts_with("data:image/gif;base64,") && !source.starts_with("data:image/webp;base64,") {
                    node.remove_attr("src");
                }
            } else if !matches!(name.as_str(), "style" | "class" | "id" | "width" | "height" | "align" | "valign" | "bgcolor" | "color" | "size" | "colspan" | "rowspan" | "alt" | "title" | "cellpadding" | "cellspacing" | "border" | "role" | "dir") {
                node.remove_attr(&name);
            }
        }
    }
    document.select("body").html().to_string()
}

fn decode_entity(entity: &str) -> Option<String> {
    if let Some(number) = entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")) {
        return u32::from_str_radix(number, 16).ok().and_then(char::from_u32).map(|ch| ch.to_string());
    }
    if let Some(number) = entity.strip_prefix('#') {
        return number.parse::<u32>().ok().and_then(char::from_u32).map(|ch| ch.to_string());
    }
    match entity {
        "amp" => Some("&".into()), "lt" => Some("<".into()), "gt" => Some(">".into()),
        "quot" => Some("\"".into()), "apos" | "#39" => Some("'".into()),
        "nbsp" => Some(" ".into()), "ndash" => Some("–".into()), "mdash" => Some("—".into()),
        "hellip" => Some("…".into()), "copy" => Some("©".into()), "reg" => Some("®".into()),
        _ => None,
    }
}

fn html_to_text(html: &str) -> String {
    let mut output = String::new();
    let mut rest = html;
    let mut hidden: Option<String> = None;
    while !rest.is_empty() && output.len() < 250_000 {
        if rest.starts_with('<') {
            let Some(end) = rest.find('>') else { break; };
            let tag = rest[1..end].trim().to_ascii_lowercase();
            let name = tag.trim_start_matches('/').split(|ch: char| ch.is_whitespace() || ch == '/').next().unwrap_or("");
            if hidden.is_none() && matches!(name, "script" | "style" | "head" | "svg") && !tag.starts_with('/') {
                hidden = Some(name.to_string());
            } else if hidden.as_deref() == Some(name) && tag.starts_with('/') {
                hidden = None;
            } else if hidden.is_none() && matches!(name, "br" | "p" | "div" | "li" | "tr" | "table" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote") {
                if !output.ends_with('\n') { output.push('\n'); }
            }
            rest = &rest[end + 1..];
        } else if rest.starts_with('&') {
            if let Some(end) = rest.find(';').filter(|end| *end <= 12) {
                if hidden.is_none() {
                    if let Some(decoded) = decode_entity(&rest[1..end]) { output.push_str(&decoded); rest = &rest[end + 1..]; continue; }
                }
            }
            if hidden.is_none() { output.push('&'); }
            rest = &rest[1..];
        } else {
            let next = rest.find(['<', '&']).unwrap_or(rest.len());
            let next = next.max(1);
            if hidden.is_none() { output.push_str(&rest[..next]); }
            rest = &rest[next..];
        }
    }
    output.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join("\n")
}

fn safe_filename(raw: &str, fallback: &str) -> String {
    let normalized = raw.replace('\\', "/");
    let leaf = Path::new(&normalized).file_name().and_then(|name| name.to_str()).unwrap_or("");
    let cleaned: String = leaf.chars().filter(|ch| !ch.is_control() && !"<>:\"/\\|?*".contains(*ch)).take(120).collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim_end_matches(' ');
    let stem = cleaned.split('.').next().unwrap_or("").to_ascii_uppercase();
    if cleaned.is_empty() || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9") {
        fallback.to_string()
    } else { cleaned.to_string() }
}

fn attachments(part: &ParsedMail<'_>, path: &str, output: &mut Vec<Attachment>) {
    if part.subparts.is_empty() {
        if is_attachment(part) {
            let disposition = part.get_content_disposition();
            let raw_name = disposition.params.get("filename").or_else(|| part.ctype.params.get("name")).map(String::as_str).unwrap_or("");
            let name = safe_filename(raw_name, &format!("attachment-{}", path.replace('.', "-")));
            let size = part.get_body_raw().map(|body| body.len()).unwrap_or(0);
            output.push(Attachment { part_id: path.to_string(), name, size });
        }
        return;
    }
    for (index, child) in part.subparts.iter().enumerate() {
        let child_path = if path.is_empty() { index.to_string() } else { format!("{path}.{index}") };
        attachments(child, &child_path, output);
    }
}

fn parse_content(raw: &[u8]) -> Result<MessageContent, String> {
    let parsed = mailparse::parse_mail(raw).map_err(|_| "This message could not be parsed safely.".to_string())?;
    let source = html_source(&parsed).unwrap_or_default();
    let body = plain_text(&parsed).or_else(|| (!source.is_empty()).then(|| html_to_text(&source))).unwrap_or_else(|| "This message has no readable text body.".into());
    let mut html: String = source.chars().take(500_000).collect();
    inline_images(&parsed, &mut html);
    if !html.is_empty() { html = sanitize_html(&html); }
    let mut found = Vec::new();
    attachments(&parsed, "", &mut found);
    Ok(MessageContent { body: body.chars().take(200_000).collect(), html, attachments: found })
}

fn fetch_raw(account: &Account, password: &str, uidvalidity: u32, uid: u32) -> Result<Vec<u8>, String> {
    let mut session = connect_imap(&account.imap_host, account.imap_port, &account.imap_username, password)?;
    let mailbox = session.examine("INBOX").map_err(|error| format!("Could not open inbox: {error}"))?;
    if mailbox.uid_validity != Some(uidvalidity) { return Err("The inbox changed on the server. Sync it again.".into()); }
    let fetched = session.uid_fetch(uid.to_string(), "BODY.PEEK[]").map_err(|error| format!("Could not fetch message: {error}"))?;
    let raw = fetched.iter().find(|item| item.uid == Some(uid)).and_then(|item| item.body())
        .ok_or_else(|| "Message was not found on the server.".to_string())?;
    if raw.len() > MAX_BODY_BYTES as usize { return Err("This message exceeds the current 5 MB preview limit.".into()); }
    let raw = raw.to_vec();
    drop(fetched);
    let _ = session.logout();
    Ok(raw)
}

pub fn fetch_content(account: &Account, password: &str, uidvalidity: u32, uid: u32) -> Result<MessageContent, String> {
    parse_content(&fetch_raw(account, password, uidvalidity, uid)?)
}

pub fn save_attachment(account: &Account, password: &str, uidvalidity: u32, uid: u32, part_id: &str, downloads: &Path) -> Result<String, String> {
    let raw = fetch_raw(account, password, uidvalidity, uid)?;
    let parsed = mailparse::parse_mail(&raw).map_err(|_| "This message could not be parsed safely.".to_string())?;
    let mut part = &parsed;
    for index in part_id.split('.').filter(|index| !index.is_empty()) {
        let index: usize = index.parse().map_err(|_| "Attachment was not found.".to_string())?;
        part = part.subparts.get(index).ok_or_else(|| "Attachment was not found.".to_string())?;
    }
    if !part.subparts.is_empty() || !is_attachment(part) { return Err("Attachment was not found.".into()); }
    let disposition = part.get_content_disposition();
    let raw_name = disposition.params.get("filename").or_else(|| part.ctype.params.get("name")).map(String::as_str).unwrap_or("");
    let name = safe_filename(raw_name, &format!("attachment-{}", part_id.replace('.', "-")));
    let bytes = part.get_body_raw().map_err(|_| "Could not decode this attachment.".to_string())?;
    fs::create_dir_all(downloads).map_err(|_| "Downloads folder is unavailable.".to_string())?;
    for attempt in 0..1000 {
        let candidate = if attempt == 0 { name.clone() } else {
            let path = Path::new(&name);
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("attachment");
            let extension = path.extension().and_then(|s| s.to_str()).map(|s| format!(".{s}")).unwrap_or_default();
            format!("{stem} ({attempt}){extension}")
        };
        let destination = downloads.join(&candidate);
        match OpenOptions::new().write(true).create_new(true).open(&destination) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(&bytes) {
                    drop(file);
                    let _ = fs::remove_file(&destination);
                    return Err(format!("Could not save attachment: {error}"));
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Could not save attachment: {error}")),
        }
    }
    Err("Too many files with this attachment name in Downloads.".into())
}

pub fn update_flags(account: &Account, password: &str, uidvalidity: u32, uid: u32, unread: bool, starred: bool) -> Result<(), String> {
    let mut session = connect_imap(&account.imap_host, account.imap_port, &account.imap_username, password)?;
    let mailbox = session.select("INBOX").map_err(|error| format!("Could not open inbox: {error}"))?;
    if mailbox.uid_validity != Some(uidvalidity) { return Err("The inbox changed on the server. Sync it again.".into()); }
    let uid_text = uid.to_string();
    session.uid_store(&uid_text, if unread { "-FLAGS.SILENT (\\Seen)" } else { "+FLAGS.SILENT (\\Seen)" })
        .map_err(|error| format!("Could not update read status: {error}"))?;
    session.uid_store(&uid_text, if starred { "+FLAGS.SILENT (\\Flagged)" } else { "-FLAGS.SILENT (\\Flagged)" })
        .map_err(|error| format!("Could not update star: {error}"))?;
    let _ = session.logout();
    Ok(())
}

pub fn send_plain_text(account: &Account, password: &str, recipient: &str, subject: &str, body: &str) -> Result<(), String> {
    let from: Mailbox = account.email.parse().map_err(|_| "The sending account has an invalid email address.".to_string())?;
    let to: Mailbox = recipient.parse().map_err(|_| "Enter a valid recipient email address.".to_string())?;
    let message = Message::builder().from(from).to(to).subject(subject).body(body.to_string())
        .map_err(|error| format!("Could not prepare message: {error}"))?;
    smtp_transport(account, password)?.send(&message)
        .map_err(|error| format!("Sending failed: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_text_without_rendering_html() {
        let raw = b"MIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain\r\n\r\nHello\r\n--x\r\nContent-Type: text/html\r\n\r\n<script>bad()</script>\r\n--x--\r\n";
        let parsed = mailparse::parse_mail(raw).unwrap();
        assert_eq!(plain_text(&parsed).unwrap().trim(), "Hello");
    }

    #[test]
    fn converts_html_only_mail_to_safe_text() {
        let raw = b"Content-Type: text/html; charset=utf-8\r\n\r\n<html><head><style>secret</style></head><body><h1>Hello &amp; welcome</h1><p>Read <b>this</b> message.</p><script>bad()</script></body></html>";
        let content = parse_content(raw).unwrap();
        assert_eq!(content.body, "Hello & welcome\nRead this message.");
        assert!(content.html.contains("<h1>Hello &amp; welcome</h1>"));
    }

    #[test]
    fn embeds_cid_images_without_network() {
        let raw = b"MIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=x\r\n\r\n--x\r\nContent-Type: text/html\r\n\r\n<p>Hello</p><img src=\"cid:logo\">\r\n--x\r\nContent-Type: image/png\r\nContent-ID: <logo>\r\nContent-Transfer-Encoding: base64\r\n\r\naGk=\r\n--x--\r\n";
        let content = parse_content(raw).unwrap();
        assert!(content.html.contains("data:image/png;base64,aGk="));
        assert!(!content.html.contains("cid:logo"));
    }

    #[test]
    fn strips_active_html_and_non_https_images() {
        let html = r#"<meta http-equiv="refresh" content="0;url=https://bad.example"><script>bad()</script><a href="javascript:bad()" onclick="bad()">Read</a><img src="http://bad.example/pixel"><img src="https://images.example/photo.png" onerror="bad()">"#;
        let safe = sanitize_html(html);
        assert!(!safe.contains("<script"));
        assert!(!safe.contains("refresh"));
        assert!(!safe.contains("javascript:"));
        assert!(!safe.contains("onclick"));
        assert!(!safe.contains("onerror"));
        assert!(!safe.contains("http://bad.example"));
        assert!(safe.contains("https://images.example/photo.png"));
    }

    #[test]
    fn lists_attachments_and_restricts_filenames() {
        let raw = b"MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain\r\n\r\nHello\r\n--x\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=../../bad.txt\r\nContent-Transfer-Encoding: base64\r\n\r\naGk=\r\n--x--\r\n";
        let content = parse_content(raw).unwrap();
        assert_eq!(content.body.trim(), "Hello");
        assert_eq!(content.attachments.len(), 1);
        assert_eq!(content.attachments[0].name, "bad.txt");
        assert_eq!(content.attachments[0].size, 2);
        assert_eq!(content.attachments[0].part_id, "1");
        assert_eq!(safe_filename("CON.txt", "fallback"), "fallback");
    }
}
