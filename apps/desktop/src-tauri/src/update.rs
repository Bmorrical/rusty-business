// The notice of a new version.
//
// It answers the ask that started it: "when a new build is ready, the user is
// notified in app, and it gives them the download". When the window opens,
// once a day while it stays open, and when the person clicks Check for
// updates ("is the user able to click a refresh button? If not, I want that.",
// then "check for updates I imply"), the page asks for `check_update`. Rust
// asks GitHub for this repository's newest release. When that release is
// newer than this app and holds this system's download, the page shows a
// notice, and its Download button opens that file in the browser, which
// downloads it as the README's link does.
//
// The rules, each with a test:
//
// - Only GitHub's API is asked, over https, and only for this repository's
//   newest release. No redirect is followed, and no proxy, account, token,
//   or cookie is used. The answer is read up to a size, and no further.
// - A release is a new version only when its tag is a plain version newer
//   than this app's, it is neither a draft nor a pre-release, and it holds
//   this system's file.
// - The page gives no address. It asks for the download or the notes of the
//   version Rust found, and Rust opens only this repository's releases on
//   github.com.
// - A check that fails keeps what the last one found, and says why. The
//   Check for updates button shows why; the other checks let it pass, and
//   the next one tries again.
// - The app downloads and installs nothing itself. The browser downloads the
//   file, and the person installs it, as the first time.
// - Nothing is asked while the outbound gate is off ("off by default,
//   everything stops"), and turning it off stops a check under way. Check for
//   updates then says so: "for update, it should tell them to enable the gate
//   to check for updates." Download and Release notes wait for it too.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};

use crate::gate::Gate;

/// The repository whose releases the app follows.
const REPOSITORY: &str = "Bmorrical/rusty-business";

/// The one host the check asks.
const API_HOST: &str = "api.github.com";

/// The one host the app opens in the browser.
const SITE_HOST: &str = "github.com";

/// The most of an answer the app reads. GitHub's answer for a release with
/// ten files is a few tens of kilobytes.
const MOST_BYTES: usize = 1_000_000;

/// How long the app waits to reach GitHub, and for the whole answer.
const CONNECT_WAIT: Duration = Duration::from_secs(15);
const WHOLE_WAIT: Duration = Duration::from_secs(30);

/// Where a `.deb` install is listed. A `.rpm` install has no such file.
const DPKG_LIST: &str = "/var/lib/dpkg/info/rusty-business.list";

/// Where the `.deb` and the `.rpm` put the program.
const PACKAGED_PROGRAM: &str = "/usr/bin/rusty-business";

const COULD_NOT_OPEN: &str = "The app could not open your browser.";

/// The address the check asks.
fn latest_url() -> String {
    format!("https://{API_HOST}/repos/{REPOSITORY}/releases/latest")
}

/// This repository's releases on GitHub, with no scheme, for a person to read.
fn releases_page() -> String {
    format!("{SITE_HOST}/{REPOSITORY}/releases")
}

// ---------------------------------------------------------------------------
// The download for this system
// ---------------------------------------------------------------------------

/// The five downloads a release holds, as the README links them.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Download {
    Mac,
    Windows,
    Deb,
    Rpm,
    TarGz,
}

impl Download {
    fn file(self) -> &'static str {
        match self {
            Download::Mac => "Rusty-Business-mac-arm64.dmg",
            Download::Windows => "Rusty-Business-windows-x64-setup.exe",
            Download::Deb => "Rusty-Business-linux-x64.deb",
            Download::Rpm => "Rusty-Business-linux-x64.rpm",
            Download::TarGz => "Rusty-Business-linux-x64.tar.gz",
        }
    }

    /// What to do once the browser has the file.
    fn steps(self) -> &'static str {
        match self {
            Download::Mac => {
                "When it is downloaded, quit Rusty Business™, open the .dmg, and drag the app onto Applications to replace this one. macOS may stop it the first time it opens, as before: choose Open Anyway in System Settings."
            }
            Download::Windows => {
                "When it is downloaded, open it. The installer replaces this version. Windows may stop it first, as before: choose More info, then Run anyway."
            }
            Download::Deb | Download::Rpm => {
                "When it is downloaded, run this in the folder it is in. It replaces this version."
            }
            Download::TarGz => {
                "When it is downloaded, quit Rusty Business™ and unpack it in place of this copy."
            }
        }
    }

    /// The command that installs it, when there is one to type.
    fn command(self) -> Option<String> {
        match self {
            Download::Deb => Some(format!("sudo apt install ./{}", self.file())),
            Download::Rpm => Some(format!("sudo dnf install ./{}", self.file())),
            _ => None,
        }
    }
}

