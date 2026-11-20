//! Independent gates for internet services and the configured LAN engine.
//! Both default to off and keep their settings separately.

use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::watch;

use crate::downloads::Downloads;

/// The WAN gate, kept in the app's own folder.
#[derive(Clone)]
pub struct Gate {
    file: PathBuf,
    state: Arc<watch::Sender<GateState>>,
}

#[derive(Clone, Copy)]
struct GateState {
    open: bool,
    closures: u64,
}

pub struct LanGate(Gate);

const LAN_OFF: &str = "The LAN gate is off. Turn it on to use your LAN server.";
const LAN_CLOSED: &str = "The LAN gate was turned off. The connection to your LAN server stopped.";

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
        Self {
            file,
            state: Arc::new(watch::Sender::new(GateState { open, closures: 0 })),
        }
    }

    pub fn is_open(&self) -> bool {
        self.state.borrow().open
    }

    /// Waits until the gate is off. It is ready at once when the gate is
    /// already off.
    pub async fn closed(&self) {
        let mut state = self.state.subscribe();
        let closures = state.borrow().closures;
        let _ = state
            .wait_for(|state| !state.open || state.closures != closures)
            .await;
    }

    /// Turns the gate on or off, and keeps that for the next time the app
    /// opens. The gate changes even when it cannot be kept.
    pub fn set(&self, open: bool) -> Result<bool, String> {
        self.state.send_modify(|state| {
            if state.open && !open {
                state.closures = state.closures.wrapping_add(1);
            }
            state.open = open;
        });
        let kept = match self.file.parent() {
            Some(folder) => fs::create_dir_all(folder),
            None => Ok(()),
        }
        .and_then(|()| fs::write(&self.file, word(open)));
        kept.map(|()| open).map_err(|error| {
            format!(
                "The gate is {}, but the app could not keep that for next time: {error}.",
                word(open)
            )
        })
    }

    /// A client for reaching past this computer: only while the gate is on,
    /// only over https, and never through a proxy. `refused` says what waits
    /// while the gate is off; `shape` adds the caller's own rules.
    pub fn client(
        &self,
        refused: &str,
        shape: impl FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder,
    ) -> Result<reqwest::Client, String> {
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
        Self {
            file: PathBuf::from("/nowhere/gate"),
            state: Arc::new(watch::Sender::new(GateState { open, closures: 0 })),
        }
    }
}

impl LanGate {
    pub fn new(file: PathBuf) -> Self {
        Self(Gate::new(file))
    }

    /// Drop the entire request or stream when LAN access is withdrawn.
    pub async fn run<T>(
        &self,
        request: impl Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        let mut state = self.0.state.subscribe();
        let initial = *state.borrow_and_update();
        if !initial.open {
            return Err(LAN_OFF.to_string());
        }
        tokio::select! {
            biased;
            _ = state.wait_for(|state| state.closures != initial.closures) => Err(LAN_CLOSED.to_string()),
            result = request => result,
        }
    }
}

/// Whether the gate is on.
#[tauri::command]
pub fn gate_status(gate: tauri::State<'_, Gate>) -> bool {
    gate.is_open()
}

/// Turns the gate on or off. Off stops a model download that is under way;
/// the check for a new version, a start of the stack, and an outside engine's
/// answer stop by themselves.
#[tauri::command]
pub fn set_gate(
    open: bool,
    gate: tauri::State<'_, Gate>,
    downloads: tauri::State<'_, Downloads>,
) -> Result<bool, String> {
    let set = gate.set(open);
    if !open {
        downloads.stop_local();
    }
    set
}

#[tauri::command]
pub fn lan_gate_status(gate: tauri::State<'_, LanGate>) -> bool {
    gate.0.is_open()
}

