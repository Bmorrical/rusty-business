//! The runners: a slice handed to a model, on its time limit.
//!
//! A slice in the sliced lane can be handed to a model on this computer, on
//! the LAN, or on the rig. The model is sent the slice's brief and writes
//! the work. Every rule in `rules.rs` is checked before the model is asked,
//! and again when it is done:
//!
//! - A model outside this computer is never a runner: direct API
//!   connections are refused, and a slice is words only.
//! - The runner's clock starts once it holds its turn, and it is stopped at
//!   the time limit (`rules::ON_TIME`). A runner stopped there has drifted:
//!   its work is set aside, and the slice goes back to you to be cut smaller.
//! - Work that is allowed is kept in the board's own database beside the
//!   board, never in the board's file, and the slice moves to written.
//! - A run you stop, a model that writes nothing, or an engine that fails
//!   moves nothing.
//!
//! The orchestrator never merges: a runner only ever takes a slice into
//! written.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;
use tokio::sync::watch;

use crate::board::{Boards, Local, Work};
use crate::engine::{self, ChatDone, ChatEvent, ChatMessage, ChatRequest, Chats};
use crate::gate::LanGate;
use crate::outside;
use crate::rig::{self, Rig};
use crate::rules::{self, Move, Rules, Verdict};

/// How far past the time limit a runner is stopped, so one that is stopped
/// there is always past it.
pub const GRACE: Duration = Duration::from_secs(1);

/// What the page is told when a model outside this computer is picked.
pub const OUTSIDE: &str = "A slice goes only to a model on this computer or on your network. Direct API connections are refused.";
/// What the page is told when a model that reads pictures is picked.
pub const WORDS_ONLY: &str = "A slice is words only, so it goes to a model that writes words.";
/// What the page is told when a second runner is asked for the same slice.
pub const ALREADY: &str = "A model is already writing that slice.";
/// What the page is told when the runner wrote nothing.
pub const NOTHING: &str = "The model wrote nothing, so the slice stays where it is.";
/// What the page is told when the person stopped the runner.
pub const STOPPED: &str =
    "You stopped the model. The slice stays where it is, and nothing it wrote is kept.";
/// What the page is told when a runner was stopped at its limit with the
/// time rule turned off before it was done.
pub const STOPPED_AT_LIMIT: &str =
    "The model ran past its time limit and was stopped. The slice stays where it is.";

/// What the runner's ask came to, and how long the runner had the turn.
#[derive(Clone, Debug, Default)]
pub struct Timed {
    /// `None` when the runner was stopped at its time limit.
    pub done: Option<ChatDone>,
    pub took_ms: u64,
}

impl Timed {
    /// A run stopped before it took its turn.
    pub fn stopped() -> Self {
        Timed {
            done: Some(ChatDone {
                stopped: true,
                ..ChatDone::default()
            }),
            took_ms: 0,
        }
    }
}

fn ms(span: Duration) -> u64 {
    u64::try_from(span.as_millis()).unwrap_or(u64::MAX)
}

/// Runs an ask on the clock. With a cut-off, the ask is stopped there, and
/// what it wrote is dropped; with none, it runs until it is done or stopped.
pub(crate) async fn timed(
    cut_off: Option<Duration>,
    ask: impl Future<Output = Result<ChatDone, String>>,
) -> Result<Timed, String> {
    let start = Instant::now();
    let Some(cut_off) = cut_off else {
        let done = ask.await?;
        return Ok(Timed {
            done: Some(done),
            took_ms: ms(start.elapsed()),
        });
    };
    match tokio::time::timeout(cut_off, ask).await {
        Ok(done) => Ok(Timed {
            done: Some(done?),
            took_ms: ms(start.elapsed()),
        }),
        Err(_) => Ok(Timed {
            done: None,
            took_ms: ms(start.elapsed()).max(ms(cut_off)),
        }),
    }
}

