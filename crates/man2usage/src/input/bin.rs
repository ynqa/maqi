//! Locate installed manuals without running the documented command.
use super::{Document, MAX_PAGES, decode, read_limited, references, valid_name};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn load(bin: &str) -> Result<Vec<Document>> {
    ensure!(valid_name(bin), "invalid command name: {bin}");
    let output = Command::new("man")
        .args(["-w", bin])
        .env("LC_ALL", "C")
        .output()
        .context("could not run 'man -w'; install the man utility")?;
    ensure!(output.status.success(), "no installed man page for '{bin}'");
    let output = String::from_utf8(output.stdout).context("man returned a non-UTF-8 path")?;
    let root = output
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .with_context(|| format!("no readable installed man page for '{bin}'"))?;
    collect(bin, &root)
}

fn read(path: &Path) -> Result<Document> {
    Ok(Document {
        source: path.display().to_string(),
        text: decode(&read_limited(fs::File::open(path)?)?)?,
    })
}

fn collect(bin: &str, root: &Path) -> Result<Vec<Document>> {
    let directory = root.parent().context("manual has no parent directory")?;
    let mut siblings = BTreeMap::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let Some(filename) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let filename = filename.strip_suffix(".gz").unwrap_or(filename);
        let Some((name, section)) = filename.rsplit_once('.') else {
            continue;
        };
        if path.is_file() && matches!(section, "1" | "8") && name.starts_with(&format!("{bin}-")) {
            // Prefer uncompressed copies when both are present.
            if !siblings.contains_key(name) || path.extension().is_some_and(|s| s != "gz") {
                siblings.insert(name.to_owned(), path);
            }
        }
    }
    let mut documents = vec![read(root)?];
    let mut seen = BTreeSet::from([bin.to_owned()]);
    let mut index = 0;
    while index < documents.len() {
        for name in references(bin, &documents[index].text)? {
            if !seen.insert(name.clone()) {
                continue;
            }
            let path = siblings.get(&name).with_context(|| {
                format!(
                    "referenced man page '{name}' is missing from {}",
                    directory.display()
                )
            })?;
            ensure!(
                documents.len() < MAX_PAGES,
                "manual collection exceeds {MAX_PAGES} pages"
            );
            documents.push(read(path)?);
        }
        index += 1;
    }
    Ok(documents)
}
