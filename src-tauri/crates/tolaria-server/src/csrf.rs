//! Double-submit CSRF protection for `/api/cmd/*`.
//!
//! Login sets a non-HttpOnly `tolaria_csrf` cookie; the client JS reads it
//! and echoes it back in the `X-CSRF-Token` header on every command call.
//! `rpc::command_route` rejects requests where the header is missing or
//! does not match the cookie.

use axum::http::HeaderMap;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use rand::rngs::OsRng;
use rand::RngCore;

pub const CSRF_COOKIE: &str = "tolaria_csrf";
pub const CSRF_HEADER: &str = "x-csrf-token";

/// Generate a fresh 32-byte token, hex-encoded (64 chars).
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Non-HttpOnly so the browser JS can read it and echo it in the header.
pub fn csrf_cookie(value: String, secure: bool) -> Cookie<'static> {
    Cookie::build((CSRF_COOKIE, value))
        .http_only(false)
        .same_site(SameSite::Lax)
        .secure(secure)
        .path("/")
        .build()
}

/// True iff the `tolaria_csrf` cookie and `X-CSRF-Token` header are both
/// present, non-empty, and equal.
pub fn verify(jar: &CookieJar, headers: &HeaderMap) -> bool {
    let cookie = jar.get(CSRF_COOKIE).map(|c| c.value().to_string());
    let header = headers
        .get(CSRF_HEADER)
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);
    match (cookie, header) {
        (Some(c), Some(h)) => !c.is_empty() && c == h,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn jar_with(token: &str) -> CookieJar {
        CookieJar::new().add(Cookie::new(CSRF_COOKIE, token.to_string()))
    }
    fn headers_with(token: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(CSRF_HEADER, HeaderValue::from_str(token).unwrap());
        h
    }

    #[test]
    fn matching_passes() {
        assert!(verify(&jar_with("abc"), &headers_with("abc")));
    }
    #[test]
    fn mismatch_fails() {
        assert!(!verify(&jar_with("abc"), &headers_with("xyz")));
    }
    #[test]
    fn missing_header_fails() {
        assert!(!verify(&jar_with("abc"), &HeaderMap::new()));
    }
    #[test]
    fn missing_cookie_fails() {
        assert!(!verify(&CookieJar::new(), &headers_with("abc")));
    }
    #[test]
    fn empty_cookie_fails_even_if_header_matches() {
        assert!(!verify(&jar_with(""), &headers_with("")));
    }
    #[test]
    fn token_is_64_hex() {
        let t = generate_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }
    #[test]
    fn tokens_are_unique() {
        assert_ne!(generate_token(), generate_token());
    }
}