/// Whether a model may be a runner. A model outside this computer never is,
/// nor one that reads pictures. A model on the rig is checked by the rig.
pub fn runnable(model: &str) -> Result<(), String> {
    if outside::is_outside(model) {
        Err(OUTSIDE.to_string())
    } else if rig::is_rig(model) {
        Ok(())
    } else if engine::reads_pictures(model) {
        Err(WORDS_ONLY.to_string())
    } else if engine::catalog_entry(model).is_none() {
        Err("That model is not on this app's list.".to_string())
    } else {
        Ok(())
    }
}

/// The runs going now, so one can be stopped, and none is run twice.
#[derive(Default)]
pub struct Runs {
    going: Mutex<HashMap<String, watch::Sender<bool>>>,
}

/// A run while it goes. It leaves the runs when it ends, however it ends.
struct Going<'a> {
    runs: &'a Runs,
    key: String,
}

impl Drop for Going<'_> {
    fn drop(&mut self) {
        self.runs.held().remove(&self.key);
    }
}

fn run_key(id: &str, story: u64, slice: &str) -> String {
    format!("{id}/{story}/{slice}")
}

impl Runs {
    fn held(&self) -> std::sync::MutexGuard<'_, HashMap<String, watch::Sender<bool>>> {
        self.going.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn start(&self, key: String) -> Result<(Going<'_>, watch::Receiver<bool>), String> {
        let mut going = self.held();
        if going.contains_key(&key) {
            return Err(ALREADY.to_string());
        }
        let (stop, stopped) = watch::channel(false);
        going.insert(key.clone(), stop);
        Ok((Going { runs: self, key }, stopped))
    }

    /// Stops a run. Says whether one was going.
    pub fn stop(&self, id: &str, story: u64, slice: &str) -> bool {
        match self.held().get(&run_key(id, story, slice)) {
            Some(stop) => {
                let _ = stop.send(true);
                true
            }
            None => false,
        }
    }

    /// The slices being written now, as "board/story/slice".
    pub fn going(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.held().keys().cloned().collect();
        keys.sort();
        keys
    }
}

/// What a run came to, for the page.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ran {
    /// True when the slice moved to written.
    pub moved: bool,
    /// What happened, in a line.
    pub words: String,
    /// What the rules said, when they were asked.
    pub verdict: Option<Verdict>,
    /// The board as it is now, when the run changed it.
    pub local: Option<Local>,
    /// The work kept, when it was.
    pub work: Option<Work>,
    pub took_ms: u64,
}

fn refused(verdict: Verdict) -> Ran {
    Ran {
        words: verdict
            .refusals
            .iter()
            .map(|refusal| refusal.words.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        verdict: Some(verdict),
        ..Ran::default()
    }
}

/// The brief a runner is sent: what it is, then the story and the slice.
fn brief(
    id: &str,
    boards: &Boards,
    story: u64,
    slice: &str,
    model: &str,
) -> Result<(ChatRequest, String), String> {
    let board = boards
        .local(id)?
        .ok_or("That board is not on this computer.")?;
    let held = board
        .epics
        .iter()
        .flat_map(|epic| epic.stories.iter())
        .find(|found| found.number == story)
        .ok_or("That story is not on this board.")?;
    let piece = held
        .slices
        .iter()
        .find(|found| found.id == slice)
        .ok_or("That slice is not on this story.")?;
    let mut told = format!("Story {}: {}\n", held.number, held.title.trim());
    if !held.detail.trim().is_empty() {
        told.push_str(&format!("\n{}\n", held.detail.trim()));
    }
    told.push_str(&format!("\nSlice {}: {}\n", piece.id, piece.title.trim()));
    if !piece.detail.trim().is_empty() {
        told.push_str(&format!("\n{}\n", piece.detail.trim()));
    }
    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: "You are a runner on a workflow board. You are given one slice of a story: a small brief to finish in one sitting. Write the work the slice asks for, in full, and nothing else. When it names files, write each file's change under the file's name.".to_string(),
                images: Vec::new(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: told,
                images: Vec::new(),
            },
        ],
        max_tokens: None,
    };
    Ok((request, piece.lane.clone()))
}

fn by_model(took_ms: Option<u64>) -> Move {
    Move {
        to: "written".to_string(),
        by: "model".to_string(),
        took_ms,
        ..Move::default()
    }
}