/// Which Linux download this copy came from: the `.deb` and the `.rpm` put
/// the program in the same place, and only the `.deb` is listed by dpkg.
/// Anywhere else, it came from the `.tar.gz`.
fn linux_download(program: &Path, listed_by_dpkg: bool) -> Download {
    if program != Path::new(PACKAGED_PROGRAM) {
        Download::TarGz
    } else if listed_by_dpkg {
        Download::Deb
    } else {
        Download::Rpm
    }
}

/// The download for this computer, or none on a system with no download.
fn this_download() -> Option<Download> {
    if cfg!(target_os = "macos") {
        Some(Download::Mac)
    } else if cfg!(target_os = "windows") {
        Some(Download::Windows)
    } else if cfg!(target_os = "linux") {
        let program = std::env::current_exe().ok()?;
        Some(linux_download(&program, Path::new(DPKG_LIST).exists()))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Is the newest release a new version?
// ---------------------------------------------------------------------------

/// A plain version, such as `0.4.0` or `v0.4.0`: three whole numbers, and
/// nothing else.
fn version(text: &str) -> Option<[u64; 3]> {
    let text = text.strip_prefix('v').unwrap_or(text);
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut numbers = [0; 3];
    for (number, part) in numbers.iter_mut().zip(parts) {
        if part.is_empty() || part.len() > 9 || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        *number = part.parse().ok()?;
    }
    Some(numbers)
}

fn written([major, minor, patch]: [u64; 3]) -> String {
    format!("{major}.{minor}.{patch}")
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    #[serde(default)]
    state: String,
}

/// A new version, as the page is told of it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    /// The new version, such as `0.4.0`.
    version: String,
    /// This app's version.
    current: String,
    /// The file Download opens.
    file: String,
    /// What to do once the browser has it.
    steps: String,
    /// The command that installs it, on Linux.
    command: Option<String>,
    /// The release's page, with no scheme, in case the browser does not open.
    page: String,
}

/// The new version in GitHub's answer, or none. `current` is this app's
/// version.
fn update_from(current: &str, answer: &[u8], download: Download) -> Option<Update> {
    let release: Release = serde_json::from_slice(answer).ok()?;
    if release.draft || release.prerelease {
        return None;
    }
    let newest = version(&release.tag_name)?;
    // Release names a tag `v` and the version, and nothing else is taken.
    if release.tag_name != format!("v{}", written(newest)) {
        return None;
    }
    if newest <= version(current)? {
        return None;
    }
    let file = download.file();
    if !release
        .assets
        .iter()
        .any(|asset| asset.name == file && asset.state == "uploaded")
    {
        return None;
    }
    Some(Update {
        version: written(newest),
        current: current.to_string(),
        file: file.to_string(),
        steps: download.steps().to_string(),
        command: download.command(),
        page: format!("{}/tag/v{}", releases_page(), written(newest)),
    })
}

// ---------------------------------------------------------------------------
// Asking GitHub
// ---------------------------------------------------------------------------

/// True for an https address on exactly `host`, with no account in it and no
/// port of its own.
fn https_on(url: &reqwest::Url, host: &str) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.domain() == Some(host)
}

/// True for a page or file of this repository's releases on github.com.
fn on_releases(url: &reqwest::Url) -> bool {
    https_on(url, SITE_HOST)
        && url.path().starts_with(&format!("/{REPOSITORY}/releases/"))
        && url.query().is_none()
        && url.fragment().is_none()
}

/// What Check for updates says while the outbound gate is off.
const GATE_OFF: &str = "The gate is off. Turn it on to check for updates.";

/// What Download and Release notes say while the outbound gate is off.
const GATE_OFF_OPEN: &str = "The gate is off. Turn it on to open the download.";

