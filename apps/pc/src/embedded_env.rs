//! Build-time embedded `.env` configuration.
//!
//! `build.rs` reads the repository-root `.env` and writes the raw content into
//! `OUT_DIR/embedded_env.rs`. This module parses that content for the settings
//! UI. The raw text is part of the compiled binary; anyone with the executable
//! can extract it, so never ship a build containing production secrets.

include!(concat!(env!("OUT_DIR"), "/embedded_env.rs"));

#[derive(Debug, Clone, serde::Serialize)]
pub struct Entry {
    pub key: String,
    pub value: String,
}

/// Parsed `.env` entries. Supports `KEY=value`, `KEY:value` and the full-width
/// Chinese colon used by the local deployment file (`KEY：value`).
pub fn entries() -> Vec<Entry> {
    parse(RAW)
}

pub fn parse(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (index, separator_len) = line.char_indices().find_map(|(index, ch)| {
                if ch == '=' || ch == ':' || ch == '：' {
                    Some((index, ch.len_utf8()))
                } else {
                    None
                }
            })?;
            let key = line[..index].trim();
            if key.is_empty() {
                return None;
            }
            Some(Entry {
                key: key.to_owned(),
                value: line[index + separator_len..].trim().to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_equals_and_full_width_colon() {
        let text = "# comment\nheadscale服务器地址：https://wg.example.com/\nheadscale-key=abc123\nEMPTY=\n";
        let entries = parse(text);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].key, "headscale服务器地址");
        assert_eq!(entries[0].value, "https://wg.example.com/");
        assert_eq!(entries[1].value, "abc123");
        assert_eq!(entries[2].value, "");
    }
}
