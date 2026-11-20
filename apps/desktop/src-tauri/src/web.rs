//! Web search: a model looks something up on the web while it answers, such
//! as "I need a plumber in Jacksonville, FL".
//!
//! The rules, each with a test:
//!
//! - A search goes out only while the outbound gate is on, over https, to one
//!   of two fixed hosts: Brave Search's API when the person gave a key, and
//!   DuckDuckGo's plain results page when they did not. No setting changes
//!   either address, and no redirect is followed.
//! - The person gives the path to a file that holds their Brave key, as for
//!   an outside engine. The app keeps the path, never the key, and the key
//!   never reaches the page.
//! - Only the search words leave the computer. What comes back is cut to a
//!   few results of plain text, with every tag taken out, before the model
//!   reads it.
//! - Whether a model may search at all is the person's switch in Settings,
//!   off to start; the page keeps it and offers the tool only while it is on.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::gate::Gate;

/// The person's Brave key, read from their file for one search and dropped
/// after it. It has no `Debug` and no `Display`, so nothing can print it.
struct Key(String);

const KEY_FILE_MOST: u64 = 1024;
const KEY_SHORTEST: usize = 20;
const KEY_LONGEST: usize = 256;

pub const BRAVE_HOST: &str = "api.search.brave.com";
const BRAVE_URL: &str = "https://api.search.brave.com/res/v1/web/search";
pub const DDG_HOST: &str = "html.duckduckgo.com";
const DDG_URL: &str = "https://html.duckduckgo.com/html/";

/// The most results the model reads, and the most of an answer read.
const MOST_RESULTS: usize = 8;
const MOST_BYTES: usize = 2_000_000;
/// The longest search, in characters, and the longest snippet kept.
const LONGEST_QUERY: usize = 400;
const LONGEST_SNIPPET: usize = 400;
const PATH_MOST: usize = 4096;

/// The key in the file at `path`. A file that holds anything but one key is
/// refused, and no message says what the file holds.
fn read_key(path: &Path) -> Result<Key, String> {
    let about = fs::metadata(path)
        .map_err(|_| "There is no key file at that path, or the app cannot read it.".to_string())?;
    if about.is_dir() {
        return Err("That path is a folder. Give the file that holds the key.".to_string());
    }
    if about.len() > KEY_FILE_MOST {
        return Err("That file is too big to be a key.".to_string());
    }
    let mut text = String::new();
    fs::File::open(path)
        .and_then(|file| file.take(KEY_FILE_MOST + 1).read_to_string(&mut text))
        .map_err(|_| "The app could not read the key file.".to_string())?;
    let key = text.trim();
    if key.len() < KEY_SHORTEST
        || key.len() > KEY_LONGEST
        || !key.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(
            "That file does not hold just a key. Put the key alone in it, on one line.".to_string(),
        );
    }
    Ok(Key(key.to_string()))
}

/// A warning when other accounts on this computer can read the key file.
fn warning_for(path: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path).ok()?.permissions().mode();
        if mode & 0o077 != 0 {
            return Some("Other accounts on this computer can read the key file. Run chmod 600 on it to keep it to yours.".to_string());
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    None
}

/// This computer's home folder, for a path that starts with `~/`.
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
}

/// The key as the search's sign-in header, marked so nothing prints it.
fn signed(builder: reqwest::RequestBuilder, key: &Key) -> Result<reqwest::RequestBuilder, String> {
    let mut value = reqwest::header::HeaderValue::from_str(&key.0)
        .map_err(|_| "That file does not hold just a key.".to_string())?;
    value.set_sensitive(true);
    Ok(builder.header("X-Subscription-Token", value))
}

const WAIT: Duration = Duration::from_secs(20);
const CONNECT_WAIT: Duration = Duration::from_secs(10);

const GATE_OFF: &str =
    "The outbound gate is off, so the web cannot be searched. Turn it on to search.";

/// One result, as the model reads it.
#[derive(Debug, PartialEq)]
struct Hit {
    title: String,
    url: String,
    snippet: String,
}

// ---------------------------------------------------------------------------
// What is kept: the path to the Brave key, never the key
// ---------------------------------------------------------------------------

#[derive(Default, Deserialize, Serialize)]
struct Kept {
    #[serde(default)]
    key_path: Option<PathBuf>,
}

