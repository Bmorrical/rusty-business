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
// Then Install and restart: "when can we do an in app install rather than a
// new dmg download? is that after the signing? if we can do it now, then
// roll that next". Where this copy came from the `.dmg`, the `.exe`, the
// `.deb`, or the `.rpm`, the notice's first button installs the new version
// and opens it, and Download stays beside it.
//
// The rules, each with a test:
//
// - Only GitHub's API is asked, over https, and only for this repository's
//   newest release. The check follows no redirect, and no proxy, account,
//   token, or cookie is used. The answer is read up to a size, and no
//   further.
// - A release is a new version only when its tag is a plain version newer
//   than this app's, it is neither a draft nor a pre-release, and it holds
//   this system's file.
// - The page gives no address. It asks for the download or the notes of the
//   version Rust found, and Rust opens only this repository's releases on
//   github.com.
// - A check that fails keeps what the last one found, and says why. The
//   Check for updates button shows why; the other checks let it pass, and
//   the next one tries again.
// - Nothing is downloaded until the person clicks Install and restart. Then
//   the app fetches two files of the release it found, from this
//   repository's releases on github.com: its `latest.json` and this system's
//   file. It follows a redirect only to GitHub's own file hosts, over https,
//   and reads each file up to a size, and no further.
// - The file is installed only when its signature matches the public key in
//   tauri.conf.json and names the version found, and when `latest.json`
//   names that version and that file. Otherwise nothing is installed, and
//   the notice says why.
// - It installs only over a copy put in place from a download: the `.app`
//   from the `.dmg`, the `.exe`'s install, the `.deb`'s, or the `.rpm`'s. A
//   copy run from the `.dmg` itself, one macOS moved aside to open, a
//   development build, or one unpacked from the `.tar.gz` keeps Download, and
//   the person installs the file, as the first time.
// - One install at a time. When it is done, the app stops the engine and
//   the stack it started, as when it quits, and opens the new version.
// - Nothing is asked while the outbound gate is off ("off by default,
//   everything stops"), and turning it off stops a check under way. Check for
//   updates then says so: "for update, it should tell them to enable the gate
//   to check for updates." Download, Release notes, and Install and restart
//   wait for it too, and turning it off stops an install's download.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tauri::utils::config::BundleType;
use tauri_plugin_updater::UpdaterExt;

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

/// The release's file that names each system's file and its signature.
const MANIFEST: &str = "latest.json";

/// Where a release's files come from: github.com sends each one on to
/// GitHub's own file host, which was `objects.githubusercontent.com` before
/// `release-assets.githubusercontent.com`.
const FILE_HOSTS: [&str; 3] = [
    SITE_HOST,
    "release-assets.githubusercontent.com",
    "objects.githubusercontent.com",
];

/// The most redirects a file's download follows.
const MOST_HOPS: usize = 3;

/// The most of a new version's file the app reads. The `.dmg` for 0.6.1 was
/// 195 MB.
const MOST_PACKAGE_BYTES: usize = 1_000_000_000;

/// How long a file's download may wait for its next bytes.
const READ_WAIT: Duration = Duration::from_secs(60);

/// Where a `.deb` install is listed. A `.rpm` install has no such file.
const DPKG_LIST: &str = "/var/lib/dpkg/info/rusty-business.list";

/// Where the `.deb` and the `.rpm` put the program.
const PACKAGED_PROGRAM: &str = "/usr/bin/rusty-business";

const COULD_NOT_OPEN: &str = "The app could not open your browser.";

const COULD_NOT_REACH: &str = "The app could not reach GitHub.";

const NOT_FOUND: &str = "No new version has been found.";

/// What Install and restart says when this copy cannot install over itself.
const NO_INSTALL: &str = "This copy does not install new versions itself. Use Download.";

const BEING_INSTALLED: &str = "The new version is being installed.";

