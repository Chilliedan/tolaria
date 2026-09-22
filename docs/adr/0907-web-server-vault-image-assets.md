---
type: ADR
id: "0907"
title: "Web server serves vault image attachments over an authenticated asset route"
status: active
date: 2026-09-22
---

## Context

On desktop, `convertFileSrc` turns an absolute vault file path into a Tauri
asset URL (`asset://localhost/<percent-encoded path>`) that the webview loads
directly from disk. The web build aliased `convertFileSrc` to a passthrough
that returned the raw filesystem path, so every `<img>` in the editor pointed
at a URL the server did not serve (the SPA fallback answered with
`index.html`). Images never rendered on web, and features built on them —
including attachment rename, now routed on the server — were only half
usable.

The editor does not just display these URLs: it converts them back into
portable `attachments/...` paths when it serializes markdown
(`src/utils/vaultAttachments.ts`, via a shared list of recognised asset-URL
prefixes). Any web URL scheme therefore has to round-trip through that code
exactly as Tauri's does, or asset URLs would leak into saved notes.

## Decision

- **Route**: `tolaria-server` serves `GET /api/asset/*path` behind the existing
  `require_auth` session layer. The wildcard is the percent-decoded absolute
  path of the file.
- **URL shape**: the web `convertFileSrc` returns
  `/api/asset/${encodeURIComponent(path)}` — the same `<prefix><encoded path>`
  shape as Tauri's asset URLs. `'/api/asset/'` is added to
  `ASSET_URL_PREFIXES` in `vaultAttachments.ts`, so every existing reverse
  mapping (portable-path serialization, rename source resolution, vault
  containment checks) works unchanged. Custom protocols (for example the
  desktop-only `tolaria-html-block`) are left as a passthrough.
- **What is served**: only files whose extension is an image type (png, jpeg,
  gif, webp, avif, bmp, ico, svg), and only after the requested path
  canonicalizes (resolving `..` and symlinks) to a regular file inside
  `vault_root`. Notes, the users database, and anything outside the vault are
  never served. Every refusal is a plain `404`, so the route cannot be used
  to probe which files exist.
- **Response headers**: an explicit `Content-Type`,
  `X-Content-Type-Options: nosniff`, and `Cache-Control: private, no-cache`,
  so authenticated content is not stored by shared caches. SVGs also get
  `Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; sandbox`,
  so a script inside an SVG cannot run with the app's origin if the file is
  opened directly rather than through `<img>`.

## Consequences

- Images render in the web editor, and attachment rename is fully usable on
  web.
- Non-image previews that call `convertFileSrc` (PDFs, standalone HTML files)
  now receive a clean `404` from the asset route instead of the SPA's
  `index.html`. Serving them is a separate decision: their sandboxing needs
  differ from images.
- Files are read fully into memory per request; there is no range or
  conditional-request support. That is fine for typical note attachments, but
  very large media would warrant streaming later.
- A markdown link whose URL literally starts with `/api/asset/` is now treated
  as an asset URL on desktop too. Such a URL has no meaning on desktop, so
  this is accepted.
