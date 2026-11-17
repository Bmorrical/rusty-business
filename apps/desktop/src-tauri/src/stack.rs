//! Starting the stack.
//!
//! The stack is everything behind the chat that is not the engine: the
//! database, the cache, the file store, the mail catcher, the API, the
//! worker, and the front door, each in a Docker container. When its front
//! door does not answer, the app starts it. The app does not install Docker:
//! it opens Docker Desktop when that is on this computer and not running.
//!
//! The stack is `stack-compose.yaml`, built into the app. Its API and worker
//! are pulled from GitHub's container registry, pinned by digest, so nothing
//! is built on this computer.
//!
//! Four rules live here:
//! - the whole plan is checked before anything starts: this system, the
//!   port, Docker, and a place for the settings and the log;
//! - only what is missing is started: a front door that answers is left
//!   alone, and so is anything else that holds its port;
//! - asking twice changes nothing: there is one start at a time, however
//!   often the page asks;
//! - the app stops only a stack it started itself, and only when the app
//!   quits. Stopping keeps every container and its data, nothing here
//!   removes a volume or an image, and Docker Desktop is left running.
//!
//! The stack's passwords are made on its first start, from the system's
//! random source, and kept in `stack.env` in the app's data folder, where
//! only this user can read them.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::Manager;

use crate::starter::can_run;

/// The stack, as the app writes it to its data folder.
const COMPOSE: &str = include_str!("../stack-compose.yaml");

/// The front door's settings: the same file the stack `make up` starts uses.
const CADDYFILE: &str = include_str!("../../../../infra/caddy/Caddyfile");

/// The port the front door listens on, as in the stack `make up` starts.
pub const FRONT_DOOR_PORT: u16 = 4700;

/// The name Docker keeps the app's stack under, apart from the one `make up`
/// starts.
const PROJECT: &str = "rusty-business-app";

/// How long to wait between looks.
const PAUSE: Duration = Duration::from_millis(250);

/// How long a quick question to Docker may take.
const QUICK: Duration = Duration::from_secs(10);

/// How long the stack may take to stop when the app quits.
const STOP_PATIENCE: Duration = Duration::from_secs(30);

/// Starts the stack and waits until every part of it is healthy. The first
/// time, this downloads it.
const UP: [&str; 4] = ["up", "--detach", "--wait", "--quiet-pull"];

/// Makes the file store's bucket, once the stack is healthy.
const BUCKET: [&str; 5] = ["run", "--rm", "-T", "--quiet-pull", "minio-bucket"];

/// Stops the stack's containers and keeps them, with their data.
const STOP: [&str; 1] = ["stop"];

/// The variables Docker is told from the app's surroundings, when they are
/// set. Everything else is left out.
const PASSED: [&str; 7] = ["USER", "TMPDIR", "DOCKER_HOST", "DOCKER_CONTEXT", "DOCKER_CONFIG", "DOCKER_CERT_PATH", "DOCKER_TLS_VERIFY"];

/// How long each part of a start may take.
#[derive(Clone, Copy, Debug)]
pub struct Patience {
    /// For Docker Desktop to start, once the app has opened it.
    pub docker: Duration,
    /// For the stack to download, start, and be healthy.
    pub up: Duration,
    /// For the job that makes the file store's bucket.
    pub job: Duration,
    /// For the front door to answer, once the stack is healthy.
    pub door: Duration,
}

impl Patience {
    /// What the app allows.
    pub const USUAL: Patience =
        Patience { docker: Duration::from_secs(2 * 60), up: Duration::from_secs(15 * 60), job: Duration::from_secs(2 * 60), door: Duration::from_secs(10) };
}

// ---------------------------------------------------------------------------
// Where Docker is
// ---------------------------------------------------------------------------

/// What the app found on this computer for starting the stack.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// Docker's own program, if it is on this computer.
    pub docker: Option<PathBuf>,
    /// Docker Desktop, if it is on this computer.
    pub desktop: Option<PathBuf>,
    /// The program that opens an app: `open`, on a Mac.
    pub opener: PathBuf,
    /// Everything Docker is told about its surroundings.
    pub env: Vec<(String, OsString)>,
}

/// Looks, fresh each time: the app keeps no note of what it found before.
pub fn look() -> Found {
    let home = std::env::home_dir();
    let variable = |key: &str| std::env::var_os(key);
    let docker = places(variable("PATH").as_deref(), home.as_deref()).into_iter().map(|folder| folder.join("docker")).find(|program| can_run(program));
    let desktop = desktop_places(home.as_deref()).into_iter().find(|app| app.is_dir());
    Found { docker, desktop, opener: PathBuf::from("/usr/bin/open"), env: environment(variable, home.as_deref()) }
}

/// The folders Docker's program is looked for in, in order. An app started
/// from the Dock is given a short PATH, so the places Docker Desktop puts
/// it are added; a folder that is not a full path is passed over.
pub fn places(path: Option<&OsStr>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = match path {
        Some(path) => std::env::split_paths(path).filter(|folder| folder.is_absolute()).collect(),
        None => Vec::new(),
    };
    folders.extend(["/usr/local/bin", "/opt/homebrew/bin", "/Applications/Docker.app/Contents/Resources/bin"].map(PathBuf::from));
    if let Some(home) = home {
        folders.push(home.join(".docker").join("bin"));
    }
    folders.extend(["/usr/bin", "/bin"].map(PathBuf::from));
    let mut seen: Vec<PathBuf> = Vec::new();
    folders.retain(|folder| {
        let first = !seen.contains(folder);
        if first {
            seen.push(folder.clone());
        }
        first
    });
    folders
}

/// Where Docker Desktop may be: on a Mac, in either Applications folder.
fn desktop_places(home: Option<&Path>) -> Vec<PathBuf> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let mut apps = vec![PathBuf::from("/Applications/Docker.app")];
    if let Some(home) = home {
        apps.push(home.join("Applications").join("Docker.app"));
    }
    apps
}

/// What Docker is told about its surroundings: the home folder, the few
/// variables that say which Docker to use, and a PATH that holds Docker's
/// own helpers, such as the one that reads its sign-ins. Nothing else is
/// passed on, because Compose takes a variable from its surroundings over
/// the same one in stack.env: a POSTGRES_PASSWORD or a COMPOSE_FILE left in
/// a shell would change the stack.
fn environment(variable: impl Fn(&str) -> Option<OsString>, home: Option<&Path>) -> Vec<(String, OsString)> {
    let variable = |key: &str| variable(key).filter(|value| !value.is_empty());
    let mut env = Vec::new();
    if let Some(home) = home {
        env.push(("HOME".to_string(), home.as_os_str().to_os_string()));
    }
    for key in PASSED {
        if let Some(value) = variable(key) {
            env.push((key.to_string(), value));
        }
    }
    // A folder with a colon in its name cannot be on PATH.
    let folders = places(variable("PATH").as_deref(), home).into_iter().filter(|folder| !folder.as_os_str().as_encoded_bytes().contains(&b':'));
    if let Ok(path) = std::env::join_paths(folders) {
        env.push(("PATH".to_string(), path));
    }
    env
}

// ---------------------------------------------------------------------------
// Asking Docker
// ---------------------------------------------------------------------------

/// Docker's program, told only what `env` holds.
fn docker_command(docker: &Path, env: &[(String, OsString)]) -> Command {
    let mut command = Command::new(docker);
    command.env_clear().envs(env.iter().map(|(name, value)| (name, value)));
    command
}