/// The one client the check uses, through the outbound gate. It sends no
/// account, token, or cookie, and follows no redirect.
fn client(gate: &Gate) -> Result<reqwest::Client, String> {
    gate.client(GATE_OFF, |builder| {
        builder
            .no_proxy()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .referer(false)
            .user_agent(concat!("rusty-business/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(CONNECT_WAIT)
            .timeout(WHOLE_WAIT)
    })
}

/// An answer, read up to `most` bytes and no further.
async fn read_capped(
    chunks: impl Stream<Item = Result<Vec<u8>, String>>,
    most: usize,
) -> Result<Vec<u8>, String> {
    let mut chunks = std::pin::pin!(chunks);
    let mut answer = Vec::new();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk?;
        if answer.len() + chunk.len() > most {
            return Err("GitHub's answer was too large.".to_string());
        }
        answer.extend_from_slice(&chunk);
    }
    Ok(answer)
}

/// Why a check failed, from the status GitHub answered with. GitHub lets a
/// network ask 60 times an hour without an account, and answers 403 or 429
/// past that.
fn status_problem(status: u16) -> String {
    match status {
        403 | 429 => {
            "GitHub has had too many checks from this network for now. Try again later.".to_string()
        }
        _ => format!("GitHub answered {status}."),
    }
}

/// GitHub's answer for the newest release.
async fn ask(client: reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|_| "The app could not reach GitHub.".to_string())?;
    // With no redirects followed, this is the address asked. Checked once
    // more here.
    if !https_on(response.url(), API_HOST) {
        return Err("GitHub answered from somewhere else.".to_string());
    }
    if !response.status().is_success() {
        return Err(status_problem(response.status().as_u16()));
    }
    let chunks = response.bytes_stream().map(|chunk| {
        chunk
            .map(|bytes| bytes.to_vec())
            .map_err(|error| error.to_string())
    });
    read_capped(chunks, MOST_BYTES).await
}

/// GitHub's answer, while the outbound gate stays on. Turning it off stops
/// the check where it is.
async fn ask_through(gate: &Gate, url: &str) -> Result<Vec<u8>, String> {
    let client = client(gate)?;
    tokio::select! {
        answer = ask(client, url) => answer,
        () = gate.closed() => Err(GATE_OFF.to_string()),
    }
}

/// The new version the last check found, so the page never has to give an
/// address.
#[derive(Default)]
pub struct Updates(Mutex<Option<Update>>);

impl Updates {
    /// Keeps what a check found. A check that failed keeps what the last one
    /// found, and gives back why it failed.
    fn keep(&self, checked: Result<Option<Update>, String>) -> Result<Option<Update>, String> {
        let update = checked?;
        let mut found = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *found = update.clone();
        Ok(update)
    }

    fn found(&self) -> Option<Update> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

/// Asks GitHub whether a new version is out, unless the outbound gate is
/// off. A check that fails keeps what the last one found and answers why,
/// which the Check for updates button shows and the other checks let pass.
#[tauri::command]
pub async fn check_update(
    updates: tauri::State<'_, Updates>,
    gate: tauri::State<'_, Gate>,
) -> Result<Option<Update>, String> {
    let Some(download) = this_download() else {
        return Ok(None);
    };
    let checked = ask_through(&gate, &latest_url())
        .await
        .map(|answer| update_from(env!("CARGO_PKG_VERSION"), &answer, download));
    updates.keep(checked)
}

// ---------------------------------------------------------------------------
// Opening the download in the browser
// ---------------------------------------------------------------------------

/// What the page may ask to open: the new version's file, or its notes.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Part {
    Download,
    Notes,
}

/// The address of the part asked for, on this repository's releases.
fn address(update: &Update, part: Part) -> Result<reqwest::Url, String> {
    let address = match part {
        Part::Download => format!(
            "https://{SITE_HOST}/{REPOSITORY}/releases/download/v{}/{}",
            update.version, update.file
        ),
        Part::Notes => format!(
            "https://{SITE_HOST}/{REPOSITORY}/releases/tag/v{}",
            update.version
        ),
    };
    let url = reqwest::Url::parse(&address).map_err(|_| COULD_NOT_OPEN.to_string())?;
    if !on_releases(&url) {
        return Err(COULD_NOT_OPEN.to_string());
    }
    Ok(url)
}

/// The system's own way to open an address in the browser.
fn browser(url: &reqwest::Url) -> Command {
    let (program, before): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("/usr/bin/open", &[])
    } else if cfg!(target_os = "windows") {
        ("rundll32.exe", &["url.dll,FileProtocolHandler"])
    } else {
        ("xdg-open", &[])
    };
    let mut command = Command::new(program);
    command.args(before).arg(url.as_str());
    command
}

