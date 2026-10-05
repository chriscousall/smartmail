# SmartMail

SmartMail is a Windows-first, open-source desktop email client built for a calm interface and grouped accounts. The first working alpha uses Tauri, Rust, plain JavaScript/CSS, and SQLite. See [PLAN.md](PLAN.md), [SECURITY_PLAN.md](SECURITY_PLAN.md), and [TECH_STACK.md](TECH_STACK.md).

The interface uses the locally bundled [Montserrat](https://fonts.google.com/specimen/Montserrat) font under the [SIL Open Font License](assets/fonts/OFL.txt). It does not request fonts from Google at runtime.

## Working alpha

- Connect a manual IMAP and SMTP account using TLS and a password or app password.
- Keep credentials in Windows Credential Manager and account settings/mail cache in a local SQLite database.
- Organize multiple accounts into editable groups and view all, group, or individual inboxes.
- Sync the latest 50 inbox messages per account, read plain-text or isolated HTML mail, mark read/unread, star, and search locally cached messages.
- Display embedded images in HTML mail. Remote images load only after choosing **Load images** for that message.
- See attachment names and sizes, then explicitly save individual attachments to Downloads.
- Compose, reply, and submit plain-text messages to an SMTP server.

Gmail and Outlook provider sign-in (OAuth) is planned but not implemented. The manual connection form may work with providers that allow app passwords. The alpha has no attachment preview, Sent folder, background notifications, or automatic periodic sync. Message previews and attachment saving are limited to messages up to 5 MB. Search currently covers the latest 1,000 cached messages. SMTP setup checks TLS connectivity; credentials are verified when sending. Sending does not currently save a local Sent copy.

## Run

Install Node.js, pnpm, Rust, and Microsoft C++ Build Tools with the Windows SDK. Then:

```powershell
pnpm install
pnpm tauri dev
```

Build a standalone executable with `pnpm tauri build --no-bundle`. Run `pnpm test` and `cargo test --manifest-path src-tauri/Cargo.toml` for the current checks.

The desktop app starts with an empty inbox until an account is connected. Its `index.html`, `styles.css`, and `src/app.js` files are the production interface loaded by Tauri; they require the native runtime. The separate sample-data browser mockup has been removed.

## Local data

SmartMail creates `%APPDATA%\org.smartmail.desktop\smartmail.sqlite3` for account settings and cached messages. Windows Credential Manager stores the account passwords separately. The application does not need a user-selected working directory, and moving or replacing the EXE does not move this data. The alpha does not yet offer in-app cache removal or account disconnect; those controls are part of the security plan.

## Security and privacy

There is no SmartMail account or cloud service. IMAP uses TLS with certificate validation and SMTP uses implicit TLS or required STARTTLS. HTML mail opens in a sandboxed frame with scripts disabled; remote images require a per-message click. Mail content stays on the user's machine in SQLite except for requested remote image loads. The cache is not yet encrypted at rest; the Windows account and disk protection matter. Do not use this alpha with sensitive mail until the security plan is implemented and reviewed.
