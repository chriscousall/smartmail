# SmartMail: product plan

## Product idea

SmartMail is an open-source, Windows-first desktop email client for existing IMAP and SMTP accounts. It should feel calm, attractive, fast, and easy to understand. It has no SmartMail account, hosted backend, AI features, or mandatory cloud service.

"Smart" means thoughtful presentation and useful organization: clear visual hierarchy, fast search, and account groups that users control.

## Product rules

1. Every default feature must help someone read, find, organize, or send mail.
2. The basic experience should work without creating an account with SmartMail.
3. Keep message data local. Do not add telemetry by default.
4. Show what an action will do; make organization reversible where possible.
5. Favour a few polished workflows over many settings and integrations.
6. Measure startup time, memory use, package size, and search speed before adding dependencies.

## First release: included

### Account setup

- Connect multiple existing email accounts; no SmartMail login is needed.
- Offer guided Gmail and Outlook/Microsoft account connection using provider sign-in (OAuth2), plus manual IMAP/SMTP settings for other providers.
- Manual setup includes separate incoming and outgoing server settings, encrypted transport, and a connection test for each server.
- Save secrets through the operating system's credential store.
- Explain connection, authentication, and certificate failures in plain language.

### Account groups

- Let users create, rename, reorder, and remove as many groups as they need, such as Personal, Work, or Project A.
- Let users name each connected account and place it in one group. Moving an account between groups must not move messages on its mail server.
- Show the accounts within each group and a combined inbox for that group. Also offer an All Inboxes view.
- Show which account received each message in combined views. Reply and send from the correct account by default, with a visible way to change it.
- Keep groups local to SmartMail. They organize accounts in the app; they are distinct from server folders, labels, and message rules.

### Mail

- Sync folders and message headers; fetch message bodies and attachments when needed.
- Group and account navigation, folder list, message list, reading pane, and compose window.
- Read/unread, star, move, archive when the account supports it, and delete.
- Compose, reply, forward, send, and attach files.
- Keep the local cache consistent with server changes and recover cleanly after disconnection.

### Finding and organizing

- Fast local search across cached sender, recipients, subject, and message text that has been fetched.
- Search within one account, one group, or all accounts.
- Customizable navigation: reorder groups, pin or hide folders, and choose a compact or comfortable message density.
- Clearly indicate when search results are limited to locally cached content.

### Visual design

- Calm, minimal layout with strong typography, generous spacing, and restrained colour.
- Light and dark themes, keyboard navigation, and accessible contrast.
- Make common actions visible; put advanced settings in one predictable place.

## Outside the first release

- AI summaries, drafting, or classification.
- SmartMail accounts, server-side sync, and cross-device settings sync.
- Calendar, contacts, chat, plugins, and collaboration features.
- POP3, Exchange-specific APIs, and mobile apps.
- Automatic message classification and complex message rules.
- Nested account groups or placing one account in several groups.

## Provider compatibility boundary

Manual server configuration does not guarantee that a provider accepts password authentication. Outlook.com requires OAuth2 for IMAP/SMTP; Gmail may work with an app password on eligible accounts. The first release will include guided OAuth2 connections for Gmail and Outlook/Microsoft accounts. These flows require provider application registration, and Google's requested mail scope may require verification before a public release. Publish a tested compatibility list rather than implying that all IMAP/SMTP providers work identically.

## Proposed implementation direction

- **Desktop shell and UI:** Start with the [proposed stack](TECH_STACK.md): Tauri 2, Rust mail core, and plain TypeScript/CSS. Validate package size, memory use, and startup time with a native prototype.
- **Mail core:** Keep IMAP sync, SMTP submission, MIME parsing, provider authentication, and account settings behind narrow interfaces so the UI does not depend on protocol details.
- **Local store:** SQLite for cached metadata and message bodies; FTS5 for local full-text search.
- **Groups:** Store group names, order, and account membership locally. Combined inboxes and group search are views across account data, not copies of messages.
- **Security:** Follow [SECURITY_PLAN.md](SECURITY_PLAN.md) for TLS, credentials, local cache, untrusted mail, account isolation, and release controls.

## Milestones

1. **Design prototype:** account connection, account groups, combined inbox, reading pane, search, and compose using sample mail. Validate visual direction and keyboard flow.
2. **Protocol prototype:** connect to test accounts, list folders, read messages, and send from the correct account. Confirm TLS and credential storage.
3. **Useful alpha:** reliable sync across several accounts, local cache, group inboxes, core mail actions, attachments, and scoped search.
4. **Polished beta:** group and folder customization, light/dark themes, accessibility, performance work, and failure recovery.
5. **Public release:** Windows packaging, documentation, license, privacy statement, and a tested provider compatibility list.

## Release checks

- New users can connect a supported account and send their first email without consulting protocol documentation.
- Users can add several accounts, organize them into groups, and switch between one account, a group inbox, and All Inboxes.
- A reply from a combined inbox selects the receiving account by default.
- Restarting, losing network access, or reconnecting does not duplicate or lose messages.
- Search results explain the local cache boundary.
- The app stays responsive while syncing a large mailbox or opening an attachment.
- Package size, idle memory, startup time, and search latency are recorded and reviewed against agreed budgets.

## Decisions still open

- Should a group inbox show only Inbox messages, or also messages from selected folders?
- Should the app show notifications while closed, or should a background process be a later feature?
- Which open-source license should the project use?
- What numeric performance and package-size budgets fit the target Windows hardware?

## Technical references

- [IMAP4rev2](https://www.rfc-editor.org/rfc/rfc9051.html) and [mail submission](https://www.rfc-editor.org/info/rfc6409/)
- [Gmail OAuth for IMAP/SMTP](https://developers.google.com/workspace/gmail/imap/xoauth2-protocol)
- [Microsoft OAuth for IMAP/SMTP](https://learn.microsoft.com/en-us/exchange/client-developer/legacy-protocols/how-to-authenticate-an-imap-pop-smtp-application-by-using-oauth)
- [Outlook.com connection settings](https://support.microsoft.com/en-us/outlook/pop-imap-and-smtp-settings-for-outlook-com)
- [Tauri architecture](https://v2.tauri.app/concept/architecture/) and [SQLite FTS5](https://www.sqlite.org/fts5.html)
