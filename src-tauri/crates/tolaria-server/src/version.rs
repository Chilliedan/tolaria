use sha2::{Digest, Sha256};

/// Lowercase hex sha256 of the content's UTF-8 bytes. Must match the client's
/// Web Crypto SHA-256 over the same bytes.
pub fn content_version(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_and_lowercase_hex_64() {
        let v = content_version("hello");
        assert_eq!(v, content_version("hello"));
        assert_eq!(v.len(), 64);
        assert!(v.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn differs_on_change() {
        assert_ne!(content_version("a"), content_version("b"));
    }

    #[test]
    fn known_vector() {
        // sha256("") = e3b0c442...
        assert_eq!(&content_version("")[..8], "e3b0c442");
    }
}
