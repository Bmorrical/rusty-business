//! The outbound gate.
//!
//! It answers two asks: "there is a hard switch / circuit breaker to hard stop
//! all outbound communication, user can turn it on to use their inference",
//! and "off by default, everything stops". The app is local first: a chat
//! with a model on this computer needs nothing outside it, so it works with
//! the gate off. Everything that would reach past this computer waits for the
//! gate.
//!
//! The rules, each with a test:
//!
//! - The gate is off until the person turns it on, and it stays as they left
//!   it the next time the app opens. Anything but a kept "on" reads as off.
//! - Every connection past this computer is made here, and only while the
//!   gate is on, over https, with no proxy. The one other client in the app
//!   speaks to the model engine on this computer.
//! - Turning the gate off stops what is under way: the check for a new
//!   version, a model download, and a start of the stack.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::watch;

use crate::downloads::Downloads;

/// Whether the app may reach past this computer, kept in the app's own folder.
#[derive(Clone)]
pub struct Gate {
    file: PathBuf,
    open: Arc<watch::Sender<bool>>,
}

/// The words the gate's file holds while it is on.
const ON: &str = "on";
const OFF: &str = "off";

fn word(open: bool) -> &'static str {
    if open { ON } else { OFF }
}

impl Gate {
    /// The gate as the person last left it. With nothing kept, or anything
    /// but "on", it is off.
    pub fn new(file: PathBuf) -> Self {
        let open = fs::read_to_string(&file).is_ok_and(|text| text.trim() == ON);
        Self { file, open: Arc::new(watch::Sender::new(open)) }
    }

    pub fn is_open(&self) -> bool {
        *self.open.borrow()
    }

    /// Waits until the gate is off. It is ready at once when the gate is
    /// already off.
    pub async fn closed(&self) {
        let mut open = self.open.subscribe();
        let _ = open.wait_for(|open| !*open).await;
    }

    /// Turns the gate on or off, and keeps that for the next time the app
    /// opens. The gate changes even when it cannot be kept.
    pub fn set(&self, open: bool) -> Result<bool, String> {
        self.open.send_replace(open);
        let kept = match self.file.parent() {
            Some(folder) => fs::create_dir_all(folder),
            None => Ok(()),
        }
        .and_then(|()| fs::write(&self.file, word(open)));
        kept.map(|()| open).map_err(|error| format!("The gate is {}, but the app could not keep that for next time: {error}.", word(open)))
    }

    /// A client for reaching past this computer: only while the gate is on,
    /// only over https, and never through a proxy. `refused` says what waits
    /// while the gate is off; `shape` adds the caller's own rules.
    pub fn client(&self, refused: &str, shape: impl FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder) -> Result<reqwest::Client, String> {
        if !self.is_open() {
            return Err(refused.to_string());
        }
        shape(reqwest::Client::builder())
            .https_only(true)
            .no_proxy()
            .referer(false)
            .build()
            .map_err(|error| format!("Could not set up the connection: {error}"))
    }

    /// A gate that keeps nothing, for tests.
    #[cfg(test)]
    pub fn at(open: bool) -> Self {
        Self { file: PathBuf::from("/nowhere/gate"), open: Arc::new(watch::Sender::new(open)) }
    }
}

/// Whether the gate is on.
#[tauri::command]
pub fn gate_status(gate: tauri::State<'_, Gate>) -> bool {
    gate.is_open()
}

/// Turns the gate on or off. Off stops a model download that is under way;
/// the check for a new version and a start of the stack stop by themselves.
#[tauri::command]
pub fn set_gate(open: bool, gate: tauri::State<'_, Gate>, downloads: tauri::State<'_, Downloads>) -> Result<bool, String> {
    let set = gate.set(open);
    if !open {
        downloads.stop();
    }
    set
}

#[cfg(test)]
mod tests {
    // Brad, 2026-11-17: "there is a hard switch / circuit breaker to hard stop
    // all outbound communication, user can turn it on to use their
    // inference." Then: "off by default, everything stops". And on the
    // scope: "it's no longer "local" only, but it's "local" first."

