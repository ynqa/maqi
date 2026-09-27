//! Input acquisition; both sources return raw manuals with the root first.
pub mod bin;
pub mod debian;

use anyhow::{Context, Result, ensure};
use flate2::read::GzDecoder;
use regex::Regex;
use std::{collections::BTreeSet, io::Read};

const MAX_PAGE_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_PAGES: usize = 1000;

#[derive(Debug)]
pub struct Document {
    pub source: String,
    pub text: String,
}

pub fn read_limited(reader: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(MAX_PAGE_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_PAGE_BYTES,
        "manual exceeds {MAX_PAGE_BYTES} bytes"
    );
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<String> {
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        read_limited(GzDecoder::new(bytes))?
    } else {
        bytes.to_vec()
    };
    String::from_utf8(bytes).context("manual is not UTF-8")
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.+-".contains(c))
}

// Only discover references here; the parser verifies the actual command path.
fn references(bin: &str, text: &str) -> Result<BTreeSet<String>> {
    let fonts = Regex::new(r"\\f(?:\[[^]]*\]|[BIPR1234])")?;
    let plain = fonts
        .replace_all(text, "")
        .replace("\\-", "-")
        .replace('"', "");
    let man = Regex::new(r"([A-Za-z0-9][A-Za-z0-9_.+-]*)\s*\([18]\)")?;
    let mdoc = Regex::new(r"(?m)^\.Xr\s+([A-Za-z0-9][A-Za-z0-9_.+-]*)\s+[18]\b")?;
    Ok(man
        .captures_iter(&plain)
        .chain(mdoc.captures_iter(&plain))
        .map(|c| c[1].to_owned())
        .filter(|name| name.starts_with(&format!("{bin}-")))
        .collect())
}