pub struct Web {
    file: PathBuf,
    kept: Mutex<Kept>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebStatus {
    /// Which service a search goes to: "brave" or "duckduckgo".
    engine: &'static str,
    key_path: Option<String>,
    warning: Option<String>,
}

impl Web {
    pub fn new(file: PathBuf) -> Self {
        let kept = fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str::<Kept>(&text).ok())
            .map(|kept| Kept {
                key_path: kept.key_path.filter(|path| path.is_absolute()),
            })
            .unwrap_or_default();
        Self {
            file,
            kept: Mutex::new(kept),
        }
    }

    fn key_path(&self) -> Option<PathBuf> {
        self.kept
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .key_path
            .clone()
    }

    fn status(&self) -> WebStatus {
        let path = self.key_path();
        WebStatus {
            engine: if path.is_some() {
                "brave"
            } else {
                "duckduckgo"
            },
            key_path: path.as_ref().map(|path| path.display().to_string()),
            warning: path.as_deref().and_then(warning_for),
        }
    }

    fn keep(&self, path: Option<PathBuf>) -> Result<WebStatus, String> {
        let text = serde_json::to_string(&Kept {
            key_path: path.clone(),
        })
        .map_err(|error| error.to_string())?;
        if let Some(folder) = self.file.parent() {
            let _ = fs::create_dir_all(folder);
        }
        fs::write(&self.file, text)
            .map_err(|error| format!("The app could not keep that: {error}."))?;
        self.kept.lock().unwrap_or_else(|e| e.into_inner()).key_path = path;
        Ok(self.status())
    }
}

/// The path the person gave, whole. A key pasted in its place is refused,
/// and the message never repeats it.
fn checked_path(given: &str) -> Result<PathBuf, String> {
    let whole =
        "Give the whole path to the file that holds your Brave key, such as ~/.config/brave/key.";
    let given = given.trim();
    if given.is_empty() || given.chars().count() > PATH_MOST {
        return Err(whole.to_string());
    }
    let path = match given
        .strip_prefix("~/")
        .or_else(|| given.strip_prefix("~\\"))
    {
        Some(rest) => home().ok_or(whole)?.join(rest),
        None => PathBuf::from(given),
    };
    if !path.is_absolute() {
        return Err(whole.to_string());
    }
    Ok(path)
}

// ---------------------------------------------------------------------------
// Reading what comes back
// ---------------------------------------------------------------------------

/// The text an HTML fragment shows: tags out, the common entities read.
fn plain(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for char in html.chars() {
        match char {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(char),
            _ => {}
        }
    }
    let text = text
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    let words: Vec<&str> = text.split_whitespace().collect();
    cut(&words.join(" "), LONGEST_SNIPPET)
}

fn cut(text: &str, most: usize) -> String {
    if text.chars().count() <= most {
        return text.to_string();
    }
    let mut kept: String = text.chars().take(most).collect();
    kept.push('…');
    kept
}