/// Hands a slice to a model, on its time limit, and keeps what it wrote
/// when the rules allow it. `ask` puts the brief to the model, with the
/// cut-off, and reports what it wrote and how long it took.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run<A, F>(
    boards: &Boards,
    rules: &Rules,
    runs: &Runs,
    id: &str,
    story: u64,
    slice: &str,
    model: &str,
    ask: A,
) -> Result<Ran, String>
where
    A: FnOnce(ChatRequest, Option<Duration>, watch::Receiver<bool>) -> F,
    F: Future<Output = Result<Timed, String>>,
{
    runnable(model)?;
    let (request, lane) = brief(id, boards, story, slice, model)?;
    // Before any model is asked: a slice not the model's to write stays.
    let before = rules.check(&Move {
        from: lane,
        ..by_model(None)
    })?;
    if !before.allowed {
        return Ok(refused(before));
    }
    let view = rules.view();
    let on_time = view
        .rules
        .iter()
        .any(|rule| rule.id == rules::ON_TIME && rule.on);
    let cut_off = on_time.then(|| Duration::from_secs(u64::from(view.limit_minutes) * 60) + GRACE);

    let (going, stopped) = runs.start(run_key(id, story, slice))?;
    let timed = ask(request, cut_off, stopped).await;
    drop(going);
    let timed = timed?;
    let took_ms = timed.took_ms;

    let Some(done) = timed.done else {
        // Stopped at the limit: the runner has drifted, when the time rule
        // is still on. Its work was never read, so none is kept.
        let step = by_model(Some(took_ms));
        let lane = brief(id, boards, story, slice, model)?.1;
        let verdict = rules.check(&Move {
            from: lane,
            ..step.clone()
        })?;
        if !verdict.split {
            return Ok(Ran {
                words: STOPPED_AT_LIMIT.to_string(),
                took_ms,
                ..Ran::default()
            });
        }
        let taken = rules::take(boards, rules, id, story, slice, step)?;
        return Ok(Ran {
            took_ms,
            local: Some(taken.local),
            ..refused(taken.verdict)
        });
    };
    if done.stopped {
        return Ok(Ran {
            words: STOPPED.to_string(),
            took_ms,
            ..Ran::default()
        });
    }
    if done.answer.trim().is_empty() {
        return Ok(Ran {
            words: NOTHING.to_string(),
            took_ms,
            ..Ran::default()
        });
    }

    let step = by_model(Some(took_ms));
    let lane = brief(id, boards, story, slice, model)?.1;
    let after = rules.check(&Move {
        from: lane,
        ..step.clone()
    })?;
    // The work is kept only when the move is allowed, and before the slice
    // moves, so a slice in written always has what was written for it.
    let work = if after.allowed {
        Some(boards.keep_work(
            id,
            story,
            slice,
            Work {
                model: model.to_string(),
                took_ms,
                text: done.answer,
                ..Work::default()
            },
        )?)
    } else {
        None
    };
    let taken = rules::take(boards, rules, id, story, slice, step)?;
    let moved = taken.verdict.allowed;
    Ok(Ran {
        moved,
        words: if moved {
            "The model wrote the slice. It is in written, for the check.".to_string()
        } else {
            refused(taken.verdict.clone()).words
        },
        verdict: Some(taken.verdict),
        local: Some(taken.local),
        work: work.filter(|_| moved),
        took_ms,
    })
}

/// Hands a slice to a model. The page names the model; the move is always
/// the model's, into written, and never further.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn run_slice(
    boards: tauri::State<'_, Boards>,
    rules: tauri::State<'_, Rules>,
    runs: tauri::State<'_, Runs>,
    chats: tauri::State<'_, Chats>,
    lan: tauri::State<'_, LanGate>,
    rig: tauri::State<'_, Rig>,
    id: String,
    story: u64,
    slice: String,
    model: String,
    on_event: Channel<ChatEvent>,
) -> Result<Ran, String> {
    let lan: &LanGate = &lan;
    let chats: &Chats = &chats;
    let rig: &Rig = &rig;
    let emit = move |event| {
        let _ = on_event.send(event);
    };
    run(
        &boards,
        &rules,
        &runs,
        &id,
        story,
        &slice,
        &model,
        |request, cut_off, mut stopped| async move {
            if rig::is_rig(&request.model) {
                timed(
                    cut_off,
                    rig::run_chat(lan, rig, &request, &mut stopped, emit),
                )
                .await
            } else {
                engine::runner_ask(lan, chats, &request, cut_off, &mut stopped, emit).await
            }
        },
    )
    .await
}