#[tauri::command]
pub fn set_lan_gate(open: bool, gate: tauri::State<'_, LanGate>) -> Result<bool, String> {
    gate.0.set(open)
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
            assert!(
                !Gate::new(place.join("gate")).is_open(),
                "{kept:?} read as on"
            );
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
        assert!(
            Gate::new(place.join("app").join("gate")).is_open(),
            "on was not kept"
        );
        assert_eq!(gate.set(false), Ok(false));
        assert!(
            !Gate::new(place.join("app").join("gate")).is_open(),
            "off was not kept"
        );
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn lan_and_wan_keep_independent_preferences() {
        let place = folder("independent");
        let wan_file = place.join("gate");
        let lan_file = place.join("lan-gate");
        let wan = Gate::new(wan_file.clone());
        let lan = LanGate::new(lan_file.clone());
        assert!(!wan.is_open());
        assert!(!lan.0.is_open());

        wan.set(true).unwrap();
        assert!(Gate::new(wan_file).is_open());
        assert!(!LanGate::new(lan_file.clone()).0.is_open());
        assert_eq!(block_on(lan.run(async { Ok(()) })).unwrap_err(), LAN_OFF);

        wan.set(false).unwrap();
        lan.0.set(true).unwrap();
        assert!(!wan.is_open());
        assert!(LanGate::new(lan_file).0.is_open());
        assert_eq!(block_on(lan.run(async { Ok(42) })), Ok(42));
        fs::remove_dir_all(place).unwrap();
    }

    #[test]
    fn lan_closure_cancels_an_in_flight_request_even_if_reopened() {
        let place = folder("lan-closure");
        let lan = LanGate::new(place.join("lan-gate"));
        lan.0.set(true).unwrap();
        let result = block_on(lan.run(async {
            lan.0.set(false).unwrap();
            lan.0.set(true).unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }));
        assert_eq!(result.unwrap_err(), LAN_CLOSED);
        assert_eq!(block_on(lan.run(async { Ok(()) })), Ok(()));
        fs::remove_dir_all(place).unwrap();
    }

    #[test]
    fn a_gate_that_cannot_be_kept_still_changes_and_says_so() {
        let place = folder("unkept");
        fs::create_dir_all(&place).unwrap();
        // A file where the app's folder should be: nothing can be kept.
        fs::write(place.join("app"), "").unwrap();
        let gate = Gate::new(place.join("app").join("gate"));
        let error = gate.set(true).unwrap_err();
        assert!(
            error.starts_with("The gate is on, but the app could not keep that for next time: "),
            "{error}"
        );
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
        let client = Gate::at(true)
            .client("", |builder| builder.https_only(false))
            .unwrap();
        // Were plain http let through, the stand-in would never answer, so the
        // wait is short and the test fails rather than hangs.
        let error = block_on(
            client
                .get(format!("http://127.0.0.1:{port}/"))
                .timeout(Duration::from_secs(5))
                .send(),
        )
        .unwrap_err();
        assert!(error.is_builder(), "{error}");
        listener.set_nonblocking(true).unwrap();
        assert!(
            listener.accept().is_err(),
            "the client reached the stand-in over plain http"
        );
    }

    #[test]
    fn turning_the_gate_off_is_heard_at_once() {
        let gate = Gate::at(true);
        let listening = gate.clone();
        let heard = std::thread::spawn(move || {
            block_on(async {
                tokio::time::timeout(Duration::from_secs(30), listening.closed()).await
            })
        });
        std::thread::sleep(Duration::from_millis(50));
        gate.set(false).unwrap_err();
        assert!(
            heard.join().unwrap().is_ok(),
            "turning the gate off was not heard"
        );
        // Already off: nothing to wait for.
        block_on(async {
            tokio::time::timeout(Duration::from_secs(5), Gate::at(false).closed()).await
        })
        .unwrap();
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
            if code.contains("Client::builder()")
                || code.contains("Client::new()")
                || code.contains("reqwest::get(")
            {
                builders.push(file.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
        builders.sort();
        // The engine adapter validates addresses and gates private ones.
        assert_eq!(builders, ["engine.rs", "gate.rs"]);
        let engine = fs::read_to_string(folder.join("engine.rs")).unwrap();
        assert!(
            engine.contains("ip.is_loopback()"),
            "the engine no longer distinguishes loopback"
        );
        assert!(engine.contains("ip.is_private()"));
    }
}
