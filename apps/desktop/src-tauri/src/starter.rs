//! Starting the engine.
//!
//! When no engine answers, the app starts one, so there is nothing to type in
//! a terminal first. On a Mac the engine's program is `mlx_lm.server`. The Mac
//! app that `make app` and `make dmg` build carries one inside it; otherwise
//! the app starts one that is already on this computer. It installs none.
//! On Windows and Linux the engine's program is llama.cpp's `llama-server`,
//! serving the GGUF models in the app's own models folder.
//!
//! Four rules live here:
//! - the whole plan is checked before anything starts: the program, a model
//!   for it to serve, the port, and a place for its log;
//! - only what is missing is started: an engine that answers is left alone,
//!   and so is anything else that holds the engine's port;
//! - asking twice changes nothing: there is one engine at most, however often
//!   the page asks;
//! - the app stops only an engine it started itself, and only when the app
//!   quits. One that was already running is never stopped or restarted.
//!   Stopping one stops what it started in turn, as `llama-server` starts a
//!   second program for each model it loads.
//!
//! An engine the app starts listens on this computer only, is told to stay
//! offline (so it downloads nothing and serves the models already on the
//! disk), and is never asked to log what is typed. `llama-server` is also
//! told to load one model at a time, and to keep its own chat page off.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::engine;

/// Names the engine's program outright, for one kept somewhere unusual.
pub const PROGRAM_VARIABLE: &str = "RUSTY_ENGINE_PROGRAM";

/// How long the app waits for an engine it started to answer.
const PATIENCE: Duration = Duration::from_secs(60);

/// How long it waits between one look and the next.
const PAUSE: Duration = Duration::from_millis(250);

/// How long an engine the app started is given to stop by itself when the app
/// quits, before it is made to.
const GRACE: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Which engine
// ---------------------------------------------------------------------------

/// The engine the app starts on this system.
#[derive(Clone, Debug, PartialEq)]
pub enum Engine {
    /// `mlx_lm.server`, on a Mac. It finds its models in the Hugging Face
    /// cache.
    Mlx,
    /// llama.cpp's `llama-server`, on Windows and Linux, serving the GGUF
    /// models in the app's own models folder, `models`.
    Llama { models: PathBuf },
}

impl Engine {
    /// The engine for `os`, one of Rust's names for a system, where `models`
    /// is the app's own models folder. Taking the system lets the Mac's tests
    /// check what Windows and Linux get. There is none on any other system,
    /// or on Windows and Linux without a models folder.
    pub fn on(os: &str, models: Option<PathBuf>) -> Option<Engine> {
        match os {
            "macos" => Some(Engine::Mlx),
            "windows" | "linux" => models.map(|models| Engine::Llama { models }),
            _ => None,
        }
    }

    /// The engine for this system.
    pub fn here() -> Option<Engine> {
        Engine::on(std::env::consts::OS, crate::models::models_folder())
    }

    /// The engine's program, by its file name on this system.
    pub fn program(&self) -> &'static str {
        self.program_on(std::env::consts::OS)
    }

    fn program_on(&self, os: &str) -> &'static str {
        match self {
            Engine::Mlx => "mlx_lm.server",
            Engine::Llama { .. } if os == "windows" => "llama-server.exe",
            Engine::Llama { .. } => "llama-server",
        }
    }

    /// How this engine's program is started.
    fn command(&self, program: &Path, address: &Address) -> Command {
        match self {
            Engine::Mlx => command(program, address),
            Engine::Llama { models } => llama_command(program, address, models, std::env::vars_os().map(|(name, _)| name)),
        }
    }
}

// ---------------------------------------------------------------------------
// Where the engine's program is
// ---------------------------------------------------------------------------

/// The engine's program on this computer, or none.
///
/// A program named by `RUSTY_ENGINE_PROGRAM` is the only one tried: the app
/// does not fall back to another than the one it was told to use. Otherwise
/// the engine the app carries comes first, when it carries one, then the
/// first one found in `places`.
///
/// `variable` reads one environment variable, and `carried` is the folder of
/// the engine the app carries (see `carried_folder`).
pub fn find_program(name: &str, variable: impl Fn(&str) -> Option<String>, home: Option<&Path>, carried: Option<&Path>) -> Option<PathBuf> {
    let set = |key: &str| variable(key).map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
    if let Some(named) = set(PROGRAM_VARIABLE) {
        let named = PathBuf::from(named);
        return (named.is_absolute() && can_run(&named)).then_some(named);
    }
    carried
        .map(Path::to_path_buf)
        .into_iter()
        .chain(places(set("PATH").as_deref(), home))
        .map(|folder| folder.join(name))
        .find(|candidate| can_run(candidate))
}

/// Where the engine the app carries is, when the app runs as a Mac app:
/// `Contents/Resources/engine/bin`, beside the `Contents/MacOS` folder that
/// holds the app's own program, `exe`. `make app` and `make dmg` put it there.
/// None when the app runs any other way, as it does from `npm run tauri dev`.
pub fn carried_folder(exe: &Path) -> Option<PathBuf> {
    let programs = exe.parent()?;
    let contents = programs.parent()?;
    (programs.file_name()? == "MacOS" && contents.file_name()? == "Contents").then(|| contents.join("Resources").join("engine").join("bin"))
}

/// The folders the program is looked for in on this system, in order.
pub fn places(path: Option<&str>, home: Option<&Path>) -> Vec<PathBuf> {
    places_on(std::env::consts::OS, path, home)
}

/// The folders the program is looked for in on `os`, in order. First, each
/// folder on `PATH` that is a full path. Then, on a Mac, `.local/bin` in the
/// home folder, Homebrew's two folders, and the folders `pip install --user`
/// fills, newest Python first: an app opened from the Finder gets a short
/// `PATH`. On Linux, `.local/bin`, Homebrew's folder, `/usr/local/bin`, and
/// `/usr/bin`. On Windows, `PATH` alone.
fn places_on(os: &str, path: Option<&str>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = match path {
        Some(path) => std::env::split_paths(path).filter(|folder| folder.is_absolute()).collect(),
        None => Vec::new(),
    };
    match os {
        "macos" => {
            folders.extend(home.map(|home| home.join(".local").join("bin")));
            folders.push(PathBuf::from("/opt/homebrew/bin"));
            folders.push(PathBuf::from("/usr/local/bin"));
            if let Some(home) = home {
                folders.extend(pip_folders(&home.join("Library").join("Python")));
            }
        }
        "linux" => {
            folders.extend(home.map(|home| home.join(".local").join("bin")));
            folders.push(PathBuf::from("/home/linuxbrew/.linuxbrew/bin"));
            folders.push(PathBuf::from("/usr/local/bin"));
            folders.push(PathBuf::from("/usr/bin"));
        }
        _ => {}
    }
    folders
}

/// `Library/Python/3.13/bin` and its like, newest Python first.
fn pip_folders(library: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(library) else {
        return Vec::new();
    };
    let mut versions: Vec<(Vec<u32>, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let numbers: Option<Vec<u32>> = name.to_str().map(|name| name.split('.').map(|part| part.parse().ok()).collect()).unwrap_or(None);
        if let Some(numbers) = numbers {
            versions.push((numbers, entry.path().join("bin")));
        }
    }
    versions.sort_by(|one, other| other.0.cmp(&one.0));
    versions.into_iter().map(|(_, folder)| folder).collect()
}

/// A file that can be run, or a link that leads to one.
pub(crate) fn can_run(path: &Path) -> bool {
    let Ok(found) = fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        found.is_file() && found.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        found.is_file()
    }
}