/// Docker Compose, told which stack, where its settings are, and the front
/// door's port. What to do with it is added after.
pub fn compose(docker: &Path, env: &[(String, OsString)], folder: &Path, project: &str, port: u16) -> Command {
    let mut command = docker_command(docker, env);
    command.env("PORT_FRONT_DOOR", port.to_string());
    command
        .arg("compose")
        .arg("--project-name")
        .arg(project)
        .arg("--project-directory")
        .arg(folder)
        .arg("--env-file")
        .arg(folder.join("stack.env"))
        .arg("--file")
        .arg(folder.join("compose.yaml"));
    command
}

/// Docker answers.
fn docker_runs(docker: &Path, env: &[(String, OsString)]) -> bool {
    let mut command = docker_command(docker, env);
    command.args(["info", "--format", "{{.ServerVersion}}"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    run_within(&mut command, QUICK).is_some_and(|status| status.success())
}

/// Runs `command`, and ends it if it takes longer than `patience`. The
/// caller says where its output goes.
fn run_within(command: &mut Command, patience: Duration) -> Option<ExitStatus> {
    let mut child = on_its_own(command).spawn().ok()?;
    let deadline = Instant::now() + patience;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            _ => {
                end(&mut child);
                return None;
            }
        }
    }
}

/// Starts `command` in a process group of its own, so `end` can end
/// everything it starts in turn.
fn on_its_own(command: &mut Command) -> &mut Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
}

/// Ends a program the app started, and everything it started in turn:
/// `docker compose` runs Compose as a second program, which ending `docker`
/// alone would leave running.
fn end(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Ok(group) = libc::pid_t::try_from(child.id())
            && group > 1
        {
            // SAFETY: kill takes no pointers. The group is the child's own,
            // since it was started in a group of its own, and the child has
            // not been waited for yet, so its number cannot have been handed
            // to anything else.
            unsafe {
                libc::kill(-group, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

// ---------------------------------------------------------------------------
// The front door
// ---------------------------------------------------------------------------

/// The stack's front door answers on `port`, as itself.
pub fn front_door_answers(port: u16) -> bool {
    let at = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut door) = TcpStream::connect_timeout(&at, Duration::from_millis(500)) else {
        return false;
    };
    let _ = door.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = door.set_write_timeout(Some(Duration::from_secs(2)));
    let asked = format!("GET /front-door HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if door.write_all(asked.as_bytes()).is_err() {
        return false;
    }
    let mut answer = Vec::new();
    let _ = (&mut door).take(4096).read_to_end(&mut answer);
    is_front_door(&answer)
}

/// The answer is the front door's own: a 200 whose body is `ok`, as the
/// Caddyfile has it.
pub fn is_front_door(answer: &[u8]) -> bool {
    let Some(end) = answer.windows(4).position(|four| four == b"\r\n\r\n") else {
        return false;
    };
    let Ok(head) = std::str::from_utf8(&answer[..end]) else {
        return false;
    };
    let mut first = head.lines().next().unwrap_or_default().split(' ');
    let version = first.next().unwrap_or_default();
    let status = first.next().unwrap_or_default();
    version.starts_with("HTTP/1.") && status == "200" && &answer[end + 4..] == b"ok"
}

/// Something, anything, answers on `port`.
fn someone_answers(port: u16) -> bool {
    TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_millis(500)).is_ok()
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// What is true when the page asks for the stack.
#[derive(Clone, Debug)]
pub struct Facts {
    /// The front door answers.
    pub answering: bool,
    /// This app is starting the stack already.
    pub busy: bool,
    /// The app can start the stack on this system.
    pub here: bool,
    /// Something that is not the front door holds its port.
    pub port_taken: bool,
    /// Docker's program is on this computer.
    pub docker: bool,
    /// Docker answers.
    pub docker_running: bool,
    /// Docker Desktop is on this computer, so the app can open it.
    pub desktop: bool,
}

/// Why the app did not start the stack.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Why {
    NotOnThisSystem,
    PortTaken,
    NoDocker,
    DockerNotRunning,
}

/// What the app does about the stack.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// It answers, so it is left alone.
    Nothing,
    /// This app is starting it already, so wait for that.
    Wait,
    /// Something is missing, so nothing is started.
    Cannot(Why),
    /// Open Docker Desktop, wait for it, then start the stack.
    OpenDocker,
    /// Start the stack.
    Start,
}

/// The whole plan, from the facts alone.
pub fn plan(facts: &Facts) -> Step {
    if facts.answering {
        return Step::Nothing;
    }
    if facts.busy {
        return Step::Wait;
    }
    if !facts.here {
        return Step::Cannot(Why::NotOnThisSystem);
    }
    if facts.port_taken {
        return Step::Cannot(Why::PortTaken);
    }
    if !facts.docker {
        return Step::Cannot(Why::NoDocker);
    }
    if facts.docker_running {
        return Step::Start;
    }
    if facts.desktop {
        return Step::OpenDocker;
    }
    Step::Cannot(Why::DockerNotRunning)
}

/// What the page shows when the app did not start the stack.
pub fn sentence(why: Why, port: u16) -> String {
    match why {
        Why::NotOnThisSystem => "The app cannot start the stack on this system yet.".to_string(),
        Why::PortTaken => format!("Something that is not the stack is using port {port} on this computer, so the app did not start the stack."),
        Why::NoDocker => "Docker is not on this computer, so the stack is not running; the chat does not need it yet.".to_string(),
        Why::DockerNotRunning => "Docker is not running, so the app did not start the stack; start Docker, then select Try again.".to_string(),
    }
}

const DOCKER_DID_NOT_START: &str = "Docker Desktop did not start within 2 minutes; if it is asking something, answer it, then select Try again.";
const NOT_READY: &str = "The stack was not ready within 15 minutes; select Try again to go on from where it stopped.";
const NOT_ANSWERING: &str = "The stack started, and its front door is not answering yet.";
const QUITTING: &str = "The app is closing, so it did not start the stack.";

fn failed(log: &Path) -> String {
    format!("The stack did not start, and what Docker said is in {}.", log.display())
}

fn bucket_failed(log: &Path) -> String {
    format!("The stack started, and its file store could not be set up; what Docker said is in {}.", log.display())
}

fn could_not_write(error: &io::Error) -> String {
    format!("The app could not write the stack's settings, so it did not start the stack: {error}.")
}

fn could_not_log(error: &io::Error) -> String {
    format!("The app could not write the stack's log, so it did not start the stack: {error}.")
}

fn could_not_start(error: &str) -> String {
    format!("The app could not start Docker Compose: {error}.")
}

// ---------------------------------------------------------------------------
// Starting, and stopping what was started
// ---------------------------------------------------------------------------

/// What the page is told.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackReport {
    /// The front door answers.
    pub running: bool,
    /// This call started the stack, or part of it.
    pub started: bool,
    /// Where the front door is.
    pub address: String,
    /// Why it is not running, when it is not.
    pub reason: Option<String>,
}

/// The stack this app starts, and what it knows of it.
pub struct Stack {
    /// Where the stack's settings are written.
    folder: PathBuf,
    /// Where what Docker says is written.
    log: PathBuf,
    /// The front door's port.
    port: u16,
    /// The name Docker keeps the stack under.
    project: String,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// A start is under way.
    busy: bool,
    /// The app is quitting, so nothing more is started.
    quitting: bool,
    /// How to reach Docker, once this app has run Compose.
    ours: Option<Ours>,
    /// The Compose run under way, if one is.
    child: Option<Child>,
    /// Why the last start did not finish, for a caller that waited on it.
    last: Option<String>,
}

/// How this app reached Docker, so it can stop what it started.
#[derive(Clone)]
struct Ours {
    docker: PathBuf,
    env: Vec<(String, OsString)>,
}