const BAD_MANIFEST: &str =
    "The new version's latest.json was not as expected, so nothing was installed.";

const BAD_SIGNATURE: &str = "The new version's signature did not match, so nothing was installed.";

const SENT_ELSEWHERE: &str = "GitHub sent the file somewhere else, so the app stopped.";

/// The address the check asks.
fn latest_url() -> String {
    format!("https://{API_HOST}/repos/{REPOSITORY}/releases/latest")
}

/// This repository's releases on GitHub, with no scheme, for a person to read.
fn releases_page() -> String {
    format!("{SITE_HOST}/{REPOSITORY}/releases")
}

/// A file of one release, on this repository's releases on github.com.
fn release_address(version: &str, file: &str) -> String {
    format!("https://{SITE_HOST}/{REPOSITORY}/releases/download/v{version}/{file}")
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

    /// The file Install and restart puts in place, and its name in
    /// `latest.json`. The Mac's is the `.app` alone, packed as a `.tar.gz`.
    /// The `.tar.gz` for Linux has none: it is unpacked by hand.
    fn package(self) -> Option<Package> {
        let (file, target) = match self {
            Download::Mac => ("Rusty-Business-mac-arm64.app.tar.gz", "darwin-aarch64"),
            Download::Windows => ("Rusty-Business-windows-x64-setup.exe", "windows-x86_64"),
            Download::Deb => ("Rusty-Business-linux-x64.deb", "linux-x86_64-deb"),
            Download::Rpm => ("Rusty-Business-linux-x64.rpm", "linux-x86_64-rpm"),
            Download::TarGz => return None,
        };
        Some(Package { file, target })
    }
}

/// What Install and restart fetches for this system.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Package {
    /// The release's file.
    file: &'static str,
    /// Its name among `latest.json`'s platforms.
    target: &'static str,
}

/// True when this copy can be installed over: it came from the download
/// whose package Install and restart puts in place. `bundle` is what the
/// build marked the program as, and `program` is where it runs from.
fn installs_over(download: Download, bundle: Option<BundleType>, program: &Path) -> bool {
    match download {
        Download::Mac => bundle == Some(BundleType::App) && put_in_place(program),
        Download::Windows => bundle == Some(BundleType::Nsis),
        Download::Deb => bundle == Some(BundleType::Deb),
        Download::Rpm => bundle == Some(BundleType::Rpm),
        Download::TarGz => false,
    }
}

/// True for the program inside an `.app` the person put in place. Not one
/// opened from the `.dmg` itself, or from another disk under `/Volumes`, and
/// not one macOS moved aside to open (App Translocation): those cannot be
/// replaced.
fn put_in_place(program: &Path) -> bool {
    let mut above = program.ancestors().skip(1);
    let (Some(macos), Some(contents), Some(app)) = (above.next(), above.next(), above.next())
    else {
        return false;
    };
    macos.file_name() == Some("MacOS".as_ref())
        && contents.file_name() == Some("Contents".as_ref())
        && app.extension() == Some("app".as_ref())
        && !program.starts_with("/Volumes")
        && !program
            .components()
            .any(|part| part.as_os_str() == "AppTranslocation")
}

/// True when this copy can be installed over.
fn this_installs(download: Download) -> bool {
    std::env::current_exe().is_ok_and(|program| {
        installs_over(download, tauri::utils::platform::bundle_type(), &program)
    })
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
    /// True when Install and restart can put it in place of this copy.
    install: bool,
}

/// True when the release holds `name`, all of it uploaded.
fn holds(release: &Release, name: &str) -> bool {
    release
        .assets
        .iter()
        .any(|asset| asset.name == name && asset.state == "uploaded")
}