#[tauri::command]
pub fn stop_run(runs: tauri::State<'_, Runs>, id: String, story: u64, slice: String) -> bool {
    runs.stop(&id, story, &slice)
}

#[tauri::command]
pub fn runs_going(runs: tauri::State<'_, Runs>) -> Vec<String> {
    runs.going()
}

#[tauri::command]
pub fn slice_work(
    boards: tauri::State<'_, Boards>,
    id: String,
    story: u64,
    slice: String,
) -> Result<Option<Work>, String> {
    boards.work(&id, story, &slice)
}

#[cfg(test)]
mod tests {
    // Brad, 2026-11-27, on what is left to build: "The orchestrator: handing
    // slices to runners, with their time limits." On drift: "Time". On the
    // API: "direct API connections are refused as rule for now".

    use super::*;
    use std::fs;
    use std::path::PathBuf;

    const MODEL: &str = "phi-4";

    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("rusty-runners-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn sample(name: &str) -> (Boards, Rules) {
        let place = folder(name);
        let boards = Boards::new(place.join("boards"));
        boards.sample().unwrap();
        (boards, Rules::new(place.join("rules.json")))
    }

    fn lane_of(boards: &Boards, story: u64, slice: &str) -> (String, String, usize) {
        let board = boards.local("sample").unwrap().unwrap();
        let held = board
            .epics
            .iter()
            .flat_map(|epic| epic.stories.iter())
            .find(|found| found.number == story)
            .and_then(|found| found.slices.iter().find(|piece| piece.id == slice))
            .unwrap();
        (held.lane.clone(), held.turn.clone(), held.notes.len())
    }

    fn answered(text: &str, took_ms: u64) -> Timed {
        Timed {
            done: Some(ChatDone {
                answer: text.to_string(),
                ..ChatDone::default()
            }),
            took_ms,
        }
    }

    fn go(
        boards: &Boards,
        rules: &Rules,
        runs: &Runs,
        slice: &str,
        model: &str,
        timed: Timed,
    ) -> Result<Ran, String> {
        tauri::async_runtime::block_on(run(
            boards,
            rules,
            runs,
            "sample",
            412,
            slice,
            model,
            |_, _, _| async move { Ok(timed) },
        ))
    }

    #[test]
    fn a_runner_in_time_writes_the_slice_and_its_work_is_kept() {
        let (boards, rules) = sample("allowed");
        let runs = Runs::default();
        let ran = go(
            &boards,
            &rules,
            &runs,
            "d",
            MODEL,
            answered("The keys sort.", 60_000),
        )
        .unwrap();
        assert!(ran.moved, "{}", ran.words);
        assert_eq!(lane_of(&boards, 412, "d").0, "written");
        assert_eq!(lane_of(&boards, 412, "d").1, "check", "the check is next");
        let kept = boards.work("sample", 412, "d").unwrap().unwrap();
        assert_eq!(kept.text, "The keys sort.");
        assert_eq!(kept.model, MODEL);
        assert_eq!(kept.took_ms, 60_000);
        assert_eq!(ran.work, Some(kept));
        assert!(runs.going().is_empty(), "a finished run was left going");
    }