/// How a Compose run ended.
enum Ran {
    Done,
    Failed,
    TooLong,
    /// The app was quitting before it began.
    Quitting,
    /// The app ended it, because it is quitting.
    Stopped,
    CouldNotStart(String),
}

/// The one start under way. Letting it go lets the next caller start.
/// It is never let go while the state is locked.
struct Claim<'a>(&'a Stack);

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.0.state().busy = false;
    }
}

impl Stack {
    pub fn new(folder: PathBuf, log: PathBuf, port: u16) -> Self {
        Self::named(PROJECT, folder, log, port)
    }

    fn named(project: &str, folder: PathBuf, log: PathBuf, port: u16) -> Self {
        Self { folder, log, port, project: project.to_string(), state: Mutex::new(State::default()) }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn quitting(&self) -> bool {
        self.state().quitting
    }

    fn report(&self, running: bool, started: bool, reason: Option<String>) -> StackReport {
        StackReport { running, started, address: format!("127.0.0.1:{}", self.port), reason }
    }

    /// Starts the stack if it is not running, and says how that went. It
    /// waits while the stack starts, so it is run away from the page.
    pub fn bring_up(&self, found: &Found, patience: &Patience) -> StackReport {
        let answering = front_door_answers(self.port);
        let (busy, claim) = {
            let mut state = self.state();
            if state.quitting {
                return self.report(false, false, Some(QUITTING.to_string()));
            }
            let busy = state.busy;
            let claim = (!answering && !busy).then(|| {
                state.busy = true;
                Claim(self)
            });
            (busy, claim)
        };

        let here = cfg!(unix);
        let undecided = !answering && !busy && here;
        let port_taken = undecided && someone_answers(self.port);
        let docker_running = undecided && !port_taken && found.docker.as_deref().is_some_and(|docker| docker_runs(docker, &found.env));
        let facts = Facts { answering, busy, here, port_taken, docker: found.docker.is_some(), docker_running, desktop: found.desktop.is_some() };

        let report = match plan(&facts) {
            Step::Nothing => return self.report(true, false, None),
            Step::Wait => return self.wait_for_the_other(),
            Step::Cannot(why) => self.report(false, false, Some(sentence(why, self.port))),
            Step::OpenDocker => self.carry_out(true, found, patience),
            Step::Start => self.carry_out(false, found, patience),
        };
        self.state().last = report.reason.clone();
        drop(claim);
        report
    }

    /// Waits for the start another caller began, and tells this caller how
    /// it went.
    fn wait_for_the_other(&self) -> StackReport {
        let last = loop {
            {
                let state = self.state();
                if state.quitting {
                    return self.report(false, false, Some(QUITTING.to_string()));
                }
                if !state.busy {
                    break state.last.clone();
                }
            }
            thread::sleep(PAUSE);
        };
        if front_door_answers(self.port) {
            return self.report(true, false, None);
        }
        self.report(false, false, Some(last.unwrap_or_else(|| NOT_ANSWERING.to_string())))
    }

    fn carry_out(&self, open_docker: bool, found: &Found, patience: &Patience) -> StackReport {
        let Some(docker) = found.docker.clone() else {
            return self.report(false, false, Some(sentence(Why::NoDocker, self.port)));
        };
        if let Err(error) = self.write_settings() {
            return self.report(false, false, Some(could_not_write(&error)));
        }
        if let Err(error) = self.open_log(false) {
            return self.report(false, false, Some(could_not_log(&error)));
        }
        let ours = Ours { docker, env: found.env.clone() };

        if open_docker && !self.open_docker(found, patience) {
            let reason = if self.quitting() { QUITTING } else { DOCKER_DID_NOT_START };
            return self.report(false, false, Some(reason.to_string()));
        }

        match self.run_ours(&ours, &UP, patience.up) {
            Ran::Done => {}
            Ran::Failed => return self.report(false, true, Some(failed(&self.log))),
            Ran::TooLong => return self.report(false, true, Some(NOT_READY.to_string())),
            Ran::Quitting => return self.report(false, false, Some(QUITTING.to_string())),
            Ran::Stopped => return self.report(false, true, Some(QUITTING.to_string())),
            Ran::CouldNotStart(error) => return self.report(false, false, Some(could_not_start(&error))),
        }
        match self.run_ours(&ours, &BUCKET, patience.job) {
            Ran::Done => {}
            Ran::Quitting | Ran::Stopped => return self.report(false, true, Some(QUITTING.to_string())),
            Ran::Failed | Ran::TooLong => return self.report(false, true, Some(bucket_failed(&self.log))),
            Ran::CouldNotStart(error) => return self.report(false, true, Some(could_not_start(&error))),
        }

        let deadline = Instant::now() + patience.door;
        loop {
            if front_door_answers(self.port) {
                return self.report(true, true, None);
            }
            if self.quitting() {
                return self.report(false, true, Some(QUITTING.to_string()));
            }
            if Instant::now() >= deadline {
                return self.report(false, true, Some(NOT_ANSWERING.to_string()));
            }
            thread::sleep(PAUSE);
        }
    }

    /// Opens Docker Desktop and waits for Docker to answer.
    fn open_docker(&self, found: &Found, patience: &Patience) -> bool {
        let (Some(desktop), Some(docker)) = (&found.desktop, &found.docker) else {
            return false;
        };
        let mut opener = Command::new(&found.opener);
        opener.env_clear().envs(found.env.iter().map(|(name, value)| (name, value)));
        opener.arg(desktop).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        run_within(&mut opener, QUICK);

        let deadline = Instant::now() + patience.docker;
        loop {
            if docker_runs(docker, &found.env) {
                return true;
            }
            let now = Instant::now();
            if self.quitting() || now >= deadline {
                return false;
            }
            thread::sleep(Duration::from_secs(1).min(deadline - now));
        }
    }

    /// Runs Compose with `what`, its output added to the log. The app keeps
    /// hold of the run, so quitting can end it.
    fn run_ours(&self, ours: &Ours, what: &[&str], patience: Duration) -> Ran {
        {
            let mut state = self.state();
            if state.quitting {
                return Ran::Quitting;
            }
            let (out, err) = match self.open_log(true).and_then(|log| Ok((log.try_clone()?, log))) {
                Ok(both) => both,
                Err(error) => return Ran::CouldNotStart(error.to_string()),
            };
            let mut command = compose(&ours.docker, &ours.env, &self.folder, &self.project, self.port);
            command.args(what).stdin(Stdio::null()).stdout(out).stderr(err);
            match on_its_own(&mut command).spawn() {
                Ok(child) => {
                    state.child = Some(child);
                    state.ours = Some(ours.clone());
                }
                Err(error) => return Ran::CouldNotStart(error.to_string()),
            }
        }

        let deadline = Instant::now() + patience;
        loop {
            {
                let mut state = self.state();
                let Some(child) = state.child.as_mut() else {
                    return Ran::Stopped;
                };
                match child.try_wait() {
                    Ok(Some(status)) => {
                        state.child = None;
                        return if status.success() { Ran::Done } else { Ran::Failed };
                    }
                    Ok(None) if Instant::now() < deadline => {}
                    Ok(None) => {
                        end(child);
                        state.child = None;
                        return Ran::TooLong;
                    }
                    Err(_) => {
                        end(child);
                        state.child = None;
                        return Ran::Failed;
                    }
                }
            }
            thread::sleep(PAUSE);
        }
    }

    /// Writes the stack and the front door's settings, readable by Docker,
    /// and, on the first start only, stack.env with new passwords, readable
    /// by this user only. Later starts keep stack.env as it is, since the
    /// database was set up with its passwords.
    fn write_settings(&self) -> io::Result<()> {
        fs::create_dir_all(&self.folder)?;
        write_shared(&self.folder.join("compose.yaml"), COMPOSE)?;
        write_shared(&self.folder.join("Caddyfile"), CADDYFILE)?;

        let settings = self.folder.join("stack.env");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&settings) {
            Ok(mut file) => file.write_all(new_settings().as_bytes())?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&settings, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// The log, new for each start, then added to by each run.
    fn open_log(&self, add: bool) -> io::Result<File> {
        if let Some(folder) = self.log.parent() {
            fs::create_dir_all(folder)?;
        }
        let mut options = OpenOptions::new();
        options.create(true);
        if add {
            options.append(true).truncate(false);
        } else {
            options.write(true).truncate(true);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            options.mode(0o600);
            let log = options.open(&self.log)?;
            log.set_permissions(fs::Permissions::from_mode(0o600))?;
            Ok(log)
        }
        #[cfg(not(unix))]
        {
            options.open(&self.log)
        }
    }

    /// Stops the stack this app started, if it did, and ends a Compose run
    /// under way. Nothing more is started after this.
    pub fn stop(&self) {
        let (child, ours) = {
            let mut state = self.state();
            state.quitting = true;
            (state.child.take(), state.ours.take())
        };
        if let Some(mut child) = child {
            end(&mut child);
        }
        let Some(ours) = ours else {
            return;
        };
        let mut command = compose(&ours.docker, &ours.env, &self.folder, &self.project, self.port);
        command.args(STOP).stdin(Stdio::null());
        match self.open_log(true).and_then(|log| Ok((log.try_clone()?, log))) {
            Ok((out, err)) => command.stdout(out).stderr(err),
            Err(_) => command.stdout(Stdio::null()).stderr(Stdio::null()),
        };
        run_within(&mut command, STOP_PATIENCE);
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Writes a file the stack's containers read, such as the Caddyfile, which
/// Caddy reads as a user of its own.
fn write_shared(path: &Path, text: &str) -> io::Result<()> {
    fs::write(path, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o644))?;
    }
    Ok(())
}

/// stack.env for a first start. Each password is 64 hex characters from two
/// random UUIDs, which come from the system's random source.
fn new_settings() -> String {
    let password = || format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    format!(
        "# The stack's settings, made on its first start. Only this user can read this file.\n\
         POSTGRES_DB=rusty\n\
         POSTGRES_USER=rusty\n\
         POSTGRES_PASSWORD={}\n\
         REDIS_PASSWORD={}\n\
         MINIO_ROOT_USER=rusty\n\
         MINIO_ROOT_PASSWORD={}\n\
         MINIO_BUCKET=rusty-business\n",
        password(),
        password(),
        password()
    )
}

/// Starts the stack when it is not running. The page asks when it opens,
/// and when Try again is selected.
#[tauri::command]
pub async fn start_stack(app: tauri::AppHandle) -> Result<StackReport, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Stack>().bring_up(&look(), &Patience::USUAL))
        .await
        .map_err(|error| format!("The app could not start the stack: {error}."))
}