// ---------------------------------------------------------------------------
// Where the engine is started
// ---------------------------------------------------------------------------

/// The host and port an engine is started at.
#[derive(Clone, Debug, PartialEq)]
pub struct Address {
    pub host: String,
    pub port: u16,
}

/// The host and port in the engine's address. The address is checked again
/// here, so an engine is never started where another computer could reach it.
pub fn address_of(base: &str) -> Result<Address, String> {
    let checked = engine::checked_base_url(base)?;
    let url = reqwest::Url::parse(&checked).map_err(|_| "The engine address could not be read.".to_string())?;
    let host = url.host_str().unwrap_or_default().trim_start_matches('[').trim_end_matches(']').to_string();
    match url.port_or_known_default() {
        Some(port) if !host.is_empty() => Ok(Address { host, port }),
        _ => Err("The engine address could not be read.".to_string()),
    }
}

/// How the engine is started: at the engine's own address, and with no model
/// named, so nothing is loaded until a chat asks for a model.
pub fn command(program: &Path, address: &Address) -> Command {
    let mut command = Command::new(program);
    command.arg("--host").arg(&address.host);
    command.arg("--port").arg(address.port.to_string());
    // At this level the engine logs each request's line and its progress,
    // not the words. One level down it would log what is typed.
    command.arg("--log-level").arg("INFO");
    // Offline, the engine serves the models on the disk and downloads none.
    command.env("HF_HUB_OFFLINE", "1");
    command
}

/// How `llama-server` is started: at the engine's own address, serving the
/// models in `models`, with no model named, so nothing is loaded until a chat
/// asks for a model. `environment` is the names of the app's own environment
/// variables.
///
/// It is not told how much a model may take in at once: each model's own
/// setting is used, fitted to the memory there is. Nor is it told to log
/// more than it does by itself, which is each request's line, not the words.
pub fn llama_command(program: &Path, address: &Address, models: &Path, environment: impl IntoIterator<Item = OsString>) -> Command {
    let mut command = Command::new(program);
    command.arg("--host").arg(&address.host);
    command.arg("--port").arg(address.port.to_string());
    command.arg("--models-dir").arg(models);
    // One model loaded at a time, so two never fill the memory together.
    command.arg("--models-max").arg("1");
    // Offline, it serves the models on the disk and downloads none.
    command.arg("--offline");
    // The app's window is where a chat happens. The engine's own page stays off.
    command.arg("--no-webui");
    // `llama-server` reads settings from variables named `LLAMA_ARG_HOST` and
    // the like, and each model it loads gets the same variables. None of them
    // is passed on, so none can move the engine off this computer, or have it
    // keep what is typed.
    for name in llama_settings(environment) {
        command.env_remove(name);
    }
    command
}

/// The names in `names` that are settings for llama.cpp: each name that
/// starts `LLAMA_`, in any case, as Windows reads it.
fn llama_settings(names: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    names.into_iter().filter(|name| name.to_string_lossy().to_ascii_uppercase().starts_with("LLAMA_")).collect()
}

/// Sets the engine apart from the app. On Mac and Linux it runs in a process
/// group of its own, so `end` can reach everything it starts in turn. On
/// Windows it runs with no console window: the app's window is the only one.
fn apart(command: &mut Command) -> &mut Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    quiet(command)
}

/// On Windows, a program run with no console window of its own. Elsewhere,
/// the command as it is.
fn quiet(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW.
        command.creation_flags(0x0800_0000);
    }
    command
}

/// True when something accepts a connection at the engine's host and port.
fn someone_answers(address: &Address) -> bool {
    let Ok(places) = (address.host.as_str(), address.port).to_socket_addrs() else {
        return false;
    };
    places.into_iter().any(|place| TcpStream::connect_timeout(&place, Duration::from_millis(500)).is_ok())
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// What this computer has for starting an engine.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// The engine for this system, or none where there is none.
    pub engine: Option<Engine>,
    /// The engine's program, when it is on this computer.
    pub program: Option<PathBuf>,
    /// How many of the app's models are whole on the disk.
    pub models: usize,
}

/// Looks, fresh each time: the app keeps no note of what it found before.
pub fn look() -> Found {
    let engine = Engine::here();
    let home = std::env::home_dir();
    let carried = std::env::current_exe().ok().and_then(|exe| carried_folder(&exe));
    let program = engine.as_ref().and_then(|engine| find_program(engine.program(), |key| std::env::var(key).ok(), home.as_deref(), carried.as_deref()));
    Found { engine, program, models: crate::models::on_disk().len() }
}

/// Everything the plan is made from.
#[derive(Clone, Debug)]
pub struct Facts {
    /// The engine answers at its address.
    pub answering: bool,
    /// An engine this app started is still running.
    pub ours: bool,
    /// Something accepts connections at the engine's port.
    pub port_taken: bool,
    pub found: Found,
}

/// Why nothing was started.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Why {
    NoEngineForThisSystem,
    PortTaken,
    NoProgram,
    NoModel,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// The engine answers. There is nothing to do.
    Nothing,
    /// An engine this app started is on its way up. Wait for it.
    Wait,
    /// Start this engine, with this program.
    Start(Engine, PathBuf),
    /// Start nothing, and say why.
    Cannot(Why),
}

/// The whole plan, worked out before anything is started.
pub fn plan(facts: &Facts) -> Step {
    if facts.answering {
        return Step::Nothing;
    }
    if facts.ours {
        return Step::Wait;
    }
    let Some(engine) = &facts.found.engine else {
        return Step::Cannot(Why::NoEngineForThisSystem);
    };
    if facts.port_taken {
        return Step::Cannot(Why::PortTaken);
    }
    let Some(program) = &facts.found.program else {
        return Step::Cannot(Why::NoProgram);
    };
    if facts.found.models == 0 {
        return Step::Cannot(Why::NoModel);
    }
    Step::Start(engine.clone(), program.clone())
}

/// Why nothing was started, as one sentence for the window.
pub fn sentence(why: Why, name: &str, port: u16) -> String {
    match why {
        Why::NoEngineForThisSystem => NO_ENGINE.to_string(),
        Why::PortTaken => format!("Something that is not the engine is using port {port} on this computer, so the app did not start one."),
        Why::NoProgram => format!("The app looked for {name} on this computer and did not find it."),
        Why::NoModel => "With no model to serve, the app did not start the engine.".to_string(),
    }
}

const NO_ENGINE: &str = "The app cannot start an engine on this system yet.";

const NOT_YET: &str = "The app started the engine, and it is not answering yet.";

// ---------------------------------------------------------------------------
// Starting, and stopping what was started
// ---------------------------------------------------------------------------

/// What the page is told.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartReport {
    /// The engine answers now.
    pub answering: bool,
    /// This call started an engine.
    pub started: bool,
    /// Why no engine answers, in one sentence, when none does.
    pub reason: Option<String>,
}

impl StartReport {
    fn not_answering(started: bool, reason: String) -> Self {
        StartReport { answering: false, started, reason: Some(reason) }
    }
}

/// How a call began, once the plan was carried out.
enum Begun {
    Answering,
    Waiting,
    Started,
    Cannot(String),
}

/// The engine this app started, when it has started one, and where that
/// engine's output goes.
pub struct Starter {
    log: PathBuf,
    child: Mutex<Option<Child>>,
}

impl Starter {
    pub fn new(log: PathBuf) -> Self {
        Starter { log, child: Mutex::new(None) }
    }

