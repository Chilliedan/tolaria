---
type: ADR
id: "0148"
title: "Built-in authentication for the web server (tolaria-server, Phase 3)"
status: active
date: 2026-07-01
---

## Context

ADR-0147 shipped Phase 2 of the web client: an unauthenticated, read-only
`tolaria-server` intended to sit behind a trusted reverse proxy. The web
client design spec's Phase 3 requires the server to authenticate its own
users rather than relying solely on network-level trust, so that the vault
can be exposed beyond a fully trusted network boundary and so later phases
(write operations in Phase 4, git sync in Phase 5) have an identity to
attribute changes to.

Constraints carried over from Phase 2:

1. **No self-service signup.** Tolaria vaults are personal/small-team; an
   admin (or the vault owner) provisions accounts out of band.
2. **Single web client codebase.** The React SPA still ships one build; the
   login surface does not need to participate in that build if it is simpler
   to keep it server-rendered.
3. **`tolaria-core` stays the single source of vault logic.** Auth is a
   `tolaria-server`-local concern — it does not touch `tolaria-core`.

## Decision

### User store: SQLite + argon2 (`tolaria-server::users`)

Accounts live in a SQLite database (`TOLARIA_USERS_DB`, default
`/app/data/users.db`), created on first run if missing. Each row stores
`username`, an argon2 password hash (via the `argon2` crate, `OsRng`-salted),
and a `git_name` / `git_email` pair. The git identity columns are populated
at account creation but are **not consumed anywhere yet** — they exist so
Phase 4/5 (write commits, git sync) can attribute changes per-user without a
schema migration. `verify_credentials` returns a `UserRecord` (no password
hash) on success and `None` on any failure — wrong password and unknown
username are indistinguishable to the caller, so login responses cannot be
used to enumerate valid usernames.

### Session model: in-memory store + opaque-token cookie

Sessions are **not** persisted or backed by JWTs. `SessionStore` is an
in-memory `HashMap<String, StoredSession>` behind an `RwLock`, keyed by a
32-byte (256-bit) token generated with `OsRng` and hex-encoded — opaque and
unguessable, carrying no embedded claims. Sessions expire on a fixed TTL
(7 days) from creation; `SessionStore::get` lazily evicts expired entries.
Restarting the server invalidates all sessions, which is an accepted
trade-off for Phase 3 — a durable session store is not required by the spec
and would add a second piece of persistent state beyond the users DB.

The token is carried in an HttpOnly cookie (`tolaria_session`):

| Attribute | Value | Reason |
|---|---|---|
| `HttpOnly` | always on | not readable from JS, mitigates XSS token theft |
| `SameSite` | `Lax` | blocks cross-site POST/XHR replay, allows normal top-level navigation (e.g. following a link to the app) |
| `Secure` | `TOLARIA_COOKIE_SECURE` (default `true`) | cookie is only sent over HTTPS in production |
| `Path` | `/` | valid for the whole app, including `/api/*` and the SPA |

`TOLARIA_COOKIE_SECURE=false` exists solely to allow local/non-TLS testing
(e.g. `docker compose up` on `http://localhost` without a reverse proxy in
front). It must not be set to `false` in any deployment reachable over plain
HTTP from untrusted networks.

### Login UI: server-rendered, outside the React SPA

`GET /login` is rendered as a plain HTML form directly by `tolaria-server`
(`auth_routes::login_page`), not by the React app. `POST /api/auth/login`
consumes the form, verifies credentials, and on success creates a session
and sets the cookie before redirecting to `/`; on failure it redirects back
to `/login?error=1`. `POST /api/auth/logout` clears the session (store-side
and cookie-side) and redirects to `/login`. `GET /api/auth/me` returns the
current session's username as JSON, or 401 if unauthenticated.

Keeping the login page server-rendered and outside the SPA build means it
does not need to participate in the React i18n system (`src/lib/locales/`)
or the Tauri/web transport shim — it is a static, dependency-free HTML
response that works identically whether or not the SPA bundle has loaded,
and it never needs the `invoke` shim from ADR-0147. This is a deliberate
scope trade: the login page is currently English-only. Localizing it is
left to a future task if/when the web client gains real end users who need
it, tracked outside this ADR.

### Route protection: `require_auth` middleware

`auth_middleware::require_auth` wraps the router (Axum
`middleware::from_fn_with_state`) and checks the session cookie against the
`SessionStore` on every request that reaches it. Unauthenticated requests
are rejected differently depending on route shape, matching how each client
type consumes errors:

- `/api/*` → `401 Unauthorized` with a JSON `{ "error": "not authenticated" }`
  body, so the SPA's fetch/XHR calls can detect the failure programmatically.
- everything else (SPA fallback / pages) → `302`/`303` redirect to `/login`,
  so a browser navigating directly lands on the sign-in form instead of a
  raw JSON error.

`/login`, `/api/auth/login`, and static login assets are mounted outside
this middleware layer so the login flow itself is reachable while
unauthenticated.

### Provisioning: `useradd` CLI, no self-service signup

`tolaria-server useradd <username> <git_name> <git_email>` (password read
from the `TOLARIA_NEW_PASSWORD` environment variable, not a CLI argument, to
avoid it landing in shell history or `ps`) opens the configured users DB and
inserts a new argon2-hashed row via `run_useradd`. There is no HTTP
registration endpoint and none is planned — every account is provisioned by
whoever operates the deployment (the vault owner, or an admin), consistent
with Tolaria's personal/small-team scope.

### Docker packaging

`TOLARIA_USERS_DB` defaults to `/app/data/users.db` inside the runtime
image; the Dockerfile ensures `/app/data` exists. `docker-compose.yml` mounts
a named volume, `tolaria_users:/app/data`, so the users DB (and therefore
every provisioned account) survives container recreation, and sets
`TOLARIA_COOKIE_SECURE` from the host environment (defaulting to `true`).
Operators running behind TLS-terminating nginx (ADR-0147's `proxy` service
or a host nginx) need no override; only a non-TLS local test setup should
pass `TOLARIA_COOKIE_SECURE=false`.

## Consequences

- The web server is no longer unauthenticated-by-default: every route except
  `/login` and the login POST requires a valid session.
- Restarting `tolaria-server` (including `docker compose restart` /
  redeploys) invalidates all active sessions; users must sign in again. This
  is acceptable for Phase 3 and can be revisited if session durability
  becomes a real usability problem.
- The users DB is a new piece of persistent state distinct from the vault
  mount; it must be backed up/migrated independently of vault content if a
  deployment is moved.
- Write operations (Phase 4) and git sync + WebSocket push (Phase 5) remain
  **out of scope** here. This ADR only establishes *who is logged in*; it
  does not yet grant any additional server capability beyond the read-only
  RPC whitelist from ADR-0147. The stored `git_name`/`git_email` columns are
  inert until a later phase's write path reads them.
- The login page is a second, unlocalized UI surface alongside the
  internationalized React SPA. This is an intentional, documented gap, not
  an oversight.