/// The new version in GitHub's answer, or none. `current` is this app's
/// version. It installs when the release holds this system's package and
/// `latest.json`; whether this copy can be installed over is checked apart.
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
    if !holds(&release, file) {
        return None;
    }
    let install = download
        .package()
        .is_some_and(|package| holds(&release, package.file) && holds(&release, MANIFEST));
    Some(Update {
        version: written(newest),
        current: current.to_string(),
        file: file.to_string(),
        steps: download.steps().to_string(),
        command: download.command(),
        page: format!("{}/tag/v{}", releases_page(), written(newest)),
        install,
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
const GATE_OFF: &str = "The WAN gate is off. Turn it on to check for updates.";

/// What Download and Release notes say while the outbound gate is off.
const GATE_OFF_OPEN: &str = "The WAN gate is off. Turn it on to open the download.";

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
        .map_err(|_| COULD_NOT_REACH.to_string())?;
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
/// address, and whether it is being installed.
#[derive(Default)]
pub struct Updates {
    found: Mutex<Option<Update>>,
    installing: AtomicBool,
}

/// An install under way. When it ends, however it ends, another may start.
struct Installing<'a>(&'a AtomicBool);

impl Drop for Installing<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Updates {
    /// Keeps what a check found. A check that failed keeps what the last one
    /// found, and gives back why it failed.
    fn keep(&self, checked: Result<Option<Update>, String>) -> Result<Option<Update>, String> {
        let update = checked?;
        let mut found = self
            .found
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *found = update.clone();
        Ok(update)
    }

    fn found(&self) -> Option<Update> {
        self.found
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Starts an install, unless one is under way.
    fn start_install(&self) -> Option<Installing<'_>> {
        self.installing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| Installing(&self.installing))
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
    let checked = ask_through(&gate, &latest_url()).await.map(|answer| {
        update_from(env!("CARGO_PKG_VERSION"), &answer, download).map(|mut update| {
            update.install &= this_installs(download);
            update
        })
    });
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
        Part::Download => release_address(&update.version, &update.file),
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
    let update = updates.found().ok_or_else(|| NOT_FOUND.to_string())?;
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

// ---------------------------------------------------------------------------
// Install and restart
// ---------------------------------------------------------------------------

/// What Install and restart says while the outbound gate is off.
const GATE_OFF_INSTALL: &str = "The WAN gate is off. Turn it on to install the new version.";

/// What it says when the gate is turned off during the download.
const GATE_TURNED_OFF: &str = "The WAN gate was turned off, so nothing was installed.";

/// The address of one of the new version's files, checked as Download's is.
fn release_file(update: &Update, file: &str) -> Result<reqwest::Url, String> {
    reqwest::Url::parse(&release_address(&update.version, file))
        .ok()
        .filter(on_releases)
        .ok_or_else(|| BAD_MANIFEST.to_string())
}

/// True for an https address on github.com or GitHub's own file hosts, with
/// no account in it and no port of its own. A file host's address carries a
/// signed query, which is let through.
fn from_github(url: &reqwest::Url) -> bool {
    FILE_HOSTS.iter().any(|host| https_on(url, host))
}

/// True when a file's download may follow a redirect to `next`, having
/// followed `followed` already.
fn may_follow(next: &reqwest::Url, followed: usize) -> bool {
    followed < MOST_HOPS && from_github(next)
}

/// Redirects for a file's download: only to GitHub's own hosts, and only a
/// few.
fn file_redirects() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if may_follow(attempt.url(), attempt.previous().len()) {
            attempt.follow()
        } else {
            attempt.error("sent off GitHub")
        }
    })
}