    fn slot(&self) -> MutexGuard<'_, Option<Child>> {
        self.child.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// True while an engine this app started is still running.
    pub fn running(&self) -> bool {
        still_running(&mut self.slot())
    }

    /// Makes sure an engine answers at `base`, starting one only when none
    /// does, and waits up to `patience` for one it started.
    pub async fn bring_up(&self, base: &str, found: &Found, patience: Duration) -> StartReport {
        let address = match address_of(base) {
            Ok(address) => address,
            Err(reason) => return StartReport::not_answering(false, reason),
        };
        let answering = engine::fetch_model_ids(base).await.is_ok();
        let started = match self.begin(&address, found, answering) {
            Begun::Answering => return StartReport { answering: true, started: false, reason: None },
            Begun::Cannot(reason) => return StartReport::not_answering(false, reason),
            Begun::Waiting => false,
            Begun::Started => true,
        };

        let until = Instant::now() + patience;
        loop {
            if engine::fetch_model_ids(base).await.is_ok() {
                return StartReport { answering: true, started, reason: None };
            }
            if !self.running() {
                let reason = format!("The engine stopped before it answered, and what it said is in {}.", self.log.display());
                return StartReport::not_answering(started, reason);
            }
            if Instant::now() >= until {
                return StartReport::not_answering(started, NOT_YET.to_string());
            }
            tokio::time::sleep(PAUSE).await;
        }
    }

    /// Checks the whole plan, then carries it out. Only one caller is in
    /// here at a time, so a second one finds the first one's engine running
    /// and starts nothing.
    fn begin(&self, address: &Address, found: &Found, answering: bool) -> Begun {
        let mut slot = self.slot();
        let ours = still_running(&mut slot);
        let port_taken = !answering && !ours && someone_answers(address);
        match plan(&Facts { answering, ours, port_taken, found: found.clone() }) {
            Step::Nothing => Begun::Answering,
            Step::Wait => Begun::Waiting,
            Step::Cannot(why) => Begun::Cannot(sentence(why, found.engine.as_ref().map_or("the engine", Engine::program), address.port)),
            Step::Start(engine, program) => match self.launch(&engine, &program, address) {
                Ok(child) => {
                    *slot = Some(child);
                    Begun::Started
                }
                Err(reason) => Begun::Cannot(reason),
            },
        }
    }

    /// Starts the engine's program, set apart from the app, with everything
    /// it says going to the log.
    fn launch(&self, engine: &Engine, program: &Path, address: &Address) -> Result<Child, String> {
        let name = program.file_name().and_then(|name| name.to_str()).unwrap_or("the engine");
        let no_log = |error: std::io::Error| format!("The app could not write the engine's log, so it did not start the engine: {error}.");
        let log = self.fresh_log().map_err(no_log)?;
        let errors = log.try_clone().map_err(no_log)?;
        apart(&mut engine.command(program, address))
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(errors)
            .spawn()
            .map_err(|error| format!("The app could not start {name}: {error}."))
    }

    /// The log, emptied: it holds what the latest engine said and no more.
    /// Only this computer's user can read it.
    fn fresh_log(&self) -> std::io::Result<File> {
        if let Some(folder) = self.log.parent() {
            fs::create_dir_all(folder)?;
        }
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&self.log)
    }

    /// Stops the engine this app started, when it started one, and what it
    /// started in turn. Nothing else is touched: an engine that was already
    /// running is not the app's to stop.
    pub fn stop(&self) {
        self.stop_within(GRACE);
    }

    /// Stops the engine the app started, giving it `grace` to stop by itself.
    /// The slot stays locked until it has stopped, so nothing starts another
    /// engine at the same address meanwhile.
    fn stop_within(&self, grace: Duration) {
        let mut slot = self.slot();
        if let Some(mut child) = slot.take() {
            end(&mut child, grace);
        }
    }
}

/// Stops the engine and everything it started. On Mac and Linux the engine's
/// whole group is asked to stop, then, after `grace`, made to. On Windows,
/// which has no way to ask a program with no window, the engine and what it
/// started are made to stop at once.
fn end(child: &mut Child, grace: Duration) {
    #[cfg(unix)]
    if signal_group(child, libc::SIGTERM) {
        let until = Instant::now() + grace;
        loop {
            match child.try_wait() {
                Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(50)),
                Ok(None) => break,
                _ => return,
            }
        }
        signal_group(child, libc::SIGKILL);
    }
    #[cfg(windows)]
    end_tree(child);
    #[cfg(not(unix))]
    let _ = grace;
    let _ = child.kill();
    let _ = child.wait();
}

/// Sends `signal` to the engine's process group, the group it was started in
/// by `apart`. True when it was sent.
#[cfg(unix)]
fn signal_group(child: &Child, signal: libc::c_int) -> bool {
    let Ok(group) = libc::pid_t::try_from(child.id()) else {
        return false;
    };
    // SAFETY: kill takes no pointers. The group is the engine's own, since it
    // was started in a group of its own, and the engine has not been waited
    // for yet, so its number cannot have been handed to anything else.
    group > 1 && unsafe { libc::kill(-group, signal) } == 0
}