/// Only web addresses are passed on to the model.
fn web_address(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

/// Brave's answer: its web results.
fn brave_hits(answer: &[u8]) -> Result<Vec<Hit>, String> {
    let value: Value = serde_json::from_slice(answer)
        .map_err(|_| "Brave Search's answer could not be read.".to_string())?;
    let results = value
        .pointer("/web/results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(results
        .iter()
        .filter_map(|result| {
            let url = result.get("url")?.as_str()?;
            if !web_address(url) {
                return None;
            }
            Some(Hit {
                title: plain(result.get("title").and_then(Value::as_str).unwrap_or(url)),
                url: url.to_string(),
                snippet: plain(
                    result
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                ),
            })
        })
        .take(MOST_RESULTS)
        .collect())
}

/// The address a DuckDuckGo result points to: its links go through
/// DuckDuckGo first, with the real address in `uddg`. Ads are left out.
fn ddg_target(href: &str) -> Option<String> {
    let href = href.replace("&amp;", "&");
    let full = if href.starts_with("//") {
        format!("https:{href}")
    } else {
        href
    };
    let url = reqwest::Url::parse(&full).ok()?;
    if url
        .host_str()
        .is_some_and(|host| host.ends_with("duckduckgo.com"))
    {
        if url.path().starts_with("/y.js") {
            return None;
        }
        let target = url
            .query_pairs()
            .find(|(name, _)| name == "uddg")?
            .1
            .into_owned();
        // An ad's link goes back through DuckDuckGo.
        let own = reqwest::Url::parse(&target)
            .ok()
            .and_then(|url| url.host_str().map(|host| host.ends_with("duckduckgo.com")))
            .unwrap_or(true);
        return (web_address(&target) && !own).then_some(target);
    }
    web_address(&full).then_some(full)
}

/// The value of `name="..."` in a tag's attributes.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let start = tag.find(&format!("{name}=\""))? + name.len() + 2;
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

/// DuckDuckGo's plain results page: each result's link, title, and snippet.
fn ddg_hits(page: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut parts = page.split("class=\"result__a\"");
    parts.next();
    for part in parts {
        // The link's own tag, from the class to the end of the title.
        let Some(tag_end) = part.find('>') else {
            continue;
        };
        let Some(href) =
            attribute(part, "href").filter(|_| part.find("href=\"").is_some_and(|at| at < tag_end))
        else {
            continue;
        };
        let Some(url) = ddg_target(href) else {
            continue;
        };
        let title_end = part[tag_end..]
            .find("</a>")
            .map_or(part.len(), |at| at + tag_end);
        let title = plain(&part[tag_end + 1..title_end]);
        let snippet = part
            .find("class=\"result__snippet\"")
            .and_then(|at| {
                let open = part[at..].find('>')? + at + 1;
                let close = part[open..]
                    .find("</a>")
                    .map_or(part.len(), |end| end + open);
                Some(plain(&part[open..close]))
            })
            .unwrap_or_default();
        if !title.is_empty() {
            hits.push(Hit {
                title,
                url,
                snippet,
            });
        }
        if hits.len() == MOST_RESULTS {
            break;
        }
    }
    hits
}

/// The results as the model reads them.
fn written(query: &str, from: &str, hits: &[Hit]) -> String {
    if hits.is_empty() {
        return format!("No results for \"{query}\" from {from}.");
    }
    let mut text = format!("Results for \"{query}\" from {from}:\n");
    for (at, hit) in hits.iter().enumerate() {
        text.push_str(&format!("\n{}. {}\n   {}\n", at + 1, hit.title, hit.url));
        if !hit.snippet.is_empty() {
            text.push_str(&format!("   {}\n", hit.snippet));
        }
    }
    text
}

fn checked_query(query: &str) -> Result<String, String> {
    let words: Vec<&str> = query.split_whitespace().collect();
    let query = words.join(" ");
    if query.is_empty() {
        return Err(
            "Say what to search for: {\"tool\": \"web_search\", \"query\": \"...\"}.".to_string(),
        );
    }
    if query.chars().count() > LONGEST_QUERY {
        return Err(format!(
            "That search is too long. Keep it under {LONGEST_QUERY} characters."
        ));
    }
    Ok(query)
}

// ---------------------------------------------------------------------------
// Asking
// ---------------------------------------------------------------------------

fn client(gate: &Gate) -> Result<reqwest::Client, String> {
    gate.client(GATE_OFF, |builder| {
        builder
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!(
                "Mozilla/5.0 (compatible; rusty-business/",
                env!("CARGO_PKG_VERSION"),
                ")"
            ))
            .connect_timeout(CONNECT_WAIT)
            .timeout(WAIT)
    })
}

async fn read(response: reqwest::Response, host: &str, name: &str) -> Result<Vec<u8>, String> {
    if response.url().host_str() != Some(host) {
        return Err(format!(
            "{name} sent the search somewhere else, so the app stopped."
        ));
    }
    let code = response.status().as_u16();
    match code {
        200 => {}
        401 | 403 if host == BRAVE_HOST => {
            return Err(
                "Brave Search did not take the key. Check the key file in Settings, Web."
                    .to_string(),
            );
        }
        429 => {
            return Err(format!(
                "{name} says too many searches (429). Wait a moment and try again."
            ));
        }
        _ => return Err(format!("{name} answered {code}.")),
    }
    let mut chunks = response.bytes_stream();
    let mut answer = Vec::new();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|_| format!("{name} stopped answering part way through."))?;
        if answer.len() + chunk.len() > MOST_BYTES {
            return Err(format!("{name}'s answer was too large."));
        }
        answer.extend_from_slice(&chunk);
    }
    Ok(answer)
}

