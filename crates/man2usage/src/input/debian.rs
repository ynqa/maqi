//! Acquire a Debian root manual and its package's subcommand manuals.
use super::{Document, MAX_PAGES, decode, read_limited, valid_name};
use anyhow::{Context, Result, ensure};
use reqwest::{Url, blocking::Client, redirect::Policy};
use scraper::{Html, Selector};
use std::{collections::BTreeMap, time::Duration};

pub fn load(url: &str) -> Result<Vec<Document>> {
    let url = Url::parse(url).context("invalid Debian man-page URL")?;
    validate_origin(&url)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(40))
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                attempt.error("too many redirects")
            } else if validate_origin(attempt.url()).is_err() {
                attempt.error("redirect left manpages.debian.org")
            } else {
                attempt.follow()
            }
        }))
        .build()?;
    collect(&url, |url| {
        let response = client
            .get(url.clone())
            .send()?
            .error_for_status()
            .with_context(|| format!("fetching {url}"))?;
        Ok((response.url().clone(), read_limited(response)?))
    })
}

fn validate_origin(url: &Url) -> Result<()> {
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("manpages.debian.org")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none(),
        "expected an HTTPS man-page URL on manpages.debian.org"
    );
    Ok(())
}

#[derive(Debug)]
struct Page {
    directory: String,
    name: String,
    language: String,
}
impl Page {
    fn from_url(url: &Url) -> Result<Self> {
        validate_origin(url)?;
        let parts = url
            .path_segments()
            .context("missing URL path")?
            .collect::<Vec<_>>();
        ensure!(
            parts.len() == 3 && !parts[0].is_empty() && !parts[1].is_empty(),
            "expected a Debian man page: /<suite>/<package>/<command>.<section>.<language>.html"
        );
        let mut filename = parts[2].rsplitn(4, '.');
        ensure!(
            filename.next() == Some("html"),
            "specify the Debian HTML man-page link"
        );
        let language = filename.next().context("missing manual language")?;
        let section = filename.next().context("missing manual section")?;
        let name = filename.next().context("missing manual name")?;
        ensure!(
            valid_name(name) && matches!(section, "1" | "8") && !language.is_empty(),
            "expected a command manual in section 1 or 8"
        );
        Ok(Self {
            directory: format!("/{}/{}/", parts[0], parts[1]),
            name: name.into(),
            language: language.into(),
        })
    }
}

fn raw_url(url: &Url) -> Url {
    let mut raw = url.clone();
    raw.set_path(&format!("{}.gz", url.path().strip_suffix(".html").unwrap()));
    raw.set_query(None);
    raw.set_fragment(None);
    raw
}

