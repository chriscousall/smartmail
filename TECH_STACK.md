# SmartMail: proposed technical stack

## Decision

Build the Windows desktop client with **Tauri 2**, a **Rust mail core**, a **plain JavaScript/CSS interface**, and **SQLite with FTS5** for the local cache and search. The interface runs inside Tauri; the separate browser mockup has been removed.

Tauri uses the system WebView2 renderer on Windows rather than bundling a browser engine. The Rust side will own IMAP/SMTP, OAuth, MIME parsing, local persistence, and privileged operations. The interface will receive typed view data and narrow commands; it will not receive provider tokens or direct database access.

## Why this fits "no bloat"

- No Electron or bundled Chromium runtime.
- No React or broad UI component library by default. Add a focused dependency only when it removes more complexity than it adds.
- No backend service, analytics SDK, AI service, or background daemon in the first release.
- One SQLite file for the local cache and FTS5 index; no separate search server.
- Fetch headers first and message bodies or attachments when needed.
- Bundle fonts and assets locally; prefer Windows system fonts.
- Keep platform-specific work behind interfaces so macOS/Linux can be evaluated later without reshaping the mail model.

## Performance budgets to validate

These are initial **targets**, not claims about a build that does not exist yet:

| Measure | Initial target | How to test |
| --- | --- | --- |
| Windows installer | Under 50 MB | Measure signed release installer. |
| Idle memory | Under 150 MB with one account open | Measure after sync settles on a reference Windows PC. |
| Warm launch to usable inbox | Under 2 seconds | Time 20 launches with a populated cache. |
| Search response | Under 200 ms for a 50,000-message local index | Run representative sender/subject/body queries. |
| Initial UI assets | Under 500 KB compressed | Inspect production asset output. |

If the Tauri prototype misses a budget, investigate the cost before adding features. A smaller binary alone is not enough; startup, memory, and responsiveness matter to users.

## Implementation order

1. Finish and validate the interaction prototype with sample mail.
2. Install the Rust/Tauri toolchain and wrap the interface in a Windows desktop shell.
3. Build a narrow account and mail-core API, then connect one test IMAP/SMTP account.
4. Add local SQLite sync and FTS5 search with account IDs enforced throughout.
5. Add Gmail and Microsoft OAuth through the system browser, followed by multi-account sync.
6. Apply the release controls in [SECURITY_PLAN.md](SECURITY_PLAN.md) and measure the budgets above.

## References

- [Tauri architecture](https://v2.tauri.app/concept/architecture/) and [Windows prerequisites](https://tauri.app/start/prerequisites/)
- [SQLite FTS5](https://www.sqlite.org/fts5.html)