async fn search(web: &Web, gate: &Gate, query: &str) -> Result<String, String> {
    let query = checked_query(query)?;
    // The gate is looked at before the key is read.
    let client = client(gate)?;
    let wait = gate.clone();
    let asked = async {
        if let Some(path) = web.key_path() {
            let key = read_key(&path)?;
            let url = reqwest::Url::parse_with_params(
                BRAVE_URL,
                &[("q", query.as_str()), ("count", &MOST_RESULTS.to_string())],
            )
            .map_err(|error| error.to_string())?;
            let response = signed(client.get(url).header("Accept", "application/json"), &key)?
                .send()
                .await
                .map_err(|_| "Brave Search could not be reached.".to_string())?;
            let answer = read(response, BRAVE_HOST, "Brave Search").await?;
            Ok(written(&query, "Brave Search", &brave_hits(&answer)?))
        } else {
            let url = reqwest::Url::parse_with_params(DDG_URL, &[("q", query.as_str())])
                .map_err(|error| error.to_string())?;
            let response = client
                .get(url)
                .send()
                .await
                .map_err(|_| "DuckDuckGo could not be reached.".to_string())?;
            let answer = read(response, DDG_HOST, "DuckDuckGo").await?;
            let hits = ddg_hits(&String::from_utf8_lossy(&answer));
            if hits.is_empty() && !String::from_utf8_lossy(&answer).contains("No results") {
                return Err("DuckDuckGo gave no results it could read. A Brave Search key, in Settings, Web, is steadier.".to_string());
            }
            Ok(written(&query, "DuckDuckGo", &hits))
        }
    };
    // Turning the gate off stops a search under way.
    tokio::select! {
        done = asked => done,
        () = wait.closed() => Err(GATE_OFF.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn web_status(web: tauri::State<'_, Web>) -> WebStatus {
    web.status()
}

/// Keeps the path to the person's Brave key. The page sends the path, never
/// the key, and the key is read here once to check it is one.
#[tauri::command]
pub fn set_web_key(key_path: String, web: tauri::State<'_, Web>) -> Result<WebStatus, String> {
    let path = checked_path(&key_path)?;
    read_key(&path)?;
    web.keep(Some(path))
}

/// Forgets the path. The key file is never touched.
#[tauri::command]
pub fn remove_web_key(web: tauri::State<'_, Web>) -> Result<WebStatus, String> {
    web.keep(None)
}

#[tauri::command]
pub async fn web_search(
    query: String,
    web: tauri::State<'_, Web>,
    gate: tauri::State<'_, Gate>,
) -> Result<String, String> {
    search(&web, &gate, &query).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::async_runtime::block_on;

    fn place(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("rusty-web-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_closed_gate_searches_nothing_and_reads_no_key() {
        let folder = place("gate");
        let web = Web::new(folder.join("web.json"));
        // A key path that cannot be read: were it read first, that would be the error.
        web.keep(Some(folder.join("missing"))).unwrap();
        let error = block_on(search(
            &web,
            &Gate::at(false),
            "plumber in Jacksonville, FL",
        ))
        .unwrap_err();
        assert_eq!(error, GATE_OFF);
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn only_the_two_fixed_hosts_over_https() {
        for url in [BRAVE_URL, DDG_URL] {
            let url = reqwest::Url::parse(url).unwrap();
            assert_eq!(url.scheme(), "https");
            assert!([BRAVE_HOST, DDG_HOST].contains(&url.host_str().unwrap()));
        }
    }

    #[test]
    fn the_path_is_kept_never_the_key() {
        let folder = place("kept");
        let key = folder.join("key");
        fs::write(&key, "BSAabcdefghijklmnopqrstuvwxyz123\n").unwrap();
        let web = Web::new(folder.join("web.json"));
        assert_eq!(web.status().engine, "duckduckgo");
        let status = web.keep(Some(key.clone())).unwrap();
        assert_eq!(status.engine, "brave");
        let kept = fs::read_to_string(folder.join("web.json")).unwrap();
        assert!(!kept.contains("BSAabc"), "the key was kept");
        assert_eq!(Web::new(folder.join("web.json")).key_path(), Some(key));
        web.keep(None).unwrap();
        assert!(Web::new(folder.join("web.json")).key_path().is_none());
        assert!(folder.join("key").exists(), "the key file was touched");
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_pasted_key_is_not_a_path() {
        let error = checked_path("BSAabcdefghijklmnopqrstuvwxyz123").unwrap_err();
        assert!(!error.contains("BSAabc"));
        assert!(checked_path("").is_err());
        assert!(checked_path("/Users/me/.config/brave/key").is_ok());
    }

    #[test]
    fn a_search_is_trimmed_and_kept_short() {
        assert_eq!(
            checked_query("  plumber   in Jacksonville ").unwrap(),
            "plumber in Jacksonville"
        );
        assert!(checked_query("   ").is_err());
        assert!(checked_query(&"a".repeat(LONGEST_QUERY + 1)).is_err());
    }

    #[test]
    fn brave_results_are_read_as_plain_text() {
        let answer = br#"{"web":{"results":[
            {"title":"Best <strong>Plumbers</strong> in Jacksonville","url":"https://example.com/a","description":"Call &amp; save <strong>today</strong>."},
            {"title":"Bad","url":"javascript:alert(1)","description":"x"},
            {"title":"Second","url":"https://example.org/b"}
        ]}}"#;
        assert_eq!(
            brave_hits(answer).unwrap(),
            vec![
                Hit {
                    title: "Best Plumbers in Jacksonville".into(),
                    url: "https://example.com/a".into(),
                    snippet: "Call & save today.".into()
                },
                Hit {
                    title: "Second".into(),
                    url: "https://example.org/b".into(),
                    snippet: "".into()
                },
            ]
        );
        assert!(brave_hits(b"{}").unwrap().is_empty());
        assert!(brave_hits(b"not json").is_err());
    }

    #[test]
    fn duckduckgo_results_are_read_and_ads_left_out() {
        let page = r#"
<div class="result results_links_deep result--ad">
  <h2 class="result__title"><a rel="nofollow" class="result__a" href="https://duckduckgo.com/y.js?ad_domain=x&amp;u3=1">Ad</a></h2>
</div>
<div class="result result--ad">
  <h2 class="result__title"><a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fduckduckgo.com%2Fy.js%3Fad_domain%3Dx&amp;rut=1">Wrapped ad</a></h2>
</div>
<div class="result results_links results_links_deep web-result ">
  <h2 class="result__title">
    <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.example.com%2Fplumbing&amp;rut=abc">Jacksonville <b>Plumbing</b> Co.</a>
  </h2>
  <a class="result__snippet" href="//duckduckgo.com/l/?uddg=x">Licensed plumbers serving Jacksonville, FL &amp; nearby. Call 904-555-0100.</a>
</div>
<div class="result">
  <h2 class="result__title"><a rel="nofollow" class="result__a" href="https://plain.example.org/">Plain link</a></h2>
</div>"#;
        assert_eq!(
            ddg_hits(page),
            vec![
                Hit {
                    title: "Jacksonville Plumbing Co.".into(),
                    url: "https://www.example.com/plumbing".into(),
                    snippet:
                        "Licensed plumbers serving Jacksonville, FL & nearby. Call 904-555-0100."
                            .into(),
                },
                Hit {
                    title: "Plain link".into(),
                    url: "https://plain.example.org/".into(),
                    snippet: "".into()
                },
            ]
        );
        assert!(ddg_hits("<html>nothing</html>").is_empty());
    }

    #[test]
    fn results_are_written_for_the_model() {
        let hits = vec![Hit {
            title: "A".into(),
            url: "https://a.example".into(),
            snippet: "about a".into(),
        }];
        assert_eq!(
            written("q", "Brave Search", &hits),
            "Results for \"q\" from Brave Search:\n\n1. A\n   https://a.example\n   about a\n"
        );
        assert_eq!(
            written("q", "DuckDuckGo", &[]),
            "No results for \"q\" from DuckDuckGo."
        );
    }

    #[test]
    fn long_snippets_are_cut() {
        let long = "word ".repeat(200);
        assert!(plain(&long).chars().count() <= LONGEST_SNIPPET + 1);
    }
}