    #[test]
    fn a_runner_stopped_at_its_limit_has_drifted_and_nothing_is_kept() {
        let (boards, rules) = sample("drift");
        let runs = Runs::default();
        let limit = u64::from(rules.view().limit_minutes) * 60_000;
        let cut = Timed {
            done: None,
            took_ms: limit + ms(GRACE),
        };
        let notes = lane_of(&boards, 412, "d").2;
        let ran = go(&boards, &rules, &runs, "d", MODEL, cut).unwrap();
        assert!(!ran.moved);
        assert!(
            ran.verdict.as_ref().unwrap().split,
            "a cut-off runner did not drift"
        );
        let (lane, turn, after) = lane_of(&boards, 412, "d");
        assert_eq!(lane, "sliced", "a drifted runner moved the slice");
        assert_eq!(turn, "you", "a drifted slice goes back to you");
        assert_eq!(after, notes + 1, "the drift is not in the notes");
        assert_eq!(
            boards.work("sample", 412, "d").unwrap(),
            None,
            "a drifted runner's work was kept"
        );
        // Past the limit with an answer is a drift too.
        let late = go(
            &boards,
            &rules,
            &runs,
            "d",
            MODEL,
            answered("Late.", limit + 1),
        );
        assert!(late.unwrap().verdict.unwrap().split);
        assert_eq!(boards.work("sample", 412, "d").unwrap(), None);
    }

    #[test]
    fn the_cut_off_is_the_limit_and_a_moment_more() {
        let (boards, rules) = sample("cutoff");
        let runs = Runs::default();
        let mut seen = None;
        tauri::async_runtime::block_on(run(
            &boards,
            &rules,
            &runs,
            "sample",
            412,
            "d",
            MODEL,
            |_, cut_off, _| {
                seen = Some(cut_off);
                async { Ok(answered("x", 1)) }
            },
        ))
        .unwrap();
        assert_eq!(seen, Some(Some(Duration::from_secs(30 * 60) + GRACE)));
        rules.set(rules::ON_TIME, false).unwrap();
        seen = None;
        tauri::async_runtime::block_on(run(
            &boards,
            &rules,
            &runs,
            "sample",
            420,
            "a",
            MODEL,
            |_, cut_off, _| {
                seen = Some(cut_off);
                async { Ok(answered("x", 1)) }
            },
        ))
        .unwrap();
        assert_eq!(
            seen,
            Some(None),
            "with the time rule off, a runner has no cut-off"
        );
    }