/// Ends the engine and every program it started, with Windows' own
/// `taskkill`, from the Windows folder.
#[cfg(windows)]
fn end_tree(child: &Child) {
    let windows = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let taskkill = windows.join("System32").join("taskkill.exe");
    let _ = quiet(Command::new(taskkill).arg("/PID").arg(child.id().to_string()).arg("/T").arg("/F"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

impl Drop for Starter {
    fn drop(&mut self) {
        self.stop();
    }
}

/// True while the engine in the slot is running. One that has stopped is
/// forgotten, so the next call may start another.
fn still_running(slot: &mut Option<Child>) -> bool {
    let stopped = match slot.as_mut() {
        Some(child) => matches!(child.try_wait(), Ok(Some(_))),
        None => return false,
    };
    if stopped {
        *slot = None;
    }
    !stopped
}

/// Starts the engine when none answers. Safe to call at any time.
#[tauri::command]
pub async fn start_engine(starter: tauri::State<'_, Starter>) -> Result<StartReport, String> {
    let base = match engine::base_url() {
        Ok(base) => base,
        Err(reason) => return Ok(StartReport::not_answering(false, reason)),
    };
    Ok(starter.bring_up(&base, &look(), PATIENCE).await)
}

#[cfg(test)]
mod tests {
    // Brad, on what the app does by itself: "it should search and up the local
    // models and stack." and "install and go, one click ready". These tests
    // cover the "up" for the engine. The program they start is a small shell
    // script that stands in for the engine, and every port is one the system
    // hands out as free, never the engine's own.

    use super::*;

    const NAME: &str = "mlx_lm.server";
    const MLX: Option<Engine> = Some(Engine::Mlx);
    #[cfg(unix)]
    const SHORT: Duration = Duration::from_millis(300);

    // Where the app is put together. No test can run the whole app, so two
    // tests read the file.
    const PUT_TOGETHER: &str = include_str!("lib.rs");

    fn scratch() -> PathBuf {
        let folder = std::env::temp_dir().join(format!("rusty-business-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn facts(answering: bool, ours: bool, port_taken: bool, engine: Option<Engine>, program: Option<&str>, models: usize) -> Facts {
        Facts { answering, ours, port_taken, found: Found { engine, program: program.map(PathBuf::from), models } }
    }

    /// The source of everything but the tests.
    fn code() -> &'static str {
        include_str!("starter.rs").split("#[cfg(test)]").next().unwrap()
    }

    fn told(command: &Command) -> Vec<&str> {
        command.get_args().map(|part| part.to_str().unwrap()).collect()
    }

    #[test]
    fn only_what_is_missing_is_started() {
        let program = "/opt/homebrew/bin/mlx_lm.server";

        // Everything is in place and nothing answers: start it.
        assert_eq!(plan(&facts(false, false, false, MLX, Some(program), 1)), Step::Start(Engine::Mlx, PathBuf::from(program)));
        // On Windows and Linux, the same plan starts llama-server.
        let llama = Engine::Llama { models: PathBuf::from("/home/someone/.local/share/rusty-business/models") };
        assert_eq!(
            plan(&facts(false, false, false, Some(llama.clone()), Some("/usr/bin/llama-server"), 1)),
            Step::Start(llama.clone(), PathBuf::from("/usr/bin/llama-server"))
        );
        assert_eq!(plan(&facts(false, false, false, Some(llama), Some("/usr/bin/llama-server"), 0)), Step::Cannot(Why::NoModel));

        // An engine that answers is left alone, whatever else is true.
        assert_eq!(plan(&facts(true, false, true, MLX, Some(program), 3)), Step::Nothing);
        assert_eq!(plan(&facts(true, false, true, None, None, 0)), Step::Nothing);
        assert_eq!(plan(&facts(true, true, true, MLX, Some(program), 3)), Step::Nothing);

        // One this app started is waited for, not started again.
        assert_eq!(plan(&facts(false, true, false, MLX, Some(program), 1)), Step::Wait);
        assert_eq!(plan(&facts(false, true, true, MLX, Some(program), 1)), Step::Wait);

        // Each thing that is missing stops the plan before anything starts.
        assert_eq!(plan(&facts(false, false, false, None, Some(program), 1)), Step::Cannot(Why::NoEngineForThisSystem));
        assert_eq!(plan(&facts(false, false, true, MLX, Some(program), 1)), Step::Cannot(Why::PortTaken));
        assert_eq!(plan(&facts(false, false, false, MLX, None, 1)), Step::Cannot(Why::NoProgram));
        assert_eq!(plan(&facts(false, false, false, MLX, Some(program), 0)), Step::Cannot(Why::NoModel));
    }

    #[test]
    fn windows_and_linux_start_llama_server() {
        // Brad, 2026-11-16: "I should be able to just pull the .dmg or .exe
        // from read me and install and go, one click ready." The roadmap, of
        // the .exe: "Until Windows has an engine, the window opens and says
        // so." Brad, 2026-11-17, to a request for Linux downloads: "I got
        // you in coming build". Then, 2026-11-19: "add an engine to linux
        // and windows distro's". Both now get llama-server, serving the
        // app's own models folder. The Mac's tests check what each gets.
        let models = PathBuf::from("/somewhere/rusty-business/models");
        assert_eq!(Engine::on("macos", Some(models.clone())), Some(Engine::Mlx));
        assert_eq!(Engine::on("macos", None), Some(Engine::Mlx), "a Mac's models are in the Hugging Face cache");
        assert_eq!(Engine::on("windows", Some(models.clone())), Some(Engine::Llama { models: models.clone() }));
        assert_eq!(Engine::on("linux", Some(models.clone())), Some(Engine::Llama { models: models.clone() }));
        // With no models folder, there is nothing to serve, and the window says why.
        assert_eq!(Engine::on("windows", None), None);
        assert_eq!(Engine::on("linux", None), None);
        assert_eq!(Engine::on("freebsd", Some(models.clone())), None);
        assert_eq!(sentence(Why::NoEngineForThisSystem, NAME, 8081), "The app cannot start an engine on this system yet.");

        let llama = Engine::Llama { models };
        assert_eq!(Engine::Mlx.program_on("macos"), "mlx_lm.server");
        assert_eq!(llama.program_on("windows"), "llama-server.exe");
        assert_eq!(llama.program_on("linux"), "llama-server");
        assert!(code().contains("fn here() -> Option<Engine> {\n        Engine::on(std::env::consts::OS, crate::models::models_folder())"));
    }

    #[test]
    fn llama_server_is_started_at_its_own_address_with_the_apps_models_folder() {
        // Brad, 2026-11-19: "add an engine to linux and windows distro's".
        // The engine's own chat page stays off, it downloads nothing, and it
        // loads one model at a time, with no model named, so nothing is
        // loaded until a chat asks for one.
        let models = Path::new("/home/someone/.local/share/rusty-business/models");
        let program = Path::new("/usr/bin/llama-server");
        let command = llama_command(program, &Address { host: "127.0.0.1".to_string(), port: 8081 }, models, Vec::new());
        assert_eq!(command.get_program(), program);
        let said = told(&command);
        assert_eq!(
            said,
            ["--host", "127.0.0.1", "--port", "8081", "--models-dir", models.to_str().unwrap(), "--models-max", "1", "--offline", "--no-webui"]
        );
        // Never told to log what is typed, or more than it logs by itself,
        // and never told how much a model takes in at once.
        for never in ["-m", "--model", "-hf", "-v", "--verbose", "--log-verbose", "-lv", "--log-verbosity", "--log-prompts-dir", "-c", "--ctx-size", "--webui"] {
            assert!(!said.contains(&never), "{never}");
        }
        assert_eq!(command.get_envs().count(), 0, "nothing is set, with no settings of its own to take away");
    }

    #[test]
    fn llama_server_takes_none_of_its_settings_from_the_apps_own() {
        // A variable such as LLAMA_ARG_HOST would move it off this computer,
        // and LLAMA_ARG_LOG_PROMPTS_DIR would have it keep what is typed.
        // Each model it loads gets the same variables, so none is passed on.
        let names = ["PATH", "LLAMA_ARG_HOST", "llama_arg_log_prompts_dir", "LLAMA_CACHE", "GGML_VK_VISIBLE_DEVICES", "HOME", "NOT_LLAMA_ARG"].map(OsString::from);
        let command = llama_command(Path::new("/usr/bin/llama-server"), &Address { host: "::1".to_string(), port: 9000 }, Path::new("/models"), names);
        let mut set: Vec<(&str, Option<&std::ffi::OsStr>)> = command.get_envs().map(|(name, value)| (name.to_str().unwrap(), value)).collect();
        set.sort();
        assert_eq!(set, [("LLAMA_ARG_HOST", None), ("LLAMA_CACHE", None), ("llama_arg_log_prompts_dir", None)]);
        // The names are the app's own, as it runs.
        assert!(code().contains("llama_command(program, address, models, std::env::vars_os().map(|(name, _)| name))"));
    }

    #[test]
    fn on_windows_the_engine_has_no_window_and_stops_with_what_it_started() {
        // These run on Windows only, so the Mac's tests read them. The
        // engine's console window would open beside the app's; and stopping
        // llama-server alone would leave the program it starts for a model.
        let code = code();
        assert!(code.contains("command.creation_flags(0x0800_0000);"), "CREATE_NO_WINDOW");
        assert!(code.contains("fn apart(command: &mut Command) -> &mut Command {"));
        assert!(code.contains("apart(&mut engine.command(program, address))"));
        assert!(code.contains(".join(\"System32\").join(\"taskkill.exe\")"));
        assert!(code.contains(".arg(\"/PID\").arg(child.id().to_string()).arg(\"/T\").arg(\"/F\")"));
        assert!(code.contains("#[cfg(windows)]\n    end_tree(child);"));
    }

    #[test]
    fn every_reason_is_one_plain_sentence() {
        for why in [Why::NoEngineForThisSystem, Why::PortTaken, Why::NoProgram, Why::NoModel] {
            let said = sentence(why, NAME, 8081);
            assert!(said.ends_with('.') && !said.contains('\n'), "{said:?}");
            assert!(said.starts_with(char::is_uppercase), "{said:?}");
            assert_eq!(said.matches(". ").count(), 0, "one sentence: {said:?}");
        }
        assert_eq!(sentence(Why::NoProgram, NAME, 8081), "The app looked for mlx_lm.server on this computer and did not find it.");
        assert!(sentence(Why::PortTaken, NAME, 8081).contains("port 8081"));
        assert!(NOT_YET.ends_with('.'));
        assert_eq!(sentence(Why::NoEngineForThisSystem, NAME, 8081), NO_ENGINE);
        assert_eq!(sentence(Why::NoProgram, "llama-server.exe", 8081), "The app looked for llama-server.exe on this computer and did not find it.");
    }

    #[test]
    fn the_page_is_told_under_the_names_it_reads() {
        let not_found = StartReport::not_answering(false, sentence(Why::NoProgram, NAME, 8080));
        assert_eq!(
            serde_json::to_string(&not_found).unwrap(),
            r#"{"answering":false,"started":false,"reason":"The app looked for mlx_lm.server on this computer and did not find it."}"#
        );
        let up = StartReport { answering: true, started: true, reason: None };
        assert_eq!(serde_json::to_value(&up).unwrap(), serde_json::json!({ "answering": true, "started": true, "reason": null }));
    }

    #[test]
    fn the_engine_is_started_at_its_own_address_with_no_model_named() {
        let program = Path::new("/opt/homebrew/bin/mlx_lm.server");
        let command = command(program, &Address { host: "127.0.0.1".to_string(), port: 8081 });
        assert_eq!(command.get_program(), program);
        let said: Vec<&str> = command.get_args().map(|part| part.to_str().unwrap()).collect();
        assert_eq!(said, ["--host", "127.0.0.1", "--port", "8081", "--log-level", "INFO"]);
        assert!(!said.contains(&"--model"), "a model named here would be loaded, or downloaded, at the start");
    }

    #[test]
    fn the_engine_is_never_asked_to_log_what_is_typed() {
        let command = command(Path::new("/opt/homebrew/bin/mlx_lm.server"), &Address { host: "::1".to_string(), port: 9000 });
        let said: Vec<&str> = command.get_args().map(|part| part.to_str().unwrap()).collect();
        let level = said.iter().position(|part| *part == "--log-level").expect("the level is always named");
        assert_eq!(said[level + 1], "INFO");
        assert!(!said.contains(&"DEBUG"));
    }

    #[test]
    fn the_engine_the_app_starts_stays_offline() {
        let command = command(Path::new("/opt/homebrew/bin/mlx_lm.server"), &Address { host: "127.0.0.1".to_string(), port: 8081 });
        let set: Vec<(&str, Option<&str>)> =
            command.get_envs().map(|(name, value)| (name.to_str().unwrap(), value.and_then(|value| value.to_str()))).collect();
        assert_eq!(set, [("HF_HUB_OFFLINE", Some("1"))]);
    }

    #[test]
    fn an_engine_is_never_started_where_another_computer_could_reach_it() {
        let at = |host: &str, port: u16| Ok(Address { host: host.to_string(), port });
        assert_eq!(address_of("http://127.0.0.1:8080/v1"), at("127.0.0.1", 8080));
        assert_eq!(address_of("http://localhost:9001/v1/"), at("localhost", 9001));
        assert_eq!(address_of("http://[::1]:9000/v1"), at("::1", 9000));
        assert_eq!(address_of("http://127.0.0.1/v1"), at("127.0.0.1", 80));

        for elsewhere in ["http://0.0.0.0:8080/v1", "http://192.168.1.20:8080/v1", "http://example.com:8080/v1", "https://127.0.0.1:8080/v1"] {
            let refused = address_of(elsewhere).unwrap_err();
            assert!(refused.contains("must be on this computer"), "{elsewhere}: {refused}");
        }
    }

    #[test]
    fn the_program_is_looked_for_in_order() {
        let home = scratch().join("home");
        for version in ["3.9", "3.13", "3.12", "notes"] {
            fs::create_dir_all(home.join("Library").join("Python").join(version)).unwrap();
        }
        let pip = |version: &str| home.join("Library").join("Python").join(version).join("bin");

        // A folder on PATH that is not a full path is passed over: the app
        // does not start where a terminal starts.
        assert_eq!(
            places_on("macos", Some("/usr/bin:tools/bin::/opt/tools/bin:."), Some(&home)),
            [
                PathBuf::from("/usr/bin"),
                PathBuf::from("/opt/tools/bin"),
                home.join(".local").join("bin"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
                pip("3.13"),
                pip("3.12"),
                pip("3.9"),
            ]
        );
        assert_eq!(places_on("macos", None, None), [PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")]);
        assert_eq!(places(None, None), places_on(std::env::consts::OS, None, None));
    }

    #[test]
    fn llama_server_is_looked_for_where_each_system_keeps_programs() {
        let home = scratch().join("home");
        let on_path = std::env::temp_dir().join("llama").join("bin");
        let path = std::env::join_paths([on_path.clone(), PathBuf::from("relative")]).unwrap();
        let path = path.to_str();
        // Linux: PATH, then the user's own folder, Homebrew's, and the system's.
        assert_eq!(
            places_on("linux", path, Some(&home)),
            [
                on_path.clone(),
                home.join(".local").join("bin"),
                PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
            ]
        );
        // Windows: PATH alone, and none of the Mac's or Linux's folders.
        assert_eq!(places_on("windows", path, Some(&home)), [on_path]);
        assert_eq!(places_on("windows", None, Some(&home)), Vec::<PathBuf>::new());
    }

    #[test]
    fn the_page_can_ask_for_the_engine_to_be_started() {
        let commands = PUT_TOGETHER.split_once("generate_handler![").and_then(|(_, rest)| rest.split_once(']')).expect("the list of commands").0;
        assert!(commands.contains("starter::start_engine"), "{commands}");
        assert!(PUT_TOGETHER.contains("app.manage(starter::Starter::new("), "the command needs the starter the app keeps");
    }

    #[test]
    fn the_app_stops_its_engine_when_it_quits() {
        let on_quitting = PUT_TOGETHER.split_once("RunEvent::Exit").expect("the app is told when it quits").1;
        assert!(on_quitting.contains("starter.stop();"), "{on_quitting}");
    }

    #[cfg(unix)]
    mod with_a_program {
        // These start a real process, so they take turns.

        use std::collections::HashMap;
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::os::unix::fs::{PermissionsExt, symlink};
        use std::thread;

        use tauri::async_runtime::block_on;

        use super::*;

        static TURN: Mutex<()> = Mutex::new(());

        fn my_turn() -> MutexGuard<'static, ()> {
            TURN.lock().unwrap_or_else(PoisonError::into_inner)
        }

        /// A stand-in for the engine's program. Each time it is run it adds a
        /// line to a file beside it (its process number and what it was
        /// told), notes whether it was told to stay offline, and then does
        /// `then`.
        fn program_in(folder: &Path, then: &str) -> PathBuf {
            program_named(folder, NAME, then)
        }

        fn program_named(folder: &Path, name: &str, then: &str) -> PathBuf {
            fs::create_dir_all(folder).unwrap();
            let file = folder.join(name);
            let script = format!("#!/bin/sh\necho \"$$ $*\" >> \"$0.runs\"\necho \"$HF_HUB_OFFLINE\" > \"$0.offline\"\n{then}\n");
            fs::write(&file, script).unwrap();
            fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
            file
        }

        /// One line for each time the stand-in was run.
        fn runs(program: &Path) -> Vec<String> {
            let noted = fs::read_to_string(format!("{}.runs", program.display())).unwrap_or_default();
            noted.lines().map(String::from).collect()
        }

        fn have(program: &Path, models: usize) -> Found {
            Found { engine: MLX, program: Some(program.to_path_buf()), models }
        }

        /// Something a test started that is not the app's, standing for
        /// anything else that is running on the computer. It holds none of
        /// the test's output, and it is stopped when the test ends, pass or
        /// fail.
        struct Bystander(Child);

        impl Bystander {
            fn start() -> Self {
                Self(Command::new("sleep").arg("600").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap())
            }

            fn running(&mut self) -> bool {
                matches!(self.0.try_wait(), Ok(None))
            }
        }

        impl Drop for Bystander {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }

        fn alive(process: &str) -> bool {
            Command::new("ps").args(["-p", process]).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap().success()
        }

        /// True once `process` has gone, within a few seconds. A program
        /// whose parent has stopped is cleared away by the system, a moment
        /// later.
        fn gone_soon(process: &str) -> bool {
            let until = Instant::now() + Duration::from_secs(5);
            while alive(process) {
                if Instant::now() >= until {
                    return false;
                }
                thread::sleep(Duration::from_millis(50));
            }
            true
        }

        /// What the stand-in wrote to the file beside it named `.{what}`,
        /// once it has.
        fn noted(program: &Path, what: &str) -> String {
            let file = PathBuf::from(format!("{}.{what}", program.display()));
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                if let Ok(said) = fs::read_to_string(&file)
                    && said.ends_with('\n')
                {
                    return said.trim().to_string();
                }
                assert!(Instant::now() < until, "nothing in {}", file.display());
                thread::sleep(Duration::from_millis(50));
            }
        }

        /// A stand-in for an engine that starts a second program, the way
        /// llama-server starts one for each model, then does `then`.
        fn parent_in(folder: &Path, then: &str) -> PathBuf {
            program_in(folder, &format!("{then}\nsleep 600 &\necho $! > \"$0.kid\"\nwait"))
        }

        /// A port on this computer that nothing is using.
        fn free_port() -> u16 {
            TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
        }

        fn base(port: u16) -> String {
            format!("http://127.0.0.1:{port}/v1")
        }

        fn engine_answer() -> String {
            let body = r#"{"object":"list","data":[{"id":"mlx-community/phi-4-4bit"}]}"#;
            format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
        }

        const NOT_AN_ENGINE: &str = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

        /// Answers every caller at `listener` with `answer`, for as long as
        /// the tests run.
        fn answer_at(listener: TcpListener, answer: String) {
            thread::spawn(move || {
                for mut caller in listener.incoming().flatten() {
                    let _ = caller.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut asked = [0u8; 4096];
                    let _ = caller.read(&mut asked);
                    let _ = caller.write_all(answer.as_bytes());
                }
            });
        }

        #[test]
        fn the_whole_plan_is_checked_before_anything_starts() {
            let _turn = my_turn();
            let taken = TcpListener::bind("127.0.0.1:0").unwrap();
            let taken_port = taken.local_addr().unwrap().port();
            answer_at(taken, NOT_AN_ENGINE.to_string());

            let folder = scratch();
            let program = program_in(&folder, "exec sleep 600");
            let free = free_port();
            let cases: [(&str, u16, Found, String); 4] = [
                ("no engine is chosen for this system", free, Found { engine: None, ..have(&program, 1) }, sentence(Why::NoEngineForThisSystem, NAME, free)),
                ("the port is taken", taken_port, have(&program, 1), sentence(Why::PortTaken, NAME, taken_port)),
                ("the program is not on this computer", free, Found { program: None, ..have(&program, 1) }, sentence(Why::NoProgram, NAME, free)),
                ("no model is on the disk", free, have(&program, 0), sentence(Why::NoModel, NAME, free)),
            ];
            for (what, port, found, reason) in cases {
                let starter = Starter::new(folder.join("data").join("engine.log"));
                let report = block_on(starter.bring_up(&base(port), &found, SHORT));
                assert_eq!(report, StartReport { answering: false, started: false, reason: Some(reason) }, "{what}");
                assert_eq!(runs(&program), Vec::<String>::new(), "nothing was run: {what}");
                assert!(!starter.running(), "{what}");
                assert!(!folder.join("data").exists(), "no log, and no folder for one: {what}");
            }

            // An address that is not on this computer is refused the same way.
            let starter = Starter::new(folder.join("data").join("engine.log"));
            let report = block_on(starter.bring_up("http://192.168.1.20:8080/v1", &have(&program, 1), SHORT));
            assert!(!report.started && report.reason.unwrap().contains("must be on this computer"));
            assert_eq!(runs(&program), Vec::<String>::new());
        }

        #[test]
        fn asking_twice_changes_nothing() {
            let _turn = my_turn();
            let folder = scratch();
            let program = program_in(&folder, "exec sleep 600");
            let starter = Starter::new(folder.join("engine.log"));
            let port = free_port();

            let first = block_on(starter.bring_up(&base(port), &have(&program, 1), SHORT));
            assert_eq!(first, StartReport { answering: false, started: true, reason: Some(NOT_YET.to_string()) });
            let second = block_on(starter.bring_up(&base(port), &have(&program, 1), SHORT));
            assert_eq!(second, StartReport { answering: false, started: false, reason: Some(NOT_YET.to_string()) });
            assert_eq!(runs(&program).len(), 1, "one engine, however often it is asked for");
            assert!(starter.running());

            // Four callers at the same moment start one engine between them.
            let folder = scratch();
            let program = program_in(&folder, "exec sleep 600");
            let starter = Starter::new(folder.join("engine.log"));
            let port = free_port();
            let reports: Vec<StartReport> = thread::scope(|scope| {
                let callers: Vec<_> =
                    (0..4).map(|_| scope.spawn(|| block_on(starter.bring_up(&base(port), &have(&program, 1), SHORT)))).collect();
                callers.into_iter().map(|caller| caller.join().unwrap()).collect()
            });
            assert_eq!(reports.iter().filter(|report| report.started).count(), 1);
            assert_eq!(runs(&program).len(), 1);
        }

        #[test]
        fn the_program_is_told_where_to_listen_and_to_stay_offline() {
            let _turn = my_turn();
            let folder = scratch();
            let program = program_in(&folder, "exec sleep 600");
            let starter = Starter::new(folder.join("engine.log"));
            let port = free_port();
            assert!(block_on(starter.bring_up(&base(port), &have(&program, 1), SHORT)).started);

            let run = runs(&program).remove(0);
            let (_, told) = run.split_once(' ').unwrap();
            assert_eq!(told, format!("--host 127.0.0.1 --port {port} --log-level INFO"));
            assert_eq!(fs::read_to_string(format!("{}.offline", program.display())).unwrap().trim(), "1");
        }

        #[test]
        fn the_app_waits_for_the_engine_it_started() {
            let _turn = my_turn();
            let folder = scratch();
            let program = program_in(&folder, "exec sleep 600");
            let starter = Starter::new(folder.join("engine.log"));
            let port = free_port();

            // The stand-in never listens, so the test answers in its place a
            // moment after it starts, the way an engine takes a moment to.
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(600));
                answer_at(TcpListener::bind(("127.0.0.1", port)).unwrap(), engine_answer());
            });
            let report = block_on(starter.bring_up(&base(port), &have(&program, 1), Duration::from_secs(20)));
            assert_eq!(report, StartReport { answering: true, started: true, reason: None });

            // Asked again, it finds the engine answering and starts nothing.
            let again = block_on(starter.bring_up(&base(port), &have(&program, 1), SHORT));
            assert_eq!(again, StartReport { answering: true, started: false, reason: None });
            assert_eq!(runs(&program).len(), 1);
        }

        #[test]
        fn an_engine_that_answers_is_left_alone() {
            let _turn = my_turn();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            answer_at(listener, engine_answer());

            let folder = scratch();
            let program = program_in(&folder, "exec sleep 600");
            let starter = Starter::new(folder.join("engine.log"));
            let up = StartReport { answering: true, started: false, reason: None };
            assert_eq!(block_on(starter.bring_up(&base(port), &have(&program, 1), SHORT)), up);
            // With nothing to start one with, an engine that answers is still an engine that answers.
            assert_eq!(block_on(starter.bring_up(&base(port), &Found { engine: None, program: None, models: 0 }, SHORT)), up);
            assert_eq!(runs(&program), Vec::<String>::new());
            assert!(!folder.join("engine.log").exists());

            // Stopping what the app started does not reach it: the app started nothing.
            starter.stop();
            drop(starter);
            assert!(block_on(engine::fetch_model_ids(&base(port))).is_ok(), "the engine still answers");
        }

        #[test]
        fn a_port_something_else_holds_is_left_alone() {
            let _turn = my_turn();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            answer_at(listener, NOT_AN_ENGINE.to_string());

            let folder = scratch();
            let program = program_in(&folder, "exec sleep 600");
            let starter = Starter::new(folder.join("engine.log"));
            let report = block_on(starter.bring_up(&base(port), &have(&program, 1), SHORT));
            assert_eq!(report.reason.as_deref(), Some(sentence(Why::PortTaken, NAME, port).as_str()));
            assert!(!report.started && !report.answering);
            assert_eq!(runs(&program), Vec::<String>::new());
            assert!(someone_answers(&Address { host: "127.0.0.1".to_string(), port }), "what held the port still holds it");
        }

        #[test]
        fn the_app_stops_only_what_it_started() {
            let _turn = my_turn();
            let mut bystander = Bystander::start();

            let folder = scratch();
            let program = program_in(&folder, "exec sleep 600");
            let starter = Starter::new(folder.join("engine.log"));
            assert!(block_on(starter.bring_up(&base(free_port()), &have(&program, 1), SHORT)).started);
            let run = runs(&program).remove(0);
            let (process, _) = run.split_once(' ').unwrap();
            assert!(alive(process) && starter.running());

            starter.stop();
            assert!(!alive(process), "the engine the app started is stopped");
            assert!(!starter.running());
            assert!(bystander.running(), "nothing else is");

            // Stopping again finds nothing to stop.
            starter.stop();
            assert!(bystander.running());
        }

        #[test]
        fn llama_server_is_told_where_to_listen_and_which_folder_to_serve() {
            // Brad, 2026-11-19: "add an engine to linux and windows distro's".
            let _turn = my_turn();
            let folder = scratch();
            let models = folder.join("models");
            let program = program_named(&folder, "llama-server", "exec sleep 600");
            let starter = Starter::new(folder.join("engine.log"));
            let port = free_port();
            let found = Found { engine: Some(Engine::Llama { models: models.clone() }), program: Some(program.clone()), models: 1 };
            assert!(block_on(starter.bring_up(&base(port), &found, SHORT)).started);

            let run = runs(&program).remove(0);
            let (_, told) = run.split_once(' ').unwrap();
            assert_eq!(told, format!("--host 127.0.0.1 --port {port} --models-dir {} --models-max 1 --offline --no-webui", models.display()));
        }

        #[test]
        fn what_the_engine_started_stops_with_it() {
            // llama-server starts a second program for each model it loads.
            // When the app quits, that one stops too.
            let _turn = my_turn();
            let mut bystander = Bystander::start();
            let folder = scratch();
            let program = parent_in(&folder, "");
            let starter = Starter::new(folder.join("engine.log"));
            assert!(block_on(starter.bring_up(&base(free_port()), &have(&program, 1), SHORT)).started);
            let kid = noted(&program, "kid");
            assert!(alive(&kid));

            starter.stop();
            assert!(gone_soon(&kid), "what the engine started is stopped");
            assert!(bystander.running(), "nothing else is");
        }

        #[test]
        fn the_engine_is_asked_to_stop_before_it_is_made_to() {
            // Asked, llama-server stops each model's program itself, and
            // mlx_lm.server finishes what it is doing.
            let _turn = my_turn();
            let folder = scratch();
            let program = parent_in(&folder, "trap 'echo asked > \"$0.asked\"; exit 0' TERM");
            let starter = Starter::new(folder.join("engine.log"));
            assert!(block_on(starter.bring_up(&base(free_port()), &have(&program, 1), SHORT)).started);
            let kid = noted(&program, "kid");

            starter.stop();
            assert_eq!(noted(&program, "asked"), "asked");
            assert!(gone_soon(&kid));
        }

        #[test]
        fn an_engine_that_will_not_stop_when_asked_is_made_to() {
            let _turn = my_turn();
            let folder = scratch();
            let program = parent_in(&folder, "trap '' TERM");
            let starter = Starter::new(folder.join("engine.log"));
            assert!(block_on(starter.bring_up(&base(free_port()), &have(&program, 1), SHORT)).started);
            let run = runs(&program).remove(0);
            let (process, _) = run.split_once(' ').unwrap();
            let kid = noted(&program, "kid");

            let asked = Instant::now();
            starter.stop_within(SHORT);
            assert!(asked.elapsed() >= SHORT && asked.elapsed() < GRACE, "{:?}", asked.elapsed());
            assert!(!alive(process) && gone_soon(&kid));
            assert!(!starter.running());
        }

        #[test]
        fn an_engine_that_stops_at_once_is_reported_and_what_it_said_is_kept() {
            let _turn = my_turn();
            let folder = scratch();
            let program = program_in(&folder, "echo 'mlx_lm.server: error: no such option' >&2\nexit 2");
            let log = folder.join("data").join("engine.log");
            let starter = Starter::new(log.clone());
            let port = free_port();

            let report = block_on(starter.bring_up(&base(port), &have(&program, 1), Duration::from_secs(20)));
            let reason = report.reason.clone().unwrap();
            assert!(report.started && !report.answering);
            // One sentence, like every other reason.
            assert_eq!(reason, format!("The engine stopped before it answered, and what it said is in {}.", log.display()));
            assert_eq!(fs::read_to_string(&log).unwrap().trim(), "mlx_lm.server: error: no such option");
            assert_eq!(fs::metadata(&log).unwrap().permissions().mode() & 0o777, 0o600, "only this computer's user reads the log");
            assert!(!starter.running());

            // It is missing again, so the next call starts it again, and the
            // log holds the latest start only.
            assert!(block_on(starter.bring_up(&base(port), &have(&program, 1), Duration::from_secs(20))).started);
            assert_eq!(runs(&program).len(), 2);
            assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
        }

        #[test]
        fn a_program_that_will_not_start_is_reported() {
            let _turn = my_turn();
            let folder = scratch();
            let program = folder.join(NAME);
            fs::write(&program, "#!/nowhere/at/all\n").unwrap();
            fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
            let starter = Starter::new(folder.join("engine.log"));

            let report = block_on(starter.bring_up(&base(free_port()), &have(&program, 1), SHORT));
            let reason = report.reason.unwrap();
            assert!(!report.started && !report.answering);
            assert!(reason.starts_with("The app could not start mlx_lm.server: ") && reason.ends_with('.'), "{reason}");
            assert!(!starter.running());
        }

        #[test]
        fn the_named_program_is_the_only_one_tried() {
            let root = scratch();
            let home = root.join("home");
            let on_path = program_in(&root.join("on-path"), "exit 0");
            let in_home = program_in(&home.join(".local").join("bin"), "exit 0");
            let unusual = program_in(&root.join("unusual"), "exit 0");
            let find = |pairs: &[(&str, String)]| {
                let set: HashMap<String, String> = pairs.iter().map(|(name, value)| (name.to_string(), value.clone())).collect();
                find_program(NAME, |name| set.get(name).cloned(), Some(&home), None)
            };
            let text = |file: &Path| file.to_str().unwrap().to_string();
            let path = ("PATH", format!("/nowhere/at/all:{}", text(on_path.parent().unwrap())));

            // PATH first, then the usual folders.
            assert_eq!(find(&[path.clone()]), Some(on_path.clone()));
            assert_eq!(find(&[("PATH", "/nowhere/at/all".to_string())]), Some(in_home.clone()));
            assert_eq!(find(&[]), Some(in_home));

            // Named outright, it is that one or none.
            let from_here = "../".repeat(64) + text(&unusual).trim_start_matches('/');
            assert_eq!(find(&[path.clone(), (PROGRAM_VARIABLE, text(&unusual))]), Some(unusual));
            assert_eq!(find(&[path.clone(), (PROGRAM_VARIABLE, text(&root.join("moved-away").join(NAME)))]), None);
            // A name that is not a full path is never tried, not even one
            // that leads to the program from the folder the app started in.
            assert!(can_run(Path::new(&from_here)), "{from_here} leads to the program from any folder");
            assert_eq!(find(&[path.clone(), (PROGRAM_VARIABLE, from_here)]), None);
            assert_eq!(find(&[path.clone(), (PROGRAM_VARIABLE, "unusual/mlx_lm.server".to_string())]), None);
            // A variable set to nothing is not set.
            assert_eq!(find(&[path, (PROGRAM_VARIABLE, "  ".to_string())]), Some(on_path));
        }

        #[test]
        fn only_a_program_that_can_run_is_found() {
            let root = scratch();
            let real = program_in(&root.join("real"), "exit 0");
            assert!(can_run(&real));

            // A folder with the program's name.
            fs::create_dir_all(root.join("folder").join(NAME)).unwrap();
            assert!(!can_run(&root.join("folder").join(NAME)));

            // A file that may not be run.
            let plain = program_in(&root.join("plain"), "exit 0");
            fs::set_permissions(&plain, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(!can_run(&plain));

            // A link is followed: to a program it is one, to nothing it is not.
            fs::create_dir_all(root.join("links")).unwrap();
            symlink(&real, root.join("links").join(NAME)).unwrap();
            assert!(can_run(&root.join("links").join(NAME)));
            symlink(root.join("gone"), root.join("links").join("gone")).unwrap();
            assert!(!can_run(&root.join("links").join("gone")));

            let found = find_program(NAME, |name| (name == "PATH").then(|| {
                [root.join("folder"), root.join("plain"), root.join("links")].map(|folder| folder.to_str().unwrap().to_string()).join(":")
            }), None, None);
            assert_eq!(found, Some(root.join("links").join(NAME)));
        }

        #[test]
        fn the_engine_the_app_carries_comes_first() {
            let root = scratch();
            let home = root.join("home");
            let carried = program_in(&root.join("Any Name.app/Contents/Resources/engine/bin"), "exit 0");
            let carried_folder = carried.parent().unwrap();
            let on_path = program_in(&root.join("on-path"), "exit 0");
            let named = program_in(&root.join("named"), "exit 0");
            let text = |file: &Path| file.to_str().unwrap().to_string();
            let path = text(on_path.parent().unwrap());
            let find = |pairs: &[(&str, String)], carried: Option<&Path>| {
                let set: HashMap<String, String> = pairs.iter().map(|(name, value)| (name.to_string(), value.clone())).collect();
                find_program(NAME, |name| set.get(name).cloned(), Some(&home), carried)
            };

            // Ahead of PATH and every usual folder.
            assert_eq!(find(&[("PATH", path.clone())], Some(carried_folder)), Some(carried.clone()));
            // Not there, or not one that can run: the usual search goes on.
            assert_eq!(find(&[("PATH", path.clone())], Some(&root.join("empty"))), Some(on_path.clone()));
            fs::set_permissions(&carried, fs::Permissions::from_mode(0o644)).unwrap();
            assert_eq!(find(&[("PATH", path.clone())], Some(carried_folder)), Some(on_path));
            fs::set_permissions(&carried, fs::Permissions::from_mode(0o755)).unwrap();
            // A program named outright is still the only one tried.
            assert_eq!(find(&[("PATH", path.clone()), (PROGRAM_VARIABLE, text(&named))], Some(carried_folder)), Some(named));
            assert_eq!(find(&[("PATH", path), (PROGRAM_VARIABLE, text(&root.join("gone").join(NAME)))], Some(carried_folder)), None);
        }

        #[test]
        fn the_carried_engine_is_looked_for_only_inside_a_mac_app() {
            // The app's file name does not matter, only where its program is.
            let app = Path::new("/Applications/Any Name.app/Contents");
            assert_eq!(carried_folder(&app.join("MacOS").join("rusty-business")), Some(app.join("Resources").join("engine").join("bin")));
            // Anywhere the app runs without its bundle, there is none.
            for exe in [
                "/Users/someone/rusty-business/apps/desktop/src-tauri/target/debug/rusty-business",
                "/usr/local/bin/rusty-business",
                "/Applications/Any Name.app/Contents/rusty-business",
                "/Applications/Somewhere/MacOS/rusty-business",
                "rusty-business",
                "/",
            ] {
                assert_eq!(carried_folder(Path::new(exe)), None, "{exe}");
            }
        }

        /// Needs the engine's program where the app looks for it (or named
        /// by `RUSTY_ENGINE_PROGRAM`) and one of the app's models on this
        /// computer: `cargo test -- --ignored`. It starts a second engine on
        /// a free port and stops it. An engine that is already running is
        /// not touched.
        #[test]
        #[ignore]
        fn live_start_on_a_free_port() {
            let _turn = my_turn();
            let found = look();
            println!("Found: {found:?}");
            assert!(found.program.is_some(), "the engine's program was not found on this computer");
            assert!(found.models > 0, "none of the app's models is in the cache folder");

            let port = free_port();
            assert_ne!(port, 8080, "never the port an engine may already be on");
            let folder = scratch();
            let starter = Starter::new(folder.join("engine.log"));
            let report = block_on(starter.bring_up(&base(port), &found, Duration::from_secs(120)));
            println!("Report: {report:?}");
            println!("Log: {}", fs::read_to_string(folder.join("engine.log")).unwrap_or_default());
            assert_eq!(report, StartReport { answering: true, started: true, reason: None });

            // Offline, it still serves every one of the app's models on the disk.
            let ids = block_on(engine::fetch_model_ids(&base(port))).unwrap();
            assert_eq!(engine::allowed_models(&ids), crate::models::on_disk());

            starter.stop();
            assert!(!someone_answers(&Address { host: "127.0.0.1".to_string(), port }), "the engine this test started is stopped");
        }
    }
}
