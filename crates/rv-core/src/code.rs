/// Unambiguous 32-character alphabet: no `0/O`, no `1/I/L`.
pub const CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
pub const CODE_LEN: usize = 6;

/// Largest multiple of the alphabet length that fits in a byte (32 * 7).
/// Bytes at or above this bound are rejected so modulo reduction stays uniform.
const REJECTION_BOUND: u8 = 224;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodeError {
    #[error("device code must contain exactly {CODE_LEN} characters")]
    InvalidLength,
    #[error("device code contains an unsupported character: {0:?}")]
    InvalidCharacter(char),
    #[error("operating system random source failed: {0}")]
    Random(String),
}

/// Generates a cryptographically random 6-character device code.
pub fn generate_device_code() -> Result<String, CodeError> {
    let mut out = String::with_capacity(CODE_LEN);
    let mut buf = [0u8; 16];

    while out.len() < CODE_LEN {
        getrandom::fill(&mut buf).map_err(|e| CodeError::Random(e.to_string()))?;
        for &byte in &buf {
            if byte < REJECTION_BOUND {
                out.push(CODE_ALPHABET[(byte % 32) as usize] as char);
                if out.len() == CODE_LEN {
                    break;
                }
            }
        }
    }
    Ok(out)
}

/// Normalizes operator/controller input: trims, drops separators, uppercases
/// and validates every character against [`CODE_ALPHABET`].
pub fn normalize_device_code(input: &str) -> Result<String, CodeError> {
    let mut out = String::with_capacity(CODE_LEN);
    for ch in input.chars() {
        if ch.is_whitespace() || ch == '-' || ch == '_' {
            continue;
        }
        let upper = ch.to_ascii_uppercase();
        if !CODE_ALPHABET.contains(&(upper as u8)) {
            return Err(CodeError::InvalidCharacter(ch));
        }
        out.push(upper);
    }
    if out.chars().count() != CODE_LEN {
        return Err(CodeError::InvalidLength);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn generated_codes_have_expected_shape() {
        for _ in 0..1000 {
            let code = generate_device_code().unwrap();
            assert_eq!(code.len(), CODE_LEN);
            assert!(code.bytes().all(|b| CODE_ALPHABET.contains(&b)));
            assert!(normalize_device_code(&code).is_ok());
        }
    }

    #[test]
    fn generated_codes_are_unique_enough() {
        let set: HashSet<_> = (0..2000).map(|_| generate_device_code().unwrap()).collect();
        assert!(set.len() > 1990, "only {} unique codes", set.len());
    }

    #[test]
    fn normalization_accepts_common_input_styles() {
        assert_eq!(normalize_device_code(" a2-b3 c4 ").unwrap(), "A2B3C4");
        assert_eq!(normalize_device_code("a2b3c4").unwrap(), "A2B3C4");
    }

    #[test]
    fn normalization_rejects_ambiguous_and_short_input() {
        assert!(matches!(
            normalize_device_code("ABCDE0"),
            Err(CodeError::InvalidCharacter('0'))
        ));
        assert!(matches!(
            normalize_device_code("ABCDE"),
            Err(CodeError::InvalidLength)
        ));
        assert!(matches!(
            normalize_device_code("ABCDEFG"),
            Err(CodeError::InvalidLength)
        ));
    }
}
