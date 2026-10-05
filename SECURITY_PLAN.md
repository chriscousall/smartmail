# SmartMail: security plan

This plan sits alongside [PLAN.md](PLAN.md). It covers the Windows-first desktop release with multiple IMAP/SMTP accounts, Gmail and Microsoft OAuth sign-in, manual account setup, local search, and combined inboxes.

## Security goals and limits

Protect provider credentials, private messages, attachments, and the integrity of send, move, and delete actions. Keep mail data on the user's device except when communicating with their mail provider or when the user deliberately opens an external link or remote content.

SmartMail cannot make email end-to-end encrypted by itself, guarantee that a provider keeps mail private, or protect data from malware already running with the user's Windows account. These limits should be explained plainly rather than hidden behind a generic "secure" claim.

## Trust boundaries

1. **Mail servers and OAuth providers → mail core:** servers return untrusted protocol responses, message bodies, headers, and attachments.
2. **Mail core → local cache:** the cache stores sensitive content and must retain account ownership for every record.
3. **Mail core → interface:** email HTML and strings are untrusted, even when they came from a known contact.
4. **Interface → privileged operations:** opening files, sending mail, changing account settings, and reading secrets require narrow, validated commands.
5. **Build system → user device:** an installer or update can run with the app's privileges, so release integrity matters.

## Main risks and first-release controls

| Risk | Why it matters for SmartMail | Required control |
| --- | --- | --- |
| Malicious HTML email | An email could execute script in the desktop webview or access privileged app commands. | Prefer plain text where possible. Sanitize HTML with a maintained allowlist, disable scripts/forms/iframes, render mail in an isolated webview with no privileged bridge, and enforce a restrictive Content Security Policy. |
| Remote images and tracking | Loading an image can reveal that a message was opened and expose the user's IP address. | Block remote content by default. Offer a per-message action to load it, with the destination made clear. |
| Phishing and misleading links | Display text can differ from the real destination. | Open links in the system browser, reveal the actual destination in the interface, warn when the visible address differs, and never let `javascript:` or local-file URLs launch from message content. |
| Attachment and parser attacks | MIME parts may be malformed, huge, recursive, or disguised as another file type. | Use maintained parsers; impose message, part, nesting, and download limits; use generated safe temporary names; open files only after explicit user action; do not auto-execute attachments. |
| Token or password theft | One leaked token can grant broad mailbox access. | Use the Windows credential store for refresh tokens and passwords; keep secrets out of the frontend, database, logs, URLs, crash reports, and source control. Remove them when an account is disconnected. |
| OAuth interception or account confusion | A forged redirect could connect the wrong provider account or steal a code. | Use the system browser, authorization code with PKCE S256, a one-time state value, exact redirect matching, issuer/provider validation, and per-attempt expiry. Never embed a client secret in the desktop app. |
| Intercepted mail connection | An attacker on the network could read credentials or mail, or alter content. | Require TLS for IMAP and SMTP authentication, validate hostname and certificate chain, and never silently fall back to cleartext after a TLS failure. Show a clear error rather than a casual "ignore certificate" button. |
| Local cache disclosure | Search indexes, message bodies, attachments, and drafts may remain on disk. | Keep data in the user's app-data directory with user-only access; define retention and deletion behavior; evaluate whole-cache encryption before public release. Do not claim the cache is encrypted until implemented and verified. |
| Wrong-account action | Group inboxes mix personal and business messages; a wrong send or move may disclose information. | Bind every message and operation to an immutable account ID. Reply from the receiving account by default. Show the sender account prominently in compose and confirm cross-account changes where possible. |
| Overpowered frontend | A webview bug could invoke file, shell, or network operations. | Expose only narrow app commands; validate account ID and arguments in the mail core; give windows the least Tauri capabilities; avoid generic shell, arbitrary SQL, and unrestricted file APIs. |
| Compromised update or dependency | An attacker could distribute a malicious build or exploit a vulnerable parser. | Pin dependencies, review changes, scan for known vulnerabilities, protect CI and signing keys, sign Windows releases, and verify update signatures before installation. |

## Security design rules

### Accounts and credentials