/// The client for the new version's files, through the outbound gate. It
/// sends no account, token, or cookie.
fn file_client(gate: &Gate) -> Result<reqwest::Client, String> {
    gate.client(GATE_OFF_INSTALL, |builder| {
        builder
            .no_proxy()
            .https_only(true)
            .redirect(file_redirects())
            .referer(false)
            .user_agent(concat!("rusty-business/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(CONNECT_WAIT)
            .read_timeout(READ_WAIT)
    })
}

/// One of the new version's files, read up to `most` bytes and no further.
async fn fetch(client: reqwest::Client, url: reqwest::Url, most: usize) -> Result<Vec<u8>, String> {
    let response = client.get(url).send().await.map_err(|error| {
        if error.is_redirect() {
            SENT_ELSEWHERE.to_string()
        } else {
            COULD_NOT_REACH.to_string()
        }
    })?;
    // Where the file came from, checked once more.
    if !from_github(response.url()) {
        return Err(SENT_ELSEWHERE.to_string());
    }
    if !response.status().is_success() {
        return Err(format!("GitHub answered {}.", response.status().as_u16()));
    }
    let chunks = response.bytes_stream().map(|chunk| {
        chunk
            .map(|bytes| bytes.to_vec())
            .map_err(|_| COULD_NOT_REACH.to_string())
    });
    read_capped(chunks, most).await
}

/// `work`, while the outbound gate stays on. Turning it off stops it where it
/// is.
async fn until_turned_off<T>(
    gate: &Gate,
    work: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::select! {
        done = work => done,
        () = gate.closed() => Err(GATE_TURNED_OFF.to_string()),
    }
}

/// One of the new version's files, while the outbound gate stays on.
async fn fetch_through(gate: &Gate, url: reqwest::Url, most: usize) -> Result<Vec<u8>, String> {
    let client = file_client(gate)?;
    until_turned_off(gate, fetch(client, url, most)).await
}

/// The new version to install, and its file for this system: only while the
/// gate is on, once a version has been found, and when this copy installs
/// over itself.
fn to_install(
    updates: &Updates,
    gate: &Gate,
    download: Option<Download>,
) -> Result<(Update, Package), String> {
    if !gate.is_open() {
        return Err(GATE_OFF_INSTALL.to_string());
    }
    let update = updates.found().ok_or_else(|| NOT_FOUND.to_string())?;
    let package = download
        .and_then(Download::package)
        .filter(|_| update.install)
        .ok_or_else(|| NO_INSTALL.to_string())?;
    Ok((update, package))
}

/// True when `latest.json` names the version found and this system's file on
/// its release, and nothing else.
fn signed_matches(version: &str, url: &str, update: &Update, package: Package) -> bool {
    version == update.version
        && release_file(update, package.file).is_ok_and(|expected| expected.as_str() == url)
}

fn could_not_install(error: &impl std::fmt::Display) -> String {
    format!("The new version could not be installed ({error}). Download still works.")
}

/// Fetches the new version's `latest.json` and this system's file, checks the
/// file's signature, installs it, and opens the new version. The page names
/// nothing; Rust installs only what the last check found.
#[tauri::command]
pub async fn install_update(
    app: tauri::AppHandle,
    updates: tauri::State<'_, Updates>,
    gate: tauri::State<'_, Gate>,
) -> Result<(), String> {
    let (update, package) = to_install(&updates, &gate, this_download())?;
    let _installing = updates
        .start_install()
        .ok_or_else(|| BEING_INSTALLED.to_string())?;

    let manifest_url = release_file(&update, MANIFEST)?;
    let manifest = fetch_through(&gate, manifest_url.clone(), MOST_BYTES).await?;
    let manifest: serde_json::Value =
        serde_json::from_slice(&manifest).map_err(|_| BAD_MANIFEST.to_string())?;

    // On Windows the installer closes the app itself, so the engine and the
    // stack are stopped first, as when it quits.
    let stopping = app.clone();
    let updater = app
        .updater_builder()
        .endpoints(vec![manifest_url])
        .map_err(|error| could_not_install(&error))?
        .target(package.target)
        .on_before_exit(move || {
            crate::stop_everything(&stopping);
            stopping.cleanup_before_exit();
        })
        .build()
        .map_err(|error| could_not_install(&error))?;
    let signed = updater
        .restore_update(manifest)
        .ok()
        .flatten()
        .filter(|signed| {
            signed_matches(
                &signed.version,
                signed.download_url.as_str(),
                &update,
                package,
            )
        })
        .ok_or_else(|| BAD_MANIFEST.to_string())?;

    let file = fetch_through(
        &gate,
        release_file(&update, package.file)?,
        MOST_PACKAGE_BYTES,
    )
    .await?;
    signed
        .verify(&file)
        .map_err(|_| BAD_SIGNATURE.to_string())?;
    tauri::async_runtime::spawn_blocking(move || signed.install(file))
        .await
        .map_err(|error| could_not_install(&error))?
        .map_err(|error| could_not_install(&error))?;
    app.request_restart();
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
        .chain([MAC_PACKAGE, MANIFEST])
        .collect()
    }

    const MAC_PACKAGE: &str = "Rusty-Business-mac-arm64.app.tar.gz";

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
                install: true,
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
            [
                "command", "current", "file", "install", "page", "steps", "version"
            ]
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
        assert_eq!(
            error,
            "The WAN gate is off. Turn it on to check for updates."
        );
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
                "The WAN gate is off. Turn it on to open the download."
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

    // Brad, 2026-11-19: "when can we do an in app install rather than a new
    // dmg download? is that after the signing? if we can do it now, then
    // roll that next". He chose "Click to install", and ".deb and .rpm" with
    // the Mac and Windows.
    #[test]
    fn a_release_with_this_systems_package_and_latest_json_installs() {
        for download in [
            Download::Mac,
            Download::Windows,
            Download::Deb,
            Download::Rpm,
        ] {
            let found = update_from("0.4.0", &answer("v0.5.0", &every_file()), download).unwrap();
            assert!(found.install, "{download:?}");
            // Download stays beside it.
            assert_eq!(found.file, download.file());
        }
        let packages: Vec<_> = [
            Download::Mac,
            Download::Windows,
            Download::Deb,
            Download::Rpm,
        ]
        .iter()
        .map(|download| download.package().unwrap())
        .collect();
        assert_eq!(
            packages,
            [
                Package {
                    file: MAC_PACKAGE,
                    target: "darwin-aarch64"
                },
                Package {
                    file: "Rusty-Business-windows-x64-setup.exe",
                    target: "windows-x86_64"
                },
                Package {
                    file: "Rusty-Business-linux-x64.deb",
                    target: "linux-x86_64-deb"
                },
                Package {
                    file: "Rusty-Business-linux-x64.rpm",
                    target: "linux-x86_64-rpm"
                },
            ]
        );
        // The .tar.gz is unpacked by hand, so it keeps Download alone.
        assert_eq!(Download::TarGz.package(), None);
        let found =
            update_from("0.4.0", &answer("v0.5.0", &every_file()), Download::TarGz).unwrap();
        assert!(!found.install);

        // A release without the package or latest.json, as every release
        // before 0.8.0, still shows its notice, with Download alone.
        for missing in [MAC_PACKAGE, MANIFEST] {
            let files: Vec<_> = every_file()
                .into_iter()
                .filter(|name| *name != missing)
                .collect();
            let found = update_from("0.4.0", &answer("v0.5.0", &files), Download::Mac).unwrap();
            assert!(!found.install, "{missing}");
        }
        // One still on its way up is not there yet.
        let mut release: serde_json::Value =
            serde_json::from_slice(&answer("v0.5.0", &every_file())).unwrap();
        for asset in release["assets"].as_array_mut().unwrap() {
            if asset["name"] == MANIFEST {
                asset["state"] = "starter".into();
            }
        }
        let found = update_from("0.4.0", release.to_string().as_bytes(), Download::Mac).unwrap();
        assert!(!found.install);
    }

    /// The Mac app's file name, which carries no mark.
    const BUNDLE: &str = "Rusty Business.app";

    #[test]
    fn only_a_copy_put_in_place_installs_over_itself() {
        let folder = BUNDLE.trim_end_matches(".app");
        let placed = format!("/Applications/{BUNDLE}/Contents/MacOS/rusty-business");
        let placed = Path::new(&placed);
        assert!(installs_over(Download::Mac, Some(BundleType::App), placed));
        let home = format!("/Users/me/Applications/{BUNDLE}/Contents/MacOS/rusty-business");
        assert!(installs_over(
            Download::Mac,
            Some(BundleType::App),
            Path::new(&home)
        ));
        for elsewhere in [
            // Run from the .dmg itself.
            format!("/Volumes/{folder}/{BUNDLE}/Contents/MacOS/rusty-business"),
            // Moved aside by macOS to open.
            format!(
                "/private/var/folders/xy/abc/T/AppTranslocation/1234/d/{BUNDLE}/Contents/MacOS/rusty-business"
            ),
            // Not inside an .app.
            "/Users/me/rusty-business/target/release/rusty-business".to_string(),
            format!("/Applications/{folder}/Contents/MacOS/rusty-business"),
            format!("/Applications/{BUNDLE}/Contents/Resources/rusty-business"),
            "rusty-business".to_string(),
        ] {
            assert!(
                !installs_over(Download::Mac, Some(BundleType::App), Path::new(&elsewhere)),
                "{elsewhere}"
            );
        }
        // A development build is marked as none of the downloads.
        assert!(!installs_over(Download::Mac, None, placed));
        assert!(!installs_over(Download::Mac, Some(BundleType::Dmg), placed));

        let program = Path::new("/usr/bin/rusty-business");
        assert!(installs_over(
            Download::Windows,
            Some(BundleType::Nsis),
            program
        ));
        assert!(installs_over(Download::Deb, Some(BundleType::Deb), program));
        assert!(installs_over(Download::Rpm, Some(BundleType::Rpm), program));
        assert!(!installs_over(
            Download::Windows,
            Some(BundleType::Msi),
            program
        ));
        assert!(!installs_over(
            Download::Deb,
            Some(BundleType::Rpm),
            program
        ));
        assert!(!installs_over(
            Download::Rpm,
            Some(BundleType::Deb),
            program
        ));
        for download in [Download::Windows, Download::Deb, Download::Rpm] {
            assert!(!installs_over(download, None, program), "{download:?}");
        }
        for bundle in [None, Some(BundleType::AppImage), Some(BundleType::Deb)] {
            assert!(
                !installs_over(Download::TarGz, bundle.clone(), program),
                "{bundle:?}"
            );
        }

        // These tests run as a development build, which keeps Download.
        for download in [
            Download::Mac,
            Download::Windows,
            Download::Deb,
            Download::Rpm,
        ] {
            assert!(!this_installs(download), "{download:?}");
        }
    }

    #[test]
    fn install_waits_for_the_gate_a_version_found_and_a_copy_that_installs() {
        let updates = Updates::default();
        let found = update_from("0.4.0", &answer("v0.5.0", &every_file()), Download::Mac);
        updates.keep(Ok(found.clone())).unwrap();
        assert_eq!(
            to_install(&updates, &Gate::at(false), Some(Download::Mac)).unwrap_err(),
            "The WAN gate is off. Turn it on to install the new version."
        );
        assert_eq!(
            to_install(&Updates::default(), &Gate::at(true), Some(Download::Mac)).unwrap_err(),
            NOT_FOUND
        );
        let (update, package) = to_install(&updates, &Gate::at(true), Some(Download::Mac)).unwrap();
        assert_eq!(Some(update), found);
        assert_eq!(package, Download::Mac.package().unwrap());
        for download in [None, Some(Download::TarGz)] {
            assert_eq!(
                to_install(&updates, &Gate::at(true), download).unwrap_err(),
                "This copy does not install new versions itself. Use Download.",
                "{download:?}"
            );
        }
        // A copy that cannot install over itself was found with install off.
        let mut kept = found.unwrap();
        kept.install = false;
        updates.keep(Ok(Some(kept))).unwrap();
        assert_eq!(
            to_install(&updates, &Gate::at(true), Some(Download::Mac)).unwrap_err(),
            NO_INSTALL
        );
    }

    #[test]
    fn one_install_at_a_time() {
        let updates = Updates::default();
        let first = updates.start_install();
        assert!(first.is_some());
        assert!(updates.start_install().is_none());
        drop(first);
        // However it ended, another may start.
        assert!(updates.start_install().is_some());
        assert_eq!(BEING_INSTALLED, "The new version is being installed.");
    }

    #[test]
    fn the_files_come_only_from_the_release_found() {
        let found = update_from("0.4.0", &answer("v0.5.0", &every_file()), Download::Mac).unwrap();
        let package = Download::Mac.package().unwrap();
        assert_eq!(
            release_file(&found, MANIFEST).unwrap().as_str(),
            "https://github.com/Bmorrical/rusty-business/releases/download/v0.5.0/latest.json"
        );
        let file = release_file(&found, package.file).unwrap();
        assert_eq!(
            file.as_str(),
            "https://github.com/Bmorrical/rusty-business/releases/download/v0.5.0/Rusty-Business-mac-arm64.app.tar.gz"
        );
        // latest.json must name the version found and this system's file
        // on its release, exactly.
        assert!(signed_matches("0.5.0", file.as_str(), &found, package));
        for (version, url) in [
            ("0.5.1", file.as_str()),
            ("0.4.0", file.as_str()),
            ("v0.5.0", file.as_str()),
            (
                "0.5.0",
                "https://github.com/Bmorrical/rusty-business/releases/download/v0.5.0/Rusty-Business-mac-arm64.dmg",
            ),
            (
                "0.5.0",
                "https://github.com/Bmorrical/rusty-business/releases/download/v0.4.0/Rusty-Business-mac-arm64.app.tar.gz",
            ),
            (
                "0.5.0",
                "https://github.com/someone/rusty-business/releases/download/v0.5.0/Rusty-Business-mac-arm64.app.tar.gz",
            ),
            (
                "0.5.0",
                "https://example.com/Rusty-Business-mac-arm64.app.tar.gz",
            ),
            (
                "0.5.0",
                "http://github.com/Bmorrical/rusty-business/releases/download/v0.5.0/Rusty-Business-mac-arm64.app.tar.gz",
            ),
            (
                "0.5.0",
                "https://github.com/Bmorrical/rusty-business/releases/download/v0.5.0/Rusty-Business-mac-arm64.app.tar.gz?x=1",
            ),
        ] {
            assert!(
                !signed_matches(version, url, &found, package),
                "{version} {url}"
            );
        }
        // A bent version is refused before anything is asked.
        let mut odd = found.clone();
        odd.version = "0.5.0/../../../../someone/else".into();
        assert_eq!(release_file(&odd, MANIFEST).unwrap_err(), BAD_MANIFEST);
    }

    #[test]
    fn a_file_follows_github_and_nothing_else() {
        for from in [
            "https://github.com/Bmorrical/rusty-business/releases/download/v0.5.0/latest.json",
            "https://release-assets.githubusercontent.com/github-production-release-asset/1/2?sp=r&sig=x",
            "https://objects.githubusercontent.com/github-production-release-asset-2e65be/1/2?X-Amz-Signature=x",
        ] {
            let url = reqwest::Url::parse(from).unwrap();
            assert!(from_github(&url), "{from}");
            assert!(
                may_follow(&url, 0) && may_follow(&url, MOST_HOPS - 1),
                "{from}"
            );
            assert!(!may_follow(&url, MOST_HOPS), "{from}");
        }
        for away in [
            "http://release-assets.githubusercontent.com/x",
            "https://release-assets.githubusercontent.com:8443/x",
            "https://someone@objects.githubusercontent.com/x",
            "https://githubusercontent.com/x",
            "https://raw.githubusercontent.com/x",
            "https://release-assets.githubusercontent.com.example.com/x",
            "https://example.com/latest.json",
            "https://127.0.0.1/x",
        ] {
            let url = reqwest::Url::parse(away).unwrap();
            assert!(!from_github(&url), "{away}");
            assert!(!may_follow(&url, 0), "{away}");
        }
    }

    // A stand-in on this computer that answers once with `reply`.
    fn stand_in(reply: &'static [u8]) -> (u16, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let _ = stream.read(&mut request);
            stream.write_all(reply).unwrap();
        });
        (port, served)
    }

    #[test]
    fn a_file_sent_off_github_is_refused() {
        // The file client's own redirects, but over plain http, which only a
        // stand-in here can give.
        let client = || {
            reqwest::Client::builder()
                .no_proxy()
                .redirect(file_redirects())
                .build()
                .unwrap()
        };
        let (port, served) = stand_in(
            b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/elsewhere\r\nContent-Length: 0\r\n\r\n",
        );
        let url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}/latest.json")).unwrap();
        let error = block_on(fetch(client(), url, MOST_BYTES)).unwrap_err();
        assert_eq!(
            error,
            "GitHub sent the file somewhere else, so the app stopped."
        );
        served.join().unwrap();

        // A file that came from anywhere but GitHub is refused once more.
        let (port, served) = stand_in(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
        let url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}/latest.json")).unwrap();
        assert_eq!(
            block_on(fetch(client(), url, MOST_BYTES)).unwrap_err(),
            SENT_ELSEWHERE
        );
        served.join().unwrap();

        // The file client itself refuses plain http before anything is sent.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}/latest.json")).unwrap();
        let error = block_on(fetch(
            file_client(&Gate::at(true)).unwrap(),
            url,
            MOST_BYTES,
        ))
        .unwrap_err();
        assert_eq!(error, COULD_NOT_REACH);
        listener.set_nonblocking(true).unwrap();
        assert!(
            listener.accept().is_err(),
            "the install reached the stand-in over plain http"
        );
        // A package is read up to a gigabyte, and no further.
        assert_eq!(MOST_PACKAGE_BYTES, 1_000_000_000);
    }

    // Brad, 2026-11-17: "off by default, everything stops".
    #[test]
    fn the_gate_stops_an_install_before_and_during_its_download() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = reqwest::Url::parse(&format!("https://127.0.0.1:{port}/latest.json")).unwrap();
        let error = block_on(fetch_through(&Gate::at(false), url, MOST_BYTES)).unwrap_err();
        assert_eq!(error, GATE_OFF_INSTALL);
        listener.set_nonblocking(true).unwrap();
        assert!(
            listener.accept().is_err(),
            "the install went out with the gate off"
        );

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
        let url = reqwest::Url::parse(&format!("https://127.0.0.1:{port}/x.app.tar.gz")).unwrap();
        let error = block_on(fetch_through(&gate, url, MOST_PACKAGE_BYTES)).unwrap_err();
        assert_eq!(
            error,
            "The WAN gate was turned off, so nothing was installed."
        );
        assert!(
            started.elapsed() < CONNECT_WAIT,
            "the download ran on after the gate was turned off"
        );
        turned_off.join().unwrap();
        drop(held.join().unwrap());
    }

    #[test]
    fn a_failed_install_says_why_and_that_download_still_works() {
        assert_eq!(
            could_not_install(&"the disk is full"),
            "The new version could not be installed (the disk is full). Download still works."
        );
        assert_eq!(
            BAD_SIGNATURE,
            "The new version's signature did not match, so nothing was installed."
        );
        assert_eq!(
            BAD_MANIFEST,
            "The new version's latest.json was not as expected, so nothing was installed."
        );
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