fn collect(
    url: &Url,
    mut fetch: impl FnMut(&Url) -> Result<(Url, Vec<u8>)>,
) -> Result<Vec<Document>> {
    // Following the site's redirect also supports abbreviated man-page links.
    let (mut root_url, root_html) = fetch(url)?;
    root_url.set_fragment(None);
    root_url.set_query(None);
    let root = Page::from_url(&root_url)?;
    let html = Html::parse_document(std::str::from_utf8(&root_html)?);
    let links = Selector::parse("a[href]").unwrap();
    let expected_raw = raw_url(&root_url);
    ensure!(
        html.select(&links)
            .filter_map(|a| a.attr("href"))
            .filter_map(|href| root_url.join(href).ok())
            .any(|url| url == expected_raw),
        "Debian page has no raw man-page link: {root_url}"
    );
    let index_url = root_url.join("index.html")?;
    let (index_final, index) = fetch(&index_url)?;
    ensure!(
        index_final == index_url,
        "Debian package index redirected outside the selected package"
    );
    let index = Html::parse_document(std::str::from_utf8(&index)?);
    let mut pages = BTreeMap::new();
    let mut root_listed = false;
    for link in index.select(&links).filter_map(|a| a.attr("href")) {
        let Ok(mut url) = index_url.join(link) else {
            continue;
        };
        url.set_query(None);
        url.set_fragment(None);
        root_listed |= url == root_url;
        let Ok(page) = Page::from_url(&url) else {
            continue;
        };
        if page.directory == root.directory
            && page.language == root.language
            && page.name.starts_with(&format!("{}-", root.name))
        {
            ensure!(
                pages.len() < MAX_PAGES - 1,
                "manual collection exceeds {MAX_PAGES} pages"
            );
            // Multiple command sections are ambiguous; do not silently pick one.
            if let Some(previous) = pages.insert(page.name.clone(), url.clone()) {
                ensure!(
                    previous == url,
                    "multiple manuals for {} in the package index",
                    page.name
                );
            }
        }
    }
    ensure!(
        root_listed,
        "root manual is missing from the Debian package index: {index_url}"
    );
    let mut documents = Vec::new();
    for url in std::iter::once(root_url).chain(pages.into_values()) {
        let raw = raw_url(&url);
        let (final_url, bytes) = fetch(&raw)?;
        ensure!(
            final_url == raw,
            "raw manual redirected away from the selected version: {raw}"
        );
        let text = decode(&bytes)?;
        ensure!(
            !text.trim_start().starts_with('<'),
            "expected raw man source at {raw}"
        );
        documents.push(Document {
            source: raw.to_string(),
            text,
        });
        if documents.len() % 25 == 0 {
            eprintln!("Downloaded {} manuals", documents.len());
        }
    }
    Ok(documents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;

    #[test]
    fn root_link_discovers_all_descendants_in_the_same_package_and_language() {
        let start = Url::parse("https://manpages.debian.org/tool").unwrap();
        let root = Url::parse("https://manpages.debian.org/stable/tools/tool.1.en.html").unwrap();
        let index = root.join("index.html").unwrap();
        let raw = raw_url(&root);
        let child = root.join("tool-child.1.en.gz").unwrap();
        let grandchild = root.join("tool-child-leaf.1.en.gz").unwrap();
        let root_html = format!("<a href=\"{}\">raw man page</a>", raw.path());
        let index_html = r#"
            <a href="tool.1.en.html">root</a>
            <a href="tool-child.1.en.html">child</a>
            <a href="tool-child.1.en.html">duplicate</a>
            <a href="tool-child-leaf.1.en.html">leaf absent from SEE ALSO</a>
            <a href="tool-child.1.fr.html">other language</a>
            <a href="tool-internals.3.en.html">library</a>
            <a href="toolbox.1.en.html">unrelated</a>
            <a href="/testing/tools/tool-other.1.en.html">other suite</a>
            <a href="/stable/other/tool-other.1.en.html">other package</a>
            <a href="https://example.org/stable/tools/tool-other.1.en.html">other host</a>
        "#;
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(b".TH TOOL 1\n").unwrap();
        let gzip = gzip.finish().unwrap();
        let mut requests = Vec::new();
        let documents = collect(&start, |url| {
            requests.push(url.clone());
            if url == &start {
                Ok((root.clone(), root_html.as_bytes().into()))
            } else if url == &index {
                Ok((index.clone(), index_html.as_bytes().into()))
            } else {
                ensure!(
                    [&raw, &child, &grandchild].contains(&url),
                    "unexpected request {url}"
                );
                Ok((url.clone(), gzip.clone()))
            }
        })
        .unwrap();
        assert_eq!(documents.len(), 3);
        assert_eq!(documents[0].source, raw.as_str());
        assert!(
            documents
                .iter()
                .all(|document| document.text == ".TH TOOL 1\n")
        );
        assert_eq!(requests.len(), 5);
    }

    #[test]
    fn rejects_other_hosts_and_non_command_pages() {
        for url in [
            "http://manpages.debian.org/stable/tools/tool.1.en.html",
            "https://example.org/stable/tools/tool.1.en.html",
            "https://manpages.debian.org/stable/tools/tool.3.en.html",
            "https://manpages.debian.org/stable/tools/index.html",
        ] {
            assert!(Page::from_url(&Url::parse(url).unwrap()).is_err(), "{url}");
        }
    }

    #[test]
    fn failed_child_download_fails_the_collection() {
        let root = Url::parse("https://manpages.debian.org/stable/tools/tool.1.en.html").unwrap();
        let result = collect(&root, |url| {
            let body = if url == &root {
                "<a href='tool.1.en.gz'>raw man page</a>"
            } else if url.path().ends_with("index.html") {
                "<a href='tool.1.en.html'>root</a><a href='tool-child.1.en.html'>child</a>"
            } else if url.path().ends_with("tool.1.en.gz") {
                ".TH TOOL 1"
            } else {
                anyhow::bail!("404 child missing")
            };
            Ok((url.clone(), body.as_bytes().into()))
        });
        assert!(result.unwrap_err().to_string().contains("404"));
    }
}
