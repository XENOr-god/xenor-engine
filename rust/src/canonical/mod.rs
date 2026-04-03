use std::collections::BTreeSet;
use std::fmt;

use crate::core::checksum_bytes;

pub const CANONICAL_TEXT_ENCODING: &str = "xenor-canonical-text/v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalError {
    detail: String,
}

impl CanonicalError {
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for CanonicalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for CanonicalError {}

#[derive(Default)]
pub struct CanonicalLineWriter {
    lines: Vec<String>,
}

impl CanonicalLineWriter {
    pub fn push_display(&mut self, key: &str, value: impl fmt::Display) {
        self.lines.push(format!("{key}={value}"));
    }

    pub fn push_str_hex(&mut self, key: &str, value: &str) {
        self.push_display(key, encode_hex(value.as_bytes()));
    }

    pub fn finish(self) -> Vec<u8> {
        format!("{}\n", self.lines.join("\n")).into_bytes()
    }
}

pub struct CanonicalLineReader<'a> {
    lines: Vec<(&'a str, &'a str)>,
    index: usize,
    consumed_keys: BTreeSet<&'a str>,
}

impl<'a> CanonicalLineReader<'a> {
    pub fn new(bytes: &'a [u8], label: &str) -> Result<Self, CanonicalError> {
        if bytes.is_empty() {
            return Err(CanonicalError::new(format!(
                "{label}: empty canonical payload"
            )));
        }

        if !bytes.ends_with(b"\n") {
            return Err(CanonicalError::new(format!(
                "{label}: missing terminal newline"
            )));
        }

        let text = std::str::from_utf8(bytes)
            .map_err(|error| CanonicalError::new(format!("{label}: invalid utf8: {error}")))?;

        if text.contains('\r') {
            return Err(CanonicalError::new(format!(
                "{label}: carriage returns are not allowed"
            )));
        }

        let mut lines = Vec::new();
        for (index, line) in text.split_terminator('\n').enumerate() {
            if line.is_empty() {
                return Err(CanonicalError::new(format!(
                    "{label}: blank line at position {}",
                    index + 1
                )));
            }

            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| CanonicalError::new(format!("{label}: invalid line `{line}`")))?;

            if key.is_empty() {
                return Err(CanonicalError::new(format!(
                    "{label}: empty key in line `{line}`"
                )));
            }

            if value.contains('=') {
                return Err(CanonicalError::new(format!(
                    "{label}: unescaped delimiter in line `{line}`"
                )));
            }

            lines.push((key, value));
        }

        Ok(Self {
            lines,
            index: 0,
            consumed_keys: BTreeSet::new(),
        })
    }

    pub fn read_value(&mut self, key: &str, label: &str) -> Result<&'a str, CanonicalError> {
        let (actual_key, value) =
            self.lines.get(self.index).copied().ok_or_else(|| {
                CanonicalError::new(format!("{label}: missing required key `{key}`"))
            })?;

        if actual_key != key {
            let detail = if self.consumed_keys.contains(actual_key) {
                format!("{label}: duplicate key `{actual_key}`")
            } else {
                format!("{label}: invalid field order, expected `{key}`, got `{actual_key}`")
            };
            return Err(CanonicalError::new(detail));
        }

        self.index += 1;
        self.consumed_keys.insert(actual_key);
        Ok(value)
    }

    pub fn expect_value(
        &mut self,
        key: &str,
        expected: &str,
        label: &str,
    ) -> Result<(), CanonicalError> {
        let value = self.read_value(key, label)?;
        if value != expected {
            return Err(CanonicalError::new(format!(
                "{label}: expected `{key}={expected}`, got `{value}`"
            )));
        }

        Ok(())
    }

    pub fn finish(self, label: &str) -> Result<(), CanonicalError> {
        if let Some((key, _)) = self.lines.get(self.index) {
            let detail = if self.consumed_keys.contains(key) {
                format!("{label}: duplicate key `{key}`")
            } else {
                format!("{label}: unexpected trailing key `{key}`")
            };
            return Err(CanonicalError::new(detail));
        }

        Ok(())
    }
}

pub fn canonical_digest(bytes: &[u8]) -> u64 {
    checksum_bytes(bytes)
}

pub fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        encoded.push(char::from(HEX[(byte >> 4) as usize]));
        encoded.push(char::from(HEX[(byte & 0x0f) as usize]));
    }

    encoded
}

pub fn decode_hex(value: &str, label: &str) -> Result<Vec<u8>, CanonicalError> {
    if value.len() % 2 != 0 {
        return Err(CanonicalError::new(format!(
            "{label}: invalid hex length {}",
            value.len()
        )));
    }

    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        bytes.push((hex_value(pair[0], label)? << 4) | hex_value(pair[1], label)?);
    }

    Ok(bytes)
}

pub fn decode_hex_string(value: &str, label: &str) -> Result<String, CanonicalError> {
    let bytes = decode_hex(value, label)?;
    String::from_utf8(bytes)
        .map_err(|error| CanonicalError::new(format!("{label}: invalid utf8 string: {error}")))
}

fn hex_value(byte: u8, label: &str) -> Result<u8, CanonicalError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(CanonicalError::new(format!(
            "{label}: invalid hex byte `{}`",
            char::from(byte)
        ))),
    }
}