#[cfg(test)]
mod tests {
    // Brad, on what the app does by itself: "it should search and up the local
    // models and stack." and "install and go, one click ready". These tests
    // cover the "up" for the stack. The Docker they run is a small shell
    // script that stands in for it and writes down what it is asked, and every
    // port is one the system hands out as free, never the stack's own. One
    // test starts the real stack, under a name, a folder, and a port of its
    // own; it runs only when asked for, with `cargo test -- --ignored`.

    use std::collections::HashMap;

    use super::*;

    // Where the app is put together. No test can run the whole app, so two
    // tests read the file.
    const PUT_TOGETHER: &str = include_str!("lib.rs");

    fn scratch() -> PathBuf {
        let folder = std::env::temp_dir().join(format!("rusty-business-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    /// Everything in place, and nothing answering.
    fn ready() -> Facts {
        Facts { answering: false, busy: false, here: true, port_taken: false, docker: true, docker_running: true, desktop: true }
    }

    /// A settings file as name and value, without its comments.
    fn settings_in(text: &str) -> HashMap<String, String> {
        text.lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn only_what_is_missing_is_started() {
        assert_eq!(plan(&ready()), Step::Start);

        // A front door that answers is left alone, whatever else is true.
        assert_eq!(plan(&Facts { answering: true, ..ready() }), Step::Nothing);
        let nothing_else = Facts { answering: true, busy: true, here: false, port_taken: true, docker: false, docker_running: false, desktop: false };
        assert_eq!(plan(&nothing_else), Step::Nothing);

        // A start under way is waited for, not begun again.
        assert_eq!(plan(&Facts { busy: true, ..ready() }), Step::Wait);
        assert_eq!(plan(&Facts { busy: true, here: false, port_taken: true, ..ready() }), Step::Wait);

        // Each thing that is missing stops the plan before anything starts.
        assert_eq!(plan(&Facts { here: false, ..ready() }), Step::Cannot(Why::NotOnThisSystem));
        assert_eq!(plan(&Facts { port_taken: true, ..ready() }), Step::Cannot(Why::PortTaken));
        assert_eq!(plan(&Facts { docker: false, docker_running: false, ..ready() }), Step::Cannot(Why::NoDocker));
        assert_eq!(plan(&Facts { docker: false, docker_running: false, desktop: false, ..ready() }), Step::Cannot(Why::NoDocker));

        // Docker that is not running is opened, when Docker Desktop is here.
        assert_eq!(plan(&Facts { docker_running: false, ..ready() }), Step::OpenDocker);
        assert_eq!(plan(&Facts { docker_running: false, desktop: false, ..ready() }), Step::Cannot(Why::DockerNotRunning));

        // Docker that runs needs no Docker Desktop.
        assert_eq!(plan(&Facts { desktop: false, ..ready() }), Step::Start);
    }

    #[test]
    fn every_reason_is_one_plain_sentence() {
        let log = Path::new("/Users/someone/Library/Application Support/net.morrical.rusty-business/stack.log");
        let error = io::Error::from(io::ErrorKind::PermissionDenied);
        let mut said: Vec<String> =
            [Why::NotOnThisSystem, Why::PortTaken, Why::NoDocker, Why::DockerNotRunning].into_iter().map(|why| sentence(why, FRONT_DOOR_PORT)).collect();
        said.extend([DOCKER_DID_NOT_START, NOT_READY, NOT_ANSWERING, QUITTING].map(String::from));
        said.extend([failed(log), bucket_failed(log), could_not_write(&error), could_not_log(&error), could_not_start("No such file or directory")]);
        for one in &said {
            assert!(one.ends_with('.') && !one.contains('\n'), "{one:?}");
            assert!(one.starts_with(char::is_uppercase), "{one:?}");
            assert_eq!(one.matches(". ").count(), 0, "one sentence: {one:?}");
        }
        assert!(sentence(Why::PortTaken, 4701).contains("port 4701"));
        assert!(failed(log).ends_with("Application Support/net.morrical.rusty-business/stack.log."));

        // The waits the sentences name are the waits the app keeps.
        assert!(DOCKER_DID_NOT_START.contains("within 2 minutes"));
        assert_eq!(Patience::USUAL.docker, Duration::from_secs(2 * 60));
        assert!(NOT_READY.contains("within 15 minutes"));
        assert_eq!(Patience::USUAL.up, Duration::from_secs(15 * 60));
    }

    #[test]
    fn the_page_is_told_under_the_names_it_reads() {
        let stack = Stack::new(PathBuf::from("/nowhere/stack"), PathBuf::from("/nowhere/stack.log"), FRONT_DOOR_PORT);
        assert_eq!(
            serde_json::to_value(stack.report(true, true, None)).unwrap(),
            serde_json::json!({ "running": true, "started": true, "address": "127.0.0.1:4700", "reason": null })
        );
        assert_eq!(
            serde_json::to_string(&stack.report(false, false, Some(sentence(Why::NoDocker, FRONT_DOOR_PORT)))).unwrap(),
            r#"{"running":false,"started":false,"address":"127.0.0.1:4700","reason":"Docker is not on this computer, so the stack is not running; the chat does not need it yet."}"#
        );
    }

    #[test]
    fn compose_is_told_which_stack_and_where_its_settings_are() {
        let folder = Path::new("/Users/someone/Library/Application Support/net.morrical.rusty-business/stack");
        let env = vec![("HOME".to_string(), OsString::from("/Users/someone"))];
        let mut command = compose(Path::new("/usr/local/bin/docker"), &env, folder, PROJECT, FRONT_DOOR_PORT);
        command.args(UP);
        assert_eq!(command.get_program(), "/usr/local/bin/docker");
        let said: Vec<String> = command.get_args().map(|part| part.to_string_lossy().into_owned()).collect();
        let at = |name: &str| folder.join(name).display().to_string();
        assert_eq!(
            said,
            [
                "compose",
                "--project-name",
                "rusty-business-app",
                "--project-directory",
                &folder.display().to_string(),
                "--env-file",
                &at("stack.env"),
                "--file",
                &at("compose.yaml"),
                "up",
                "--detach",
                "--wait",
                "--quiet-pull",
            ]
        );
        let set: Vec<(&OsStr, Option<&OsStr>)> = command.get_envs().collect();
        assert!(set.contains(&(OsStr::new("PORT_FRONT_DOOR"), Some(OsStr::new("4700")))), "{set:?}");
        assert!(set.contains(&(OsStr::new("HOME"), Some(OsStr::new("/Users/someone")))), "{set:?}");
    }

    #[test]
    fn docker_is_looked_for_in_order() {
        // A folder on PATH that is not a full path is passed over, and a
        // folder is looked in once.
        assert_eq!(
            places(Some(OsStr::new("/opt/tools/bin:tools/bin::/usr/local/bin:.:/usr/bin")), Some(Path::new("/Users/someone"))),
            [
                "/opt/tools/bin",
                "/usr/local/bin",
                "/usr/bin",
                "/opt/homebrew/bin",
                "/Applications/Docker.app/Contents/Resources/bin",
                "/Users/someone/.docker/bin",
                "/bin",
            ]
            .map(PathBuf::from)
        );
        assert_eq!(
            places(None, None),
            ["/usr/local/bin", "/opt/homebrew/bin", "/Applications/Docker.app/Contents/Resources/bin", "/usr/bin", "/bin"].map(PathBuf::from)
        );
    }

    #[test]
    fn docker_is_told_only_what_it_needs() {
        let shell: HashMap<&str, &str> = [
            ("PATH", "/opt/tools/bin:/usr/bin"),
            ("USER", "someone"),
            ("TMPDIR", "/var/folders/xy/T/"),
            ("DOCKER_CONTEXT", "desktop-linux"),
            ("DOCKER_HOST", ""),
            ("POSTGRES_PASSWORD", "from-a-shell"),
            ("COMPOSE_PROJECT_NAME", "someone-else"),
            ("COMPOSE_FILE", "/tmp/other.yaml"),
            ("PORT_FRONT_DOOR", "9999"),
        ]
        .into();
        let variable = |key: &str| shell.get(key).map(OsString::from);

        let env = environment(variable, Some(Path::new("/Users/someone")));
        let names: Vec<&str> = env.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["HOME", "USER", "TMPDIR", "DOCKER_CONTEXT", "PATH"], "an empty one is left out too");
        let told: HashMap<&str, &OsStr> = env.iter().map(|(name, value)| (name.as_str(), value.as_os_str())).collect();
        assert_eq!(told["HOME"], "/Users/someone");
        assert_eq!(
            told["PATH"],
            "/opt/tools/bin:/usr/bin:/usr/local/bin:/opt/homebrew/bin:/Applications/Docker.app/Contents/Resources/bin:/Users/someone/.docker/bin:/bin"
        );

        // A home folder with a colon in its name stays off PATH.
        let env = environment(|_: &str| None, Some(Path::new("/Users/some:one")));
        assert_eq!(env[0], ("HOME".to_string(), OsString::from("/Users/some:one")));
        assert_eq!(env[1], ("PATH".to_string(), OsString::from("/usr/local/bin:/opt/homebrew/bin:/Applications/Docker.app/Contents/Resources/bin:/usr/bin:/bin")));
    }

    #[test]
    fn docker_desktop_is_looked_for_where_a_mac_keeps_apps() {
        let home = Path::new("/Users/someone");
        if cfg!(target_os = "macos") {
            assert_eq!(desktop_places(Some(home)), [PathBuf::from("/Applications/Docker.app"), home.join("Applications").join("Docker.app")]);
            assert_eq!(desktop_places(None), [PathBuf::from("/Applications/Docker.app")]);
        } else {
            assert_eq!(desktop_places(Some(home)), Vec::<PathBuf>::new());
        }
    }

    #[test]
    fn only_the_front_door_counts_as_the_front_door() {
        let caddy = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nServer: Caddy\r\nDate: Mon, 16 Nov 2026 12:00:00 GMT\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
        assert!(is_front_door(caddy));
        assert!(is_front_door(b"HTTP/1.0 200\r\n\r\nok"));

        let others: [(&str, &[u8]); 8] = [
            ("not found", b"HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\n\r\nok"),
            ("another body", b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nokay"),
            ("an engine", b"HTTP/1.1 200 OK\r\n\r\n{\"object\":\"list\",\"data\":[]}"),
            ("HTTP/2", b"HTTP/2 200\r\n\r\nok"),
            ("not quite 200", b"HTTP/1.1 2000 OK\r\n\r\nok"),
            ("no blank line", b"HTTP/1.1 200 OK\r\nok"),
            ("not text", b"\xff\xfe 200 OK\r\n\r\nok"),
            ("nothing", b""),
        ];
        for (what, answer) in others {
            assert!(!is_front_door(answer), "{what}");
        }
        assert!(CADDYFILE.contains("respond /front-door \"ok\" 200"), "the check above reads what the Caddyfile answers");
    }

    #[test]
    fn only_the_front_door_has_a_port_and_nothing_is_built() {
        assert_eq!(COMPOSE.matches("ports:").count(), 1, "the front door's, and no other");
        assert!(COMPOSE.contains(&format!("- \"127.0.0.1:${{PORT_FRONT_DOOR:-{FRONT_DOOR_PORT}}}:8080\"\n")), "on this computer only");
        assert!(!COMPOSE.contains("build:"));
        assert!(COMPOSE.contains(&format!("\nname: {PROJECT}\n")), "the same name the app gives Compose");
        let pulled: Vec<&str> = COMPOSE.lines().filter(|line| line.trim_start().starts_with("image: ghcr.io/")).collect();
        assert_eq!(pulled.len(), 2, "{pulled:?}");
        for line in pulled {
            assert!(line.contains("@sha256:"), "pinned by digest: {line}");
        }
    }

    #[test]
    fn every_setting_the_stack_insists_on_is_made() {
        let settings = settings_in(&new_settings());
        let insisted: Vec<&str> = COMPOSE.split("${").skip(1).filter_map(|rest| rest.split_once(":?")).map(|(name, _)| name).collect();
        assert!(insisted.len() >= 6, "{insisted:?}");
        for name in insisted {
            assert!(settings.contains_key(name), "{name}");
        }

        let passwords: Vec<&String> = ["POSTGRES_PASSWORD", "REDIS_PASSWORD", "MINIO_ROOT_PASSWORD"].map(|name| &settings[name]).into();
        for password in &passwords {
            assert_eq!(password.len(), 64, "{password}");
            assert!(password.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{password}");
        }
        assert!(passwords[0] != passwords[1] && passwords[1] != passwords[2] && passwords[0] != passwords[2]);
        assert_ne!(settings_in(&new_settings())["POSTGRES_PASSWORD"], settings["POSTGRES_PASSWORD"], "new ones each time it is made");
    }

    #[test]
    fn the_page_can_ask_for_the_stack_to_be_started() {
        let commands = PUT_TOGETHER.split_once("generate_handler![").and_then(|(_, rest)| rest.split_once(']')).expect("the list of commands").0;
        assert!(commands.contains("stack::start_stack"), "{commands}");
        assert!(PUT_TOGETHER.contains("app.manage(stack::Stack::new("), "the command needs the stack the app keeps");
    }

    #[test]
    fn the_app_stops_its_stack_when_it_quits() {
        let on_quitting = PUT_TOGETHER.split_once("RunEvent::Exit").expect("the app is told when it quits").1;
        assert!(on_quitting.contains("stack.stop();"), "{on_quitting}");
    }

    #[cfg(unix)]
    mod with_docker {
        // These run real processes, so they take turns.

        use std::net::TcpListener;
        use std::os::unix::fs::PermissionsExt;

        use super::*;

        static TURN: Mutex<()> = Mutex::new(());

        fn my_turn() -> MutexGuard<'static, ()> {
            TURN.lock().unwrap_or_else(PoisonError::into_inner)
        }

        const INFO: &str = "info --format {{.ServerVersion}}";
        const UP_LINE: &str = "up --detach --wait --quiet-pull";
        const BUCKET_LINE: &str = "run --rm -T --quiet-pull minio-bucket";
        const STOP_LINE: &str = "stop";

        const QUICKLY: Patience =
            Patience { docker: Duration::from_millis(300), up: Duration::from_secs(10), job: Duration::from_secs(10), door: Duration::from_secs(5) };

        const CADDY_OK: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nServer: Caddy\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
        const NOT_A_FRONT_DOOR: &str = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

        /// A stand-in for Docker. Each time it is run it adds a line to a
        /// file beside it (what it was told) and writes down what it was
        /// told about its surroundings. It answers `info` only once its
        /// `.running` file is there. For Compose, it skips the nine words
        /// that name the stack, then does `up` or `run`; anything else it
        /// does nothing with.
        fn docker_in(folder: &Path, up: &str, run: &str) -> PathBuf {
            fs::create_dir_all(folder).unwrap();
            let file = folder.join("docker");
            let script = format!(
                "#!/bin/sh\necho \"$*\" >> \"$0.runs\"\nenv | sort > \"$0.env\"\ncase \"$1\" in info) [ -e \"$0.running\" ]; exit $? ;; esac\nshift 9\ncase \"$1\" in up) {up} ;; run) {run} ;; esac\n"
            );
            fs::write(&file, script).unwrap();
            fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
            file
        }

        /// A stand-in for `open`: it writes down what it was told, then does
        /// `then`.
        fn opener_in(folder: &Path, then: &str) -> PathBuf {
            fs::create_dir_all(folder).unwrap();
            let file = folder.join("open");
            fs::write(&file, format!("#!/bin/sh\necho \"$*\" >> \"$0.runs\"\n{then}\n")).unwrap();
            fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
            file
        }

        fn beside(program: &Path, ending: &str) -> PathBuf {
            PathBuf::from(format!("{}.{ending}", program.display()))
        }

        /// Docker answers from now on.
        fn docker_is_running(docker: &Path) {
            fs::write(beside(docker, "running"), "").unwrap();
        }

        /// One line for each time the stand-in was run.
        fn runs(program: &Path) -> Vec<String> {
            fs::read_to_string(beside(program, "runs")).unwrap_or_default().lines().map(String::from).collect()
        }

        /// What each run was asked to do, after the words that name the stack.
        fn asked(docker: &Path) -> Vec<String> {
            runs(docker).iter().map(|run| run.split_once("compose.yaml ").map_or(run.as_str(), |(_, what)| what).to_string()).collect()
        }

        fn found(place: &Path, docker: Option<&Path>, desktop: Option<&Path>, opener: &Path) -> Found {
            Found {
                docker: docker.map(Path::to_path_buf),
                desktop: desktop.map(Path::to_path_buf),
                opener: opener.to_path_buf(),
                env: vec![("HOME".to_string(), place.as_os_str().to_os_string()), ("PATH".to_string(), OsString::from("/usr/bin:/bin"))],
            }
        }

        /// The app's data folder, with a space in its name as on a Mac.
        fn data(place: &Path) -> PathBuf {
            place.join("Application Support")
        }

        fn stack_at(place: &Path, port: u16) -> Stack {
            Stack::named("rusty-business-test", data(place).join("stack"), data(place).join("stack.log"), port)
        }

        fn mode(path: &Path) -> u32 {
            fs::metadata(path).unwrap().permissions().mode() & 0o777
        }

        /// A port on this computer that nothing is using.
        fn free_port() -> u16 {
            TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
        }

        /// Answers every caller at `listener` with `answer`, for as long as
        /// the tests run.
        fn answer_at(listener: TcpListener, answer: &'static str) {
            thread::spawn(move || {
                for mut caller in listener.incoming().flatten() {
                    let _ = caller.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut asked = [0u8; 4096];
                    let _ = caller.read(&mut asked);
                    let _ = caller.write_all(answer.as_bytes());
                }
            });
        }

        /// Opens a front door on `port` once the stand-in has been told `up`,
        /// as the real stack's opens once it is healthy.
        fn door_after_up(docker: &Path, port: u16) {
            let docker = docker.to_path_buf();
            thread::spawn(move || {
                while !asked(&docker).iter().any(|what| what == UP_LINE) {
                    thread::sleep(Duration::from_millis(20));
                }
                answer_at(TcpListener::bind(("127.0.0.1", port)).unwrap(), CADDY_OK);
            });
        }

        fn alive(process: &str) -> bool {
            Command::new("ps").args(["-p", process]).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap().success()
        }

        /// The process the stand-in left running in the background, once it
        /// has written its number down.
        fn grandchild(docker: &Path) -> String {
            let noted = beside(docker, "grandchild");
            let deadline = Instant::now() + Duration::from_secs(5);
            while !noted.exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
            fs::read_to_string(noted).unwrap().trim().to_string()
        }

        fn gone_soon(process: &str) -> bool {
            let deadline = Instant::now() + Duration::from_secs(3);
            while alive(process) {
                if Instant::now() >= deadline {
                    return false;
                }
                thread::sleep(Duration::from_millis(50));
            }
            true
        }

        /// Something a test started that is not the app's, standing for
        /// anything else that is running on the computer.
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

        #[test]
        fn the_whole_plan_is_checked_before_anything_starts() {
            let _turn = my_turn();
            let taken = TcpListener::bind("127.0.0.1:0").unwrap();
            let taken_port = taken.local_addr().unwrap().port();
            answer_at(taken, NOT_A_FRONT_DOOR);

            for what in ["Docker is not on this computer", "the port is taken", "Docker is not running and Docker Desktop is not here"] {
                let place = scratch();
                let docker = docker_in(&place.join("tools"), "exit 0", "exit 0");
                let opener = opener_in(&place.join("tools"), "exit 0");
                let free = free_port();
                let (port, found, reason, expected): (u16, Found, String, &[&str]) = match what {
                    "Docker is not on this computer" => (free, found(&place, None, None, &opener), sentence(Why::NoDocker, free), &[]),
                    "the port is taken" => {
                        docker_is_running(&docker);
                        (taken_port, found(&place, Some(&docker), None, &opener), sentence(Why::PortTaken, taken_port), &[])
                    }
                    _ => (free, found(&place, Some(&docker), None, &opener), sentence(Why::DockerNotRunning, free), &[INFO]),
                };
                let stack = stack_at(&place, port);
                let report = stack.bring_up(&found, &QUICKLY);
                assert_eq!(report, StackReport { running: false, started: false, address: format!("127.0.0.1:{port}"), reason: Some(reason) }, "{what}");
                assert_eq!(asked(&docker), expected, "{what}");
                assert!(!data(&place).exists(), "no settings, no log, and no folder for them: {what}");
                assert_eq!(runs(&opener), Vec::<String>::new(), "{what}");

                stack.stop();
                assert_eq!(asked(&docker), expected, "nothing was started, so nothing is stopped: {what}");
            }
        }

        #[test]
        fn the_stack_is_started_once_and_stopped_by_name_only() {
            let _turn = my_turn();
            let place = scratch();
            let docker = docker_in(&place.join("tools"), "exit 0", "exit 0");
            docker_is_running(&docker);
            let port = free_port();
            door_after_up(&docker, port);
            let stack = stack_at(&place, port);
            let found = found(&place, Some(&docker), None, &place.join("tools").join("open"));

            let report = stack.bring_up(&found, &QUICKLY);
            assert_eq!(report, StackReport { running: true, started: true, address: format!("127.0.0.1:{port}"), reason: None });
            assert_eq!(asked(&docker), [INFO, UP_LINE, BUCKET_LINE]);

            // Compose is told the stack's name and where its settings are.
            let folder = data(&place).join("stack");
            assert_eq!(
                runs(&docker)[1],
                format!("compose --project-name rusty-business-test --project-directory {f} --env-file {f}/stack.env --file {f}/compose.yaml {UP_LINE}", f = folder.display())
            );

            // The stack and the front door, as built in, readable by Docker.
            assert_eq!(fs::read_to_string(folder.join("compose.yaml")).unwrap(), COMPOSE);
            assert_eq!(fs::read_to_string(folder.join("Caddyfile")).unwrap(), CADDYFILE);
            assert_eq!(mode(&folder.join("compose.yaml")), 0o644);
            assert_eq!(mode(&folder.join("Caddyfile")), 0o644);

            // The passwords and the log, for this user only.
            assert_eq!(mode(&folder.join("stack.env")), 0o600);
            assert_eq!(mode(&data(&place).join("stack.log")), 0o600);
            let settings = settings_in(&fs::read_to_string(folder.join("stack.env")).unwrap());
            let passwords: Vec<&String> = ["POSTGRES_PASSWORD", "REDIS_PASSWORD", "MINIO_ROOT_PASSWORD"].map(|name| &settings[name]).into();
            assert!(passwords.iter().all(|password| password.len() == 64), "{passwords:?}");
            assert!(passwords[0] != passwords[1] && passwords[1] != passwords[2] && passwords[0] != passwords[2]);

            // Docker is told nothing of the app's own surroundings but what it
            // was given, and the port. (sh sets PWD, SHLVL, and _ for itself.)
            let told = fs::read_to_string(beside(&docker, "env")).unwrap();
            let names: Vec<&str> =
                told.lines().filter_map(|line| line.split_once('=')).map(|(name, _)| name).filter(|name| !["PWD", "SHLVL", "_"].contains(name)).collect();
            assert_eq!(names, ["HOME", "PATH", "PORT_FRONT_DOOR"], "{told}");
            assert!(told.lines().any(|line| line == format!("PORT_FRONT_DOOR={port}")), "{told}");

            // Asking again changes nothing.
            let again = stack.bring_up(&found, &QUICKLY);
            assert_eq!(again, StackReport { running: true, started: false, address: format!("127.0.0.1:{port}"), reason: None });
            assert_eq!(runs(&docker).len(), 3);

            // Quitting stops it, and removes nothing.
            stack.stop();
            assert_eq!(asked(&docker), [INFO, UP_LINE, BUCKET_LINE, STOP_LINE]);
            stack.stop();
            assert_eq!(runs(&docker).len(), 4, "a second stop runs nothing");
            assert!(folder.join("stack.env").exists());
        }

        #[test]
        fn the_passwords_are_made_once_and_kept_for_this_user() {
            let _turn = my_turn();
            let place = scratch();
            let folder = data(&place).join("stack");

            let docker = docker_in(&place.join("first"), "exit 0", "exit 0");
            docker_is_running(&docker);
            let port = free_port();
            door_after_up(&docker, port);
            assert!(stack_at(&place, port).bring_up(&found(&place, Some(&docker), None, &docker), &QUICKLY).running);
            let made = fs::read_to_string(folder.join("stack.env")).unwrap();

            // Someone loosens stack.env and changes the stack's file.
            fs::set_permissions(folder.join("stack.env"), fs::Permissions::from_mode(0o644)).unwrap();
            fs::write(folder.join("compose.yaml"), "services: {}\n").unwrap();

            let docker = docker_in(&place.join("second"), "exit 0", "exit 0");
            docker_is_running(&docker);
            let port = free_port();
            door_after_up(&docker, port);
            assert!(stack_at(&place, port).bring_up(&found(&place, Some(&docker), None, &docker), &QUICKLY).running);

            assert_eq!(fs::read_to_string(folder.join("stack.env")).unwrap(), made, "the database was set up with these");
            assert_eq!(mode(&folder.join("stack.env")), 0o600);
            assert_eq!(fs::read_to_string(folder.join("compose.yaml")).unwrap(), COMPOSE);
        }

        #[test]
        fn four_callers_at_once_start_one_stack() {
            let _turn = my_turn();
            let place = scratch();
            let docker = docker_in(&place.join("tools"), "sleep 0.5", "exit 0");
            docker_is_running(&docker);
            let port = free_port();
            door_after_up(&docker, port);
            let stack = stack_at(&place, port);
            let found = found(&place, Some(&docker), None, &docker);

            let reports: Vec<StackReport> = thread::scope(|scope| {
                let callers: Vec<_> = (0..4).map(|_| scope.spawn(|| stack.bring_up(&found, &QUICKLY))).collect();
                callers.into_iter().map(|caller| caller.join().unwrap()).collect()
            });
            assert!(reports.iter().all(|report| report.running && report.reason.is_none()), "{reports:?}");
            assert_eq!(reports.iter().filter(|report| report.started).count(), 1, "{reports:?}");
            assert_eq!(asked(&docker), [INFO, UP_LINE, BUCKET_LINE]);
        }

        #[test]
        fn a_front_door_that_answers_is_left_alone() {
            let _turn = my_turn();
            let place = scratch();
            let docker = docker_in(&place.join("tools"), "exit 0", "exit 0");
            docker_is_running(&docker);
            let door = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = door.local_addr().unwrap().port();
            answer_at(door, CADDY_OK);
            let stack = stack_at(&place, port);

            let report = stack.bring_up(&found(&place, Some(&docker), None, &docker), &QUICKLY);
            assert_eq!(report, StackReport { running: true, started: false, address: format!("127.0.0.1:{port}"), reason: None });
            assert_eq!(runs(&docker), Vec::<String>::new());
            assert!(!data(&place).exists());

            stack.stop();
            assert_eq!(runs(&docker), Vec::<String>::new(), "one that was already running is not the app's to stop");
        }

        #[test]
        fn docker_desktop_is_opened_when_docker_is_not_running() {
            let _turn = my_turn();
            let place = scratch();
            let docker = docker_in(&place.join("tools"), "exit 0", "exit 0");
            let opener = opener_in(&place.join("tools"), &format!("touch \"{}\"", beside(&docker, "running").display()));
            let desktop = place.join("Applications").join("Docker.app");
            let port = free_port();
            door_after_up(&docker, port);
            let stack = stack_at(&place, port);

            let report = stack.bring_up(&found(&place, Some(&docker), Some(&desktop), &opener), &QUICKLY);
            assert_eq!(report, StackReport { running: true, started: true, address: format!("127.0.0.1:{port}"), reason: None });
            assert_eq!(runs(&opener), [desktop.display().to_string()]);
            assert_eq!(asked(&docker), [INFO, INFO, UP_LINE, BUCKET_LINE]);
            stack.stop();
            assert!(beside(&docker, "running").exists(), "Docker is left running");

            // Docker Desktop that does not start in time: nothing is started.
            let place = scratch();
            let docker = docker_in(&place.join("tools"), "exit 0", "exit 0");
            let opener = opener_in(&place.join("tools"), "exit 0");
            let stack = stack_at(&place, free_port());
            let report = stack.bring_up(&found(&place, Some(&docker), Some(&desktop), &opener), &QUICKLY);
            assert_eq!(report.reason.as_deref(), Some(DOCKER_DID_NOT_START));
            assert!(!report.running && !report.started);
            assert_eq!(runs(&opener).len(), 1);
            assert!(asked(&docker).len() >= 2 && asked(&docker).iter().all(|what| what == INFO), "{:?}", asked(&docker));
            stack.stop();
            assert!(asked(&docker).iter().all(|what| what == INFO), "nothing was started, so nothing is stopped");
        }

        #[test]
        fn a_failed_start_says_where_docker_said_why() {
            let _turn = my_turn();
            let place = scratch();
            let docker = docker_in(&place.join("tools"), "echo 'pull access denied' >&2; exit 1", "exit 0");
            docker_is_running(&docker);
            let stack = stack_at(&place, free_port());
            let log = data(&place).join("stack.log");

            let report = stack.bring_up(&found(&place, Some(&docker), None, &docker), &QUICKLY);
            assert_eq!(report.reason, Some(failed(&log)));
            assert!(!report.running && report.started, "part of it may have started");
            assert_eq!(asked(&docker), [INFO, UP_LINE]);
            assert!(fs::read_to_string(&log).unwrap().contains("pull access denied"));
            assert_eq!(mode(&log), 0o600);
            stack.stop();
            assert_eq!(asked(&docker).last().map(String::as_str), Some(STOP_LINE), "what part of it started is stopped");

            // The bucket job fails once the stack is up.
            let place = scratch();
            let docker = docker_in(&place.join("tools"), "exit 0", "echo 'bucket refused' >&2; exit 1");
            docker_is_running(&docker);
            let port = free_port();
            door_after_up(&docker, port);
            let stack = stack_at(&place, port);
            let log = data(&place).join("stack.log");
            let report = stack.bring_up(&found(&place, Some(&docker), None, &docker), &QUICKLY);
            assert_eq!(report.reason, Some(bucket_failed(&log)));
            assert!(report.started);
            assert!(fs::read_to_string(&log).unwrap().contains("bucket refused"));
        }

        #[test]
        fn a_start_that_takes_too_long_is_ended_with_all_it_started() {
            let _turn = my_turn();
            let place = scratch();
            let docker = docker_in(&place.join("tools"), "sleep 600 & echo $! > \"$0.grandchild\"; wait", "exit 0");
            docker_is_running(&docker);
            let stack = stack_at(&place, free_port());
            let patience = Patience { up: Duration::from_secs(1), ..QUICKLY };

            let report = stack.bring_up(&found(&place, Some(&docker), None, &docker), &patience);
            assert_eq!(report.reason.as_deref(), Some(NOT_READY));
            assert!(!report.running && report.started);
            let left = grandchild(&docker);
            assert!(gone_soon(&left), "Compose, and what it started, is ended");
        }

        #[test]
        fn quitting_ends_a_start_and_stops_only_what_it_started() {
            let _turn = my_turn();
            let place = scratch();
            let docker = docker_in(&place.join("tools"), "sleep 600 & echo $! > \"$0.grandchild\"; wait", "exit 0");
            docker_is_running(&docker);
            let stack = stack_at(&place, free_port());
            let found = found(&place, Some(&docker), None, &docker);
            let patience = Patience { up: Duration::from_secs(60), ..QUICKLY };
            let mut bystander = Bystander::start();

            let report = thread::scope(|scope| {
                let starting = scope.spawn(|| stack.bring_up(&found, &patience));
                let left = grandchild(&docker);
                stack.stop();
                assert!(gone_soon(&left), "the start under way is ended");
                starting.join().unwrap()
            });
            assert_eq!(report.reason.as_deref(), Some(QUITTING));
            assert!(!report.running && report.started);
            assert_eq!(asked(&docker), [INFO, UP_LINE, STOP_LINE]);
            assert!(bystander.running(), "nothing that is not the app's is ended");

            // Nothing more is started once the app is quitting.
            let report = stack.bring_up(&found, &patience);
            assert_eq!(report.reason.as_deref(), Some(QUITTING));
            assert!(!report.started);
            assert_eq!(runs(&docker).len(), 3);
        }

        #[test]
        #[ignore = "starts the real stack in Docker, under a name, a folder, and a port of its own"]
        fn live_start_on_a_free_port() {
            let place = scratch();
            let port = free_port();
            assert!(port != FRONT_DOOR_PORT && port != 8080, "never the stack's own port, or one in use here");
            let project = format!("rusty-business-test-{}", uuid::Uuid::new_v4().simple());
            let folder = data(&place).join("stack");
            let found = look();
            let docker = found.docker.clone().expect("Docker is on this computer");

            /// Removes the test's own stack, volumes and all, however the
            /// test ends. Nothing else is touched.
            struct Own<'a> {
                docker: &'a Path,
                env: &'a [(String, OsString)],
                folder: &'a Path,
                project: &'a str,
                port: u16,
            }
            impl Drop for Own<'_> {
                fn drop(&mut self) {
                    assert!(self.project.starts_with("rusty-business-test-"));
                    let mut command = compose(self.docker, self.env, self.folder, self.project, self.port);
                    command.args(["down", "--volumes"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
                    run_within(&mut command, Duration::from_secs(120));
                }
            }
            let _own = Own { docker: &docker, env: &found.env, folder: &folder, project: &project, port };

            let stack = Stack::named(&project, folder.clone(), data(&place).join("stack.log"), port);
            let report = stack.bring_up(&found, &Patience::USUAL);
            let log = fs::read_to_string(data(&place).join("stack.log")).unwrap_or_default();
            assert_eq!(report, StackReport { running: true, started: true, address: format!("127.0.0.1:{port}"), reason: None }, "{log}");
            assert!(front_door_answers(port));

            stack.stop();
            assert!(!front_door_answers(port), "the app stops the stack it started");
        }
    }
}