- Treat the Gmail and Microsoft desktop app as public OAuth clients. Request only scopes required for IMAP/SMTP functionality, document why they are needed, and keep provider tokens separate per account.
- Refresh tokens should remain in the privileged mail core and Windows credential storage. The interface receives connection status, never raw tokens. Select the Windows storage API with the expected number of accounts in mind; Credential Locker has a documented 20-credential limit per app.
- Manual accounts may use provider-issued app passwords. Reject insecure authentication over unencrypted connections.
- Disconnecting an account revokes provider access where the provider supports it, removes local credentials, and offers a clear choice about deleting its local cache.

### Message and account isolation

- Cache keys include account ID, mailbox identity, and server UID validity. Never use a message ID or subject alone as a unique key.
- Search and combined inbox queries may span accounts, but each result retains its source account. Grouping changes only the local view.
- Server operations use the source account's connection. Moving mail between different accounts is a separate export/copy feature and is out of scope for the first release.
- Avoid prefetching external resources. Keep downloaded attachments out of broad shared directories until the user saves or opens them.

### Privacy and diagnostics

- No telemetry, analytics SDK, or mail-content upload in the first release.
- Diagnostic logs contain event types and error codes, not message bodies, subject lines, addresses, tokens, or passwords. User-submitted logs should be inspectable before sharing.
- Document where cached data is stored, what is retained after account removal, and how to erase it.
- A local app lock can be considered later, but it should not be presented as protection from malware running as the same Windows user.

### Open-source release process

- Keep a short `SECURITY.md` with a private vulnerability-reporting route and supported-version policy before public launch.
- Require code review for mail parsing, credential handling, OAuth, updater, and release workflow changes.
- Run dependency and secret scanning in CI; keep lockfiles committed; publish signed release artifacts and checksums.
- Treat signing credentials and provider registration settings as release secrets. Never commit a desktop "client secret" and assume it stays secret.
- Have a way to issue a fixed release quickly and explain to users when they should revoke a provider connection.

## Verification gates

**Before an alpha with real accounts**

- Test TLS certificate failures and STARTTLS downgrade attempts.
- Test OAuth cancellation, replayed redirects, wrong-provider redirects, expired state, and reconnecting two accounts at once.
- Confirm passwords and tokens cannot be found in logs, cache, or frontend state. Disable automatic crash-dump collection or upload; document that operating-system memory dumps may contain live secrets.
- Confirm one account's message cannot be sent, moved, deleted, or displayed through another account ID.

**Before public release**

- Test HTML mail with script, event handlers, forms, `javascript:` links, remote images, and malformed markup.
- Feed malformed MIME, oversized attachments, deep nesting, and unusual filenames through the parser.
- Review the effective webview permissions and Content Security Policy in the packaged build.
- Test cache removal, account disconnect, backup/restore behavior, and signed update verification.
- Complete a focused independent security review of OAuth, message rendering, native command boundaries, and release signing.

## Decisions to settle during design

1. **Cache at rest:** use Windows user-only storage alone, or add application-level database encryption. Decide before real user data is stored in a public build, based on the threat model and measured performance cost.
2. **HTML rendering:** choose and test a sanitizer and isolation approach before connecting live mail.
3. **Remote content:** decide whether to allow a per-sender exception after the safe per-message default is proven.
4. **Updates:** choose the distribution channel and signing method before the first public installer.
5. **Retention:** define when messages, search terms, thumbnails, and temporary attachments are deleted.

## Primary references

- [OAuth for native apps, RFC 8252](https://www.rfc-editor.org/info/rfc8252/) and [OAuth security best current practice, RFC 9700](https://www.rfc-editor.org/info/rfc9700/)
- [TLS for email submission and access, RFC 8314](https://www.rfc-editor.org/info/rfc8314/)
- [OWASP cross-site scripting prevention](https://cheatsheetseries.owasp.org/cheatsheets/Cross_Site_Scripting_Prevention_Cheat_Sheet.html)
- [Tauri Content Security Policy](https://v2.tauri.app/security/csp/), [capabilities](https://v2.tauri.app/security/capabilities/), and [signed updater](https://v2.tauri.app/plugin/updater/)
- [Microsoft Windows Credential Locker](https://learn.microsoft.com/en-us/windows/apps/develop/security/credential-locker)
