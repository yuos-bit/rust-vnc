use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::code::CODE_ALPHABET;

/// Number of characters in the default pairing password.
pub const PASSWORD_LEN: usize = 8;

/// Argon2id parameters. The defaults are RFC 9106 compliant and take roughly
/// 40 ms on a modern desktop, which is appropriate for interactive pairing.
fn argon2() -> Argon2<'static> {
    Argon2::default()
}

/// Generates a readable but unguessable default password (32^8 ≈ 2^40).
pub fn generate_password() -> Result<String, String> {
    random_chars(PASSWORD_LEN)
}

/// Generates the opaque credential the pairing server issues to a controlled
/// endpoint. It is shown exactly once and only its SHA-256 digest is stored.
pub fn generate_agent_secret() -> Result<String, String> {
    Ok(format!("agt_{}", random_token()?))
}

/// Generates a long-lived "passwordless" controller token.
pub fn generate_token() -> Result<String, String> {
    Ok(format!("rvt_{}", random_token()?))
}

fn random_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("random source failed: {e}"))?;
    Ok(base64_url(&bytes))
}

fn random_chars(len: usize) -> Result<String, String> {
    let mut out = String::with_capacity(len);
    let mut buf = [0u8; 32];
    while out.len() < len {
        getrandom::fill(&mut buf).map_err(|e| format!("random source failed: {e}"))?;
        for &b in &buf {
            if b < 224 {
                out.push(CODE_ALPHABET[(b % 32) as usize] as char);
                if out.len() == len {
                    break;
                }
            }
        }
    }
    Ok(out)
}

fn base64_url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Hashes a password with Argon2id and a fresh random salt.
pub fn hash_password(password: &str) -> Result<String, String> {
    let mut salt_bytes = [0u8; 16];
    getrandom::fill(&mut salt_bytes).map_err(|e| format!("random source failed: {e}"))?;
    let salt =
        SaltString::encode_b64(&salt_bytes).map_err(|e| format!("salt encoding failed: {e}"))?;
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("password hashing failed: {e}"))
}

/// Verifies a password against a PHC string. Unknown/corrupt hashes fail
/// closed rather than panicking.
pub fn verify_password(password: &str, phc_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(phc_hash) else {
        return false;
    };
    argon2()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// SHA-256 digest used to store opaque tokens/secrets at rest.
pub fn hash_secret_hex(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Length-independent constant-time comparison for equal-length secrets.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let password = generate_password().unwrap();
        assert_eq!(password.len(), PASSWORD_LEN);
        let hash = hash_password(&password).unwrap();
        assert!(verify_password(&password, &hash));
        assert!(!verify_password("WRONG123", &hash));
        assert!(!verify_password(&password, "not-a-phc-string"));
    }

    #[test]
    fn salts_are_unique() {
        let a = hash_password("SAME1234").unwrap();
        let b = hash_password("SAME1234").unwrap();
        assert_ne!(a, b);
        assert!(verify_password("SAME1234", &a));
        assert!(verify_password("SAME1234", &b));
    }

    #[test]
    fn token_prefixes_and_hashes() {
        let token = generate_token().unwrap();
        assert!(token.starts_with("rvt_"));
        assert!(token.len() > 40);
        let digest = hash_secret_hex(&token);
        assert_eq!(digest.len(), 64);
        assert_ne!(digest, hash_secret_hex(&generate_token().unwrap()));
        assert!(constant_time_eq(&digest, &digest));
        assert!(!constant_time_eq(&digest, "00"));
    }
}