    #[test]
    fn the_clock_stops_an_ask_at_the_cut_off() {
        let slow = async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(ChatDone::default())
        };
        let started = Instant::now();
        let cut =
            tauri::async_runtime::block_on(timed(Some(Duration::from_millis(50)), slow)).unwrap();
        assert!(cut.done.is_none(), "the ask ran past its cut-off");
        assert!(cut.took_ms >= 50);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the clock did not stop it"
        );
        let quick = tauri::async_runtime::block_on(timed(Some(Duration::from_secs(5)), async {
            Ok(ChatDone {
                answer: "done".to_string(),
                ..ChatDone::default()
            })
        }))
        .unwrap();
        assert_eq!(quick.done.unwrap().answer, "done");
    }

    #[test]
    fn a_run_you_stop_or_a_model_that_writes_nothing_moves_nothing() {
        let (boards, rules) = sample("stopped");
        let runs = Runs::default();
        let ran = go(&boards, &rules, &runs, "d", MODEL, Timed::stopped()).unwrap();
        assert_eq!(ran.words, STOPPED);
        let empty = go(&boards, &rules, &runs, "d", MODEL, answered("  \n", 10)).unwrap();
        assert_eq!(empty.words, NOTHING);
        let failed = go_err(&boards, &rules, &runs);
        assert_eq!(failed, "The engine is not answering.");
        assert_eq!(lane_of(&boards, 412, "d").0, "sliced");
        assert_eq!(boards.work("sample", 412, "d").unwrap(), None);
    }

    fn go_err(boards: &Boards, rules: &Rules, runs: &Runs) -> String {
        tauri::async_runtime::block_on(run(
            boards,
            rules,
            runs,
            "sample",
            412,
            "d",
            MODEL,
            |_, _, _| async { Err("The engine is not answering.".to_string()) },
        ))
        .unwrap_err()
    }

    #[test]
    fn a_model_outside_this_computer_is_never_a_runner() {
        let (boards, rules) = sample("outside");
        let runs = Runs::default();
        for model in ["xai:grok-4.7", "openai:gpt-5", "anthropic:claude-opus-5-5"] {
            let mut asked = false;
            let refused = tauri::async_runtime::block_on(run(
                &boards,
                &rules,
                &runs,
                "sample",
                412,
                "d",
                model,
                |_, _, _| {
                    asked = true;
                    async { Ok(answered("x", 1)) }
                },
            ));
            assert_eq!(refused.unwrap_err(), OUTSIDE);
            assert!(!asked, "{model} was asked");
        }
        assert_eq!(runnable("phi-3.5-vision").unwrap_err(), WORDS_ONLY);
        assert!(runnable("not-a-model").is_err());
        assert!(runnable("rig:m1").is_ok());
        assert_eq!(lane_of(&boards, 412, "d").0, "sliced");
    }

    #[test]
    fn a_slice_not_the_models_to_write_is_refused_before_it_is_asked() {
        let (boards, rules) = sample("precheck");
        let runs = Runs::default();
        // 412 c is in written already, b in checked, a in main.
        for slice in ["c", "b", "a"] {
            let mut asked = false;
            let ran = tauri::async_runtime::block_on(run(
                &boards,
                &rules,
                &runs,
                "sample",
                412,
                slice,
                MODEL,
                |_, _, _| {
                    asked = true;
                    async { Ok(answered("x", 1)) }
                },
            ))
            .unwrap();
            assert!(!ran.moved);
            assert!(!ran.verdict.unwrap().allowed);
            assert!(!asked, "a model was asked to write {slice}");
        }
    }

    #[test]
    fn one_runner_at_a_time_on_a_slice_and_it_can_be_stopped() {
        let (boards, rules) = sample("twice");
        let runs = Runs::default();
        let ran = tauri::async_runtime::block_on(run(
            &boards,
            &rules,
            &runs,
            "sample",
            412,
            "d",
            MODEL,
            |_, _, mut stopped| {
                let runs = &runs;
                let boards = &boards;
                let rules = &rules;
                async move {
                    assert_eq!(runs.going(), ["sample/412/d"]);
                    let again = run(
                        boards,
                        rules,
                        runs,
                        "sample",
                        412,
                        "d",
                        MODEL,
                        |_, _, _| async { Ok(answered("x", 1)) },
                    )
                    .await;
                    assert_eq!(again.unwrap_err(), ALREADY);
                    assert!(runs.stop("sample", 412, "d"));
                    stopped.changed().await.unwrap();
                    assert!(*stopped.borrow());
                    Ok(Timed::stopped())
                }
            },
        ))
        .unwrap();
        assert_eq!(ran.words, STOPPED);
        assert!(runs.going().is_empty());
        assert!(
            !runs.stop("sample", 412, "d"),
            "a run that ended could still be stopped"
        );
    }

    #[test]
    fn the_work_is_kept_off_the_board_and_cut_at_its_limit() {
        let (boards, rules) = sample("kept");
        let runs = Runs::default();
        let long = "é".repeat(crate::board::WORK_LIMIT);
        go(&boards, &rules, &runs, "d", MODEL, answered(&long, 10)).unwrap();
        let kept = boards.work("sample", 412, "d").unwrap().unwrap();
        assert!(kept.cut, "work past the limit does not say it was cut");
        assert!(kept.text.len() <= crate::board::WORK_LIMIT);
        let board = fs::read_to_string(std::env::temp_dir().join(format!(
            "rusty-runners-kept-{}/boards/sample.json",
            std::process::id()
        )))
        .unwrap();
        assert!(!board.contains("éé"), "the work went into the board");
        assert!(boards.work("sample", 412, "zz").unwrap().is_none());
        assert!(boards.work("../x", 412, "d").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn the_work_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let (boards, rules) = sample("owner");
        go(
            &boards,
            &rules,
            &Runs::default(),
            "d",
            MODEL,
            answered("Mine.", 10),
        )
        .unwrap();
        let place = std::env::temp_dir().join(format!(
            "rusty-runners-owner-{}/boards/work",
            std::process::id()
        ));
        let mode = |path: PathBuf| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(place.clone()), 0o700);
        assert_eq!(mode(place.join("sample.json")), 0o600);
    }
}