    use super::*;
    use std::time::Duration;
    use tauri::async_runtime::block_on;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("rusty-gate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn the_gate_is_off_by_default() {
        let place = folder("default");
        assert!(!Gate::new(place.join("gate")).is_open(), "nothing kept");
        fs::create_dir_all(&place).unwrap();
        for kept in ["", "off", "yes", "true", "1", "ON", "on please", "\u{0}"] {
            fs::write(place.join("gate"), kept).unwrap();
            assert!(!Gate::new(place.join("gate")).is_open(), "{kept:?} read as on");
        }
        fs::write(place.join("gate"), "on\n").unwrap();
        assert!(Gate::new(place.join("gate")).is_open());
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn the_gate_stays_as_it_was_left() {
        let place = folder("kept");
        let gate = Gate::new(place.join("app").join("gate"));
        assert_eq!(gate.set(true), Ok(true));
        assert!(gate.is_open());
        assert!(Gate::new(place.join("app").join("gate")).is_open(), "on was not kept");
        assert_eq!(gate.set(false), Ok(false));
        assert!(!Gate::new(place.join("app").join("gate")).is_open(), "off was not kept");
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn a_gate_that_cannot_be_kept_still_changes_and_says_so() {
        let place = folder("unkept");
        fs::create_dir_all(&place).unwrap();
        // A file where the app's folder should be: nothing can be kept.
        fs::write(place.join("app"), "").unwrap();
        let gate = Gate::new(place.join("app").join("gate"));
        let error = gate.set(true).unwrap_err();
        assert!(error.starts_with("The gate is on, but the app could not keep that for next time: "), "{error}");
        assert!(gate.is_open());
        let error = gate.set(false).unwrap_err();
        assert!(error.starts_with("The gate is off, but"), "{error}");
        assert!(!gate.is_open());
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn a_closed_gate_makes_no_connection() {
        let refused = "The gate is off. Turn it on to try this.";
        let mut shaped = false;
        let error = Gate::at(false)
            .client(refused, |builder| {
                shaped = true;
                builder
            })
            .unwrap_err();
        assert_eq!(error, refused);
        assert!(!shaped, "a client was put together while the gate was off");
        assert!(Gate::at(true).client(refused, |builder| builder).is_ok());
    }

    #[test]
    fn an_open_gate_still_goes_over_https_only() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        // A caller cannot loosen it: the gate's own rules come last.
        let client = Gate::at(true).client("", |builder| builder.https_only(false)).unwrap();
        // Were plain http let through, the stand-in would never answer, so the
        // wait is short and the test fails rather than hangs.
        let error = block_on(client.get(format!("http://127.0.0.1:{port}/")).timeout(Duration::from_secs(5)).send()).unwrap_err();
        assert!(error.is_builder(), "{error}");
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err(), "the client reached the stand-in over plain http");
    }

    #[test]
    fn turning_the_gate_off_is_heard_at_once() {
        let gate = Gate::at(true);
        let listening = gate.clone();
        let heard = std::thread::spawn(move || block_on(async { tokio::time::timeout(Duration::from_secs(30), listening.closed()).await }));
        std::thread::sleep(Duration::from_millis(50));
        gate.set(false).unwrap_err();
        assert!(heard.join().unwrap().is_ok(), "turning the gate off was not heard");
        // Already off: nothing to wait for.
        block_on(async { tokio::time::timeout(Duration::from_secs(5), Gate::at(false).closed()).await }).unwrap();
    }

    #[test]
    fn every_connection_past_this_computer_goes_through_the_gate() {
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut builders = Vec::new();
        for entry in fs::read_dir(&folder).unwrap() {
            let file = entry.unwrap().path();
            if file.extension().is_none_or(|kind| kind != "rs") {
                continue;
            }
            let text = fs::read_to_string(&file).unwrap();
            // The tests below a file's code build their own stand-ins.
            let code = text.split("#[cfg(test)]\nmod tests").next().unwrap();
            if code.contains("Client::builder()") || code.contains("Client::new()") || code.contains("reqwest::get(") {
                builders.push(file.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
        builders.sort();
        // The engine's client speaks to the engine on this computer only.
        assert_eq!(builders, ["engine.rs", "gate.rs"]);
        let engine = fs::read_to_string(folder.join("engine.rs")).unwrap();
        assert!(engine.contains("ip.is_loopback()"), "the engine's address is not held to this computer");
    }
}