/// The address to open for the part asked for, only while the outbound gate
/// is on and a new version has been found.
fn to_open(part: Part, updates: &Updates, gate: &Gate) -> Result<reqwest::Url, String> {
    if !gate.is_open() {
        return Err(GATE_OFF_OPEN.to_string());
    }
    let update = updates
        .found()
        .ok_or_else(|| "No new version has been found.".to_string())?;
    address(&update, part)
}

/// Opens the new version's file or notes in the browser, unless the outbound
/// gate is off. The page names only which, never an address.
#[tauri::command]
pub fn open_update(
    part: Part,
    updates: tauri::State<'_, Updates>,
    gate: tauri::State<'_, Gate>,
) -> Result<(), String> {
    let url = to_open(part, &updates, &gate)?;
    let mut child = browser(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| COULD_NOT_OPEN.to_string())?;
    // Some systems keep the opener running until the browser closes, so the
    // app does not wait on it here.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::async_runtime::block_on;

    const MAC: &str = "Rusty-Business-mac-arm64.dmg";

    fn answer(tag: &str, files: &[&str]) -> Vec<u8> {
        let assets: Vec<_> = files
            .iter()
            .map(|name| serde_json::json!({ "name": name, "state": "uploaded" }))
            .collect();
        serde_json::json!({ "tag_name": tag, "draft": false, "prerelease": false, "assets": assets })
            .to_string()
            .into_bytes()
    }

    fn every_file() -> Vec<&'static str> {
        [
            Download::Mac,
            Download::Windows,
            Download::Deb,
            Download::Rpm,
            Download::TarGz,
        ]
        .iter()
        .flat_map(|download| [download.file(), "checksum"])
        .collect()
    }

    // "when a new build is ready, the user is notified in app, and it gives
    // them the download"
    #[test]
    fn a_newer_release_with_this_systems_file_is_a_new_version() {
        let found = update_from("0.4.0", &answer("v0.5.0", &every_file()), Download::Mac).unwrap();
        assert_eq!(
            found,
            Update {
                version: "0.5.0".into(),
                current: "0.4.0".into(),
                file: MAC.into(),
                steps: Download::Mac.steps().into(),
                command: None,
                page: "github.com/Bmorrical/rusty-business/releases/tag/v0.5.0".into(),
            }
        );
        // The page reads these names.
        let told = serde_json::to_value(&found).unwrap();
        let mut names: Vec<&str> = told
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        names.sort_unstable();
        assert_eq!(
            names,
            ["command", "current", "file", "page", "steps", "version"]
        );
        let url = address(&found, Part::Download).unwrap();
        assert_eq!(
            url.as_str(),
            "https://github.com/Bmorrical/rusty-business/releases/download/v0.5.0/Rusty-Business-mac-arm64.dmg"
        );
        assert_eq!(
            address(&found, Part::Notes).unwrap().as_str(),
            "https://github.com/Bmorrical/rusty-business/releases/tag/v0.5.0"
        );
    }

    #[test]
    fn the_same_or_an_older_release_is_not() {
        for tag in ["v0.4.0", "v0.3.9", "v0.0.1"] {
            assert_eq!(
                update_from("0.4.0", &answer(tag, &every_file()), Download::Mac),
                None,
                "{tag}"
            );
        }
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(update_from("0.9.0", &answer("v0.10.0", &every_file()), Download::Mac).is_some());
        assert!(update_from("0.4.9", &answer("v1.0.0", &every_file()), Download::Mac).is_some());
        assert!(update_from("0.4.0", &answer("v0.4.1", &every_file()), Download::Mac).is_some());
        assert_eq!(version("v12.0.3"), Some([12, 0, 3]));
        assert_eq!(version("0.4.0"), Some([0, 4, 0]));
    }

    #[test]
    fn a_release_without_this_systems_file_is_not_yet() {
        // A release made before Linux had downloads, as v0.2.0 was.
        let earlier = answer("v0.5.0", &[MAC, "Rusty-Business-windows-x64-setup.exe"]);
        assert!(update_from("0.4.0", &earlier, Download::Mac).is_some());
        for download in [Download::Deb, Download::Rpm, Download::TarGz] {
            assert_eq!(
                update_from("0.4.0", &earlier, download),
                None,
                "{download:?}"
            );
        }
        // A file still on its way up is not there yet.
        let uploading = serde_json::json!({
            "tag_name": "v0.5.0", "draft": false, "prerelease": false,
            "assets": [{ "name": MAC, "state": "starter" }],
        });
        assert_eq!(
            update_from("0.4.0", uploading.to_string().as_bytes(), Download::Mac),
            None
        );
        // A name that only starts like it is another file.
        assert_eq!(
            update_from(
                "0.4.0",
                &answer("v0.5.0", &[&format!("{MAC}.sha256")]),
                Download::Mac
            ),
            None
        );
    }

    #[test]
    fn a_draft_a_prerelease_or_an_odd_tag_is_not() {
        for flag in ["draft", "prerelease"] {
            let mut release: serde_json::Value =
                serde_json::from_slice(&answer("v0.5.0", &every_file())).unwrap();
            release[flag] = true.into();
            assert_eq!(
                update_from("0.4.0", release.to_string().as_bytes(), Download::Mac),
                None,
                "{flag}"
            );
        }
        for tag in [
            "0.5.0",
            "v0.5",
            "v0.5.0.1",
            "v0.5.0-beta",
            "v0.5.0+1",
            "v00.5.0",
            "v0.05.0",
            "V0.5.0",
            "v 0.5.0",
            "v0.5.0/../../x",
            "latest",
            "",
            "v1234567890.0.0",
        ] {
            assert_eq!(
                update_from("0.4.0", &answer(tag, &every_file()), Download::Mac),
                None,
                "{tag:?}"
            );
        }
        assert_eq!(update_from("0.4.0", b"not json", Download::Mac), None);
        assert_eq!(update_from("0.4.0", b"{}", Download::Mac), None);
        assert_eq!(
            update_from(
                "not a version",
                &answer("v0.5.0", &every_file()),
                Download::Mac
            ),
            None
        );
    }

    #[test]
    fn each_system_gets_its_own_file_and_steps() {
        let files: Vec<_> = [
            Download::Mac,
            Download::Windows,
            Download::Deb,
            Download::Rpm,
            Download::TarGz,
        ]
        .iter()
        .map(|download| download.file())
        .collect();
        assert_eq!(
            files,
            [
                "Rusty-Business-mac-arm64.dmg",
                "Rusty-Business-windows-x64-setup.exe",
                "Rusty-Business-linux-x64.deb",
                "Rusty-Business-linux-x64.rpm",
                "Rusty-Business-linux-x64.tar.gz",
            ]
        );
        assert_eq!(
            Download::Deb.command().as_deref(),
            Some("sudo apt install ./Rusty-Business-linux-x64.deb")
        );
        assert_eq!(
            Download::Rpm.command().as_deref(),
            Some("sudo dnf install ./Rusty-Business-linux-x64.rpm")
        );
        for download in [Download::Mac, Download::Windows, Download::TarGz] {
            assert_eq!(download.command(), None, "{download:?}");
        }

        let packaged = Path::new("/usr/bin/rusty-business");
        assert_eq!(linux_download(packaged, true), Download::Deb);
        assert_eq!(linux_download(packaged, false), Download::Rpm);
        for elsewhere in [
            "/home/me/Rusty-Business-linux-x64/bin/rusty-business",
            "/usr/local/bin/rusty-business",
            "/opt/rusty-business",
        ] {
            assert_eq!(
                linux_download(Path::new(elsewhere), true),
                Download::TarGz,
                "{elsewhere}"
            );
        }

        if cfg!(target_os = "macos") {
            assert_eq!(this_download(), Some(Download::Mac));
        }
    }

    #[test]
    fn the_check_asks_only_githubs_api_for_this_repositorys_newest_release() {
        assert_eq!(
            latest_url(),
            "https://api.github.com/repos/Bmorrical/rusty-business/releases/latest"
        );
        let url = reqwest::Url::parse(&latest_url()).unwrap();
        assert!(https_on(&url, API_HOST));
        for away in [
            "http://api.github.com/repos/Bmorrical/rusty-business/releases/latest",
            "https://api.github.com:8443/x",
            "https://someone@api.github.com/x",
            "https://api.github.com.example.com/x",
            "https://evilapi.github.com/x",
            "https://github.com/x",
            "https://140.82.112.6/x",
        ] {
            assert!(
                !https_on(&reqwest::Url::parse(away).unwrap(), API_HOST),
                "{away}"
            );
        }
    }

    #[test]
    fn a_redirect_is_never_followed() {
        // A stand-in for GitHub on this computer that answers with a redirect.
        // The client is the check's own, but for plain http, which only a
        // stand-in here can give.
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let _ = stream.read(&mut request);
            stream
                .write_all(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/elsewhere\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let error = block_on(ask(client, &format!("http://127.0.0.1:{port}/repos"))).unwrap_err();
        // Following the redirect would have failed to connect. Instead the
        // answer itself came back, and its address, not GitHub's API over
        // https, was refused by the check once more.
        assert_eq!(error, "GitHub answered from somewhere else.");
        served.join().unwrap();

        // The check's own client refuses plain http before anything is sent:
        // a stand-in listening for it is never reached.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let error = block_on(ask(
            client_for_test(),
            &format!("http://127.0.0.1:{port}/repos"),
        ))
        .unwrap_err();
        assert_eq!(error, "The app could not reach GitHub.");
        listener.set_nonblocking(true).unwrap();
        assert!(
            listener.accept().is_err(),
            "the check reached the stand-in over plain http"
        );
    }

    fn client_for_test() -> reqwest::Client {
        client(&Gate::at(true)).unwrap()
    }

    #[test]
    fn an_answer_is_read_up_to_its_size_and_no_further() {
        let chunks = |sizes: &[usize]| {
            futures_util::stream::iter(
                sizes
                    .iter()
                    .map(|size| Ok(vec![b'x'; *size]))
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            block_on(read_capped(chunks(&[400, 600]), 1000))
                .unwrap()
                .len(),
            1000
        );
        assert_eq!(
            block_on(read_capped(chunks(&[400, 601]), 1000)).unwrap_err(),
            "GitHub's answer was too large."
        );
        let broken = futures_util::stream::iter(vec![Ok(vec![b'x']), Err("cut off".to_string())]);
        assert_eq!(block_on(read_capped(broken, 1000)).unwrap_err(), "cut off");
        // GitHub's answer for a release of ten files is about 24 KB; the check
        // reads a megabyte at most.
        assert_eq!(MOST_BYTES, 1_000_000);
    }

    #[test]
    fn only_this_repositorys_releases_are_opened() {
        let found =
            update_from("0.4.0", &answer("v0.5.0", &every_file()), Download::Windows).unwrap();
        for part in [Part::Download, Part::Notes] {
            assert!(on_releases(&address(&found, part).unwrap()), "{part:?}");
        }
        for away in [
            "https://github.com/Bmorrical/rusty-business",
            "https://github.com/Bmorrical/rusty-business/issues/1",
            "https://github.com/Bmorrical/other/releases/tag/v0.5.0",
            "https://github.com/someone/rusty-business/releases/tag/v0.5.0",
            "https://github.com.example.com/Bmorrical/rusty-business/releases/tag/v0.5.0",
            "http://github.com/Bmorrical/rusty-business/releases/tag/v0.5.0",
            "https://github.com:8443/Bmorrical/rusty-business/releases/tag/v0.5.0",
            "https://someone@github.com/Bmorrical/rusty-business/releases/tag/v0.5.0",
            "https://github.com/Bmorrical/rusty-business/releases/tag/v0.5.0?x=1",
            "https://github.com/Bmorrical/rusty-business/releases/tag/v0.5.0#x",
            "https://api.github.com/repos/Bmorrical/rusty-business/releases/latest",
        ] {
            assert!(!on_releases(&reqwest::Url::parse(away).unwrap()), "{away}");
        }
        // An address built from a bent file name is checked and refused too.
        for bent in ["../../../../someone/else", "x?y=1"] {
            let mut odd =
                update_from("0.4.0", &answer("v0.5.0", &every_file()), Download::Windows).unwrap();
            odd.file = bent.to_string();
            assert_eq!(
                address(&odd, Part::Download).unwrap_err(),
                COULD_NOT_OPEN,
                "{bent}"
            );
        }
        // The page names a part, never an address.
        assert_eq!(
            serde_json::from_str::<Part>(r#""download""#).unwrap(),
            Part::Download
        );
        assert_eq!(
            serde_json::from_str::<Part>(r#""notes""#).unwrap(),
            Part::Notes
        );
        assert!(serde_json::from_str::<Part>(r#""https://example.com""#).is_err());

        let opens = browser(&address(&found, Part::Download).unwrap());
        let last = opens
            .get_args()
            .last()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(
            last,
            "https://github.com/Bmorrical/rusty-business/releases/download/v0.5.0/Rusty-Business-windows-x64-setup.exe"
        );
        if cfg!(target_os = "macos") {
            assert_eq!(opens.get_program(), "/usr/bin/open");
        }
    }

    #[test]
    fn a_check_that_fails_says_why_and_keeps_what_the_last_one_found() {
        let updates = Updates::default();
        let found = update_from("0.4.0", &answer("v0.5.0", &every_file()), Download::Mac);
        assert!(found.is_some());
        assert_eq!(updates.keep(Ok(found.clone())), Ok(found.clone()));
        // The Check for updates button shows why a check failed.
        assert_eq!(
            updates.keep(Err("The app could not reach GitHub.".into())),
            Err("The app could not reach GitHub.".to_string())
        );
        assert_eq!(updates.found(), found);
        // A check that finds nothing new clears it.
        assert_eq!(updates.keep(Ok(None)), Ok(None));
        assert_eq!(updates.found(), None);
    }

    // Brad, 2026-11-17: "off by default, everything stops", and "for update,
    // it should tell them to enable the gate to check for updates."
    #[test]
    fn with_the_gate_off_nothing_is_asked_and_the_check_says_to_turn_it_on() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let error = block_on(ask_through(
            &Gate::at(false),
            &format!("https://127.0.0.1:{port}/repos"),
        ))
        .unwrap_err();
        assert_eq!(error, "The gate is off. Turn it on to check for updates.");
        assert_eq!(error, GATE_OFF);
        listener.set_nonblocking(true).unwrap();
        assert!(
            listener.accept().is_err(),
            "the check went out with the gate off"
        );
        assert!(GATE_OFF.contains("Turn it on") && GATE_OFF.contains("check for updates"));

        // A version found while the gate was on is not opened once it is off.
        let updates = Updates::default();
        updates
            .keep(Ok(update_from(
                "0.4.0",
                &answer("v0.5.0", &every_file()),
                Download::Mac,
            )))
            .unwrap();
        for part in [Part::Download, Part::Notes] {
            assert_eq!(
                to_open(part, &updates, &Gate::at(false)).unwrap_err(),
                "The gate is off. Turn it on to open the download."
            );
            assert!(
                on_releases(&to_open(part, &updates, &Gate::at(true)).unwrap()),
                "{part:?}"
            );
        }
        assert_eq!(
            to_open(Part::Notes, &Updates::default(), &Gate::at(true)).unwrap_err(),
            "No new version has been found."
        );
    }

    #[test]
    fn turning_the_gate_off_stops_a_check_under_way() {
        // A stand-in that takes the connection and never answers.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let held = std::thread::spawn(move || listener.accept().map(|(stream, _)| stream));
        let gate = Gate::at(true);
        let switch = gate.clone();
        let started = std::time::Instant::now();
        let turned_off = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            let _ = switch.set(false);
        });
        let error = block_on(ask_through(
            &gate,
            &format!("https://127.0.0.1:{port}/repos"),
        ))
        .unwrap_err();
        assert_eq!(error, GATE_OFF);
        assert!(
            started.elapsed() < CONNECT_WAIT,
            "the check ran on after the gate was turned off"
        );
        turned_off.join().unwrap();
        drop(held.join().unwrap());
    }

    #[test]
    fn too_many_checks_are_said_plainly() {
        let busy = "GitHub has had too many checks from this network for now. Try again later.";
        assert_eq!(status_problem(403), busy);
        assert_eq!(status_problem(429), busy);
        assert_eq!(status_problem(404), "GitHub answered 404.");
        assert_eq!(status_problem(500), "GitHub answered 500.");
    }

    #[test]
    #[ignore = "asks GitHub itself; run with --ignored"]
    fn github_answers_the_check() {
        let answer = block_on(ask(client_for_test(), &latest_url())).unwrap();
        let found =
            update_from("0.0.1", &answer, Download::Mac).expect("the newest release has the .dmg");
        assert!(version(&found.version).unwrap() > [0, 0, 1]);
        assert_eq!(found.file, MAC);
    }
}
