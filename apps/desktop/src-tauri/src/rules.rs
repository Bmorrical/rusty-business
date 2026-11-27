//! The orchestrator's rules: checks coded into the app, each behind a toggle,
//! that the Workflow applies to every move of a slice.
//!
//! It answers Brad's ask, 2026-11-27: "it's a rule of the rusty business™ code,
//! and it's enforced by toggle, and it's used in workflows". The rules are
//! not a file anyone edits: they live here, with tests. What the person
//! keeps is only which rules are off and the runner's time limit, in the
//! app's own folder, readable by this account alone.
//!
//! Every rule starts on, since a rule only ever stops a move and reaches
//! nothing. One rule has no toggle: only you take a slice into main. The
//! orchestrator never merges.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::board::{LANES, MOVES};

/// The rule that keeps lanes in order.
pub const IN_ORDER: &str = "in-order";
/// The rule that each lane is worked by its own party, or by you.
pub const OWN_PARTY: &str = "own-party";
/// The rule that only you take a slice into main. It has no toggle.
pub const MAIN_IS_YOURS: &str = "main-is-yours";
/// The rule that a runner past its time limit has drifted.
pub const ON_TIME: &str = "on-time";

/// The runner's time limit to start with, in minutes.
pub const LIMIT_START: u32 = 30;
/// The shortest and the longest time limit, in minutes.
pub const LIMIT_LEAST: u32 = 5;
pub const LIMIT_MOST: u32 = 480;

/// One of the app's rules, as the Rules tab shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: &'static str,
    pub name: &'static str,
    pub words: &'static str,
    /// True for the rule that has no toggle.
    pub locked: bool,
    pub on: bool,
}

/// The rules, in the order the tab shows them.
const RULES: [(&str, &str, &str, bool); 4] = [
    (
        IN_ORDER,
        "Lanes in order",
        "A slice goes forward one lane at a time: sliced, written, checked, reviewed, then main. It may go back to an earlier lane to be done again. A slice in main stays there.",
        false,
    ),
    (
        OWN_PARTY,
        "Each lane by its own party",
        "You slice, a local model writes, the check checks, and a reviewer reviews. You may take any lane's move yourself. After each move, the next move goes to the next lane's party.",
        false,
    ),
    (
        MAIN_IS_YOURS,
        "Only you take a slice into main",
        "No model, check, or reviewer moves a slice into main, and the orchestrator never merges. This rule has no toggle.",
        true,
    ),
    (
        ON_TIME,
        "A runner keeps to its time",
        "A local model that takes longer than the time limit to write a slice has drifted. Its work is set aside, and the slice goes back to you to be cut smaller.",
        false,
    ),
];

/// What the person keeps: the rules turned off, and the time limit.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Kept {
    #[serde(default)]
    off: BTreeSet<String>,
    #[serde(default = "limit_start")]
    limit_minutes: u32,
}

fn limit_start() -> u32 {
    LIMIT_START
}

impl Default for Kept {
    fn default() -> Self {
        Self {
            off: BTreeSet::new(),
            limit_minutes: LIMIT_START,
        }
    }
}

impl Kept {
    /// What was read back, with anything that is not a toggle's rule, and a
    /// limit out of range, dropped. Nothing read can turn the locked rule off.
    fn cleaned(mut self) -> Self {
        self.off.retain(|id| {
            RULES
                .iter()
                .any(|(rule, _, _, locked)| rule == id && !locked)
        });
        if !(LIMIT_LEAST..=LIMIT_MOST).contains(&self.limit_minutes) {
            self.limit_minutes = LIMIT_START;
        }
        self
    }

    fn on(&self, id: &str) -> bool {
        !self.off.contains(id)
    }
}

/// The Rules tab: each rule, whether it is on, and the time limit.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesView {
    pub rules: Vec<Rule>,
    pub limit_minutes: u32,
}

/// A move of a slice, as the Workflow proposes it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Move {
    /// The last lane the slice has been through; empty for none yet.
    #[serde(default)]
    pub from: String,
    /// The lane it would go into.
    pub to: String,
    /// Who makes the move: you, model, check, or review.
    pub by: String,
    /// How long the runner held the slice, when a model makes the move.
    #[serde(default)]
    pub took_ms: Option<u64>,
}

/// A rule that stops a move, and why.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    pub rule: &'static str,
    pub words: String,
}

/// What the rules say of a move.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub allowed: bool,
    pub refusals: Vec<Refusal>,
    /// Whose move it is once this one is made: the next lane's party, or
    /// empty once the slice is in main. Unchanged words when refused.
    pub turn: String,
    /// True when the runner drifted: the slice goes back to be cut smaller.
    pub split: bool,
}

/// The party that works a lane: you slice and take into main, a model
/// writes, the check checks, and a reviewer reviews.
pub fn party(lane: &str) -> Option<&'static str> {
    match lane {
        "sliced" | "main" => Some("you"),
        "written" => Some("model"),
        "checked" => Some("check"),
        "reviewed" => Some("review"),
        _ => None,
    }
}

/// The place of a lane, or `None` for none yet.
fn place(lane: &str) -> Option<usize> {
    LANES.iter().position(|known| *known == lane)
}

/// Checks a move against the rules that are on. A move into a lane the app
/// does not know, or by a party it does not know, is refused whatever is on.
fn judge(kept: &Kept, step: &Move) -> Result<Verdict, String> {
    let from = match step.from.as_str() {
        "" => None,
        lane => Some(place(lane).ok_or("That slice is in a lane the app does not know.")?),
    };
    let to = place(&step.to).ok_or("That is not one of the five lanes.")?;
    if !MOVES.contains(&step.by.as_str()) {
        return Err("A move is made by you, a model, the check, or a reviewer.".to_string());
    }
    let mut refusals = Vec::new();
    let mut split = false;

    if kept.on(IN_ORDER) {
        let next = from.map_or(0, |from| from + 1);
        if from == Some(LANES.len() - 1) {
            refusals.push(Refusal {
                rule: IN_ORDER,
                words: "That slice is in main, and stays there.".to_string(),
            });
        } else if to > next {
            refusals.push(Refusal {
                rule: IN_ORDER,
                words: format!(
                    "That skips a lane: the slice goes into {} first.",
                    LANES[next]
                ),
            });
        }
    }

    let back = from.is_some_and(|from| to <= from);
    if kept.on(OWN_PARTY) && step.by != "you" {
        // Going back is a redo: whoever holds the slice now may send it back.
        let holder = from
            .and_then(|from| LANES.get(from + 1))
            .and_then(|lane| party(lane));
        let own = if back { holder } else { party(LANES[to]) };
        if own != Some(step.by.as_str()) {
            refusals.push(Refusal {
                rule: OWN_PARTY,
                words: format!(
                    "That move is not the {}'s to make.",
                    match step.by.as_str() {
                        "model" => "local model",
                        "check" => "check",
                        _ => "reviewer",
                    }
                ),
            });
        }
    }

    // Locked on: no toggle reaches it.
    if LANES[to] == "main" && step.by != "you" {
        refusals.push(Refusal {
            rule: MAIN_IS_YOURS,
            words: "Only you take a slice into main.".to_string(),
        });
    }

    if kept.on(ON_TIME) && LANES[to] == "written" && step.by == "model" && !back {
        let limit = u64::from(kept.limit_minutes) * 60_000;
        if step.took_ms.is_some_and(|took| took > limit) {
            split = true;
            refusals.push(Refusal {
                rule: ON_TIME,
                words: format!(
                    "The model took longer than {} minutes, so it drifted. Its work is set aside, and the slice goes back to you to be cut smaller.",
                    kept.limit_minutes
                ),
            });
        }
    }

    let allowed = refusals.is_empty();
    let turn = if split {
        "you".to_string()
    } else if allowed {
        LANES
            .get(to + 1)
            .and_then(|lane| party(lane))
            .unwrap_or_default()
            .to_string()
    } else {
        String::new()
    };
    Ok(Verdict {
        allowed,
        refusals,
        turn,
        split,
    })
}

/// The rules, kept in the app's own folder.
pub struct Rules {
    file: PathBuf,
    kept: Mutex<Kept>,
}

impl Rules {
    /// The rules as last left. A file that cannot be read is every rule on,
    /// with the time limit to start with.
    pub fn new(file: PathBuf) -> Self {
        let kept = fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str::<Kept>(&text).ok())
            .map(Kept::cleaned)
            .unwrap_or_default();
        Self {
            file,
            kept: Mutex::new(kept),
        }
    }

    fn kept(&self) -> Kept {
        self.kept
            .lock()
            .map(|kept| kept.clone())
            .unwrap_or_default()
    }

    pub fn view(&self) -> RulesView {
        let kept = self.kept();
        RulesView {
            rules: RULES
                .iter()
                .map(|(id, name, words, locked)| Rule {
                    id,
                    name,
                    words,
                    locked: *locked,
                    on: *locked || kept.on(id),
                })
                .collect(),
            limit_minutes: kept.limit_minutes,
        }
    }

    /// Changes what is kept, keeps it, and uses it from the next move on. A
    /// change that cannot be kept changes nothing.
    fn change(&self, change: impl FnOnce(&mut Kept)) -> Result<RulesView, String> {
        let mut kept = self
            .kept
            .lock()
            .map_err(|_| "The app could not take the rules.".to_string())?;
        let mut next = kept.clone();
        change(&mut next);
        keep(&self.file, &next)?;
        *kept = next;
        drop(kept);
        Ok(self.view())
    }

    /// Turns one rule on or off. The locked rule cannot be turned off.
    pub fn set(&self, id: &str, on: bool) -> Result<RulesView, String> {
        let (_, _, _, locked) = RULES
            .iter()
            .find(|(rule, ..)| *rule == id)
            .ok_or("That is not one of the app's rules.")?;
        if *locked {
            return Err("Only you take a slice into main. That rule has no toggle.".to_string());
        }
        self.change(|kept| {
            if on {
                kept.off.remove(id);
            } else {
                kept.off.insert(id.to_string());
            }
        })
    }

    /// Sets the runner's time limit, in minutes.
    pub fn set_limit(&self, minutes: u32) -> Result<RulesView, String> {
        if !(LIMIT_LEAST..=LIMIT_MOST).contains(&minutes) {
            return Err(format!(
                "The time limit is from {LIMIT_LEAST} to {LIMIT_MOST} minutes."
            ));
        }
        self.change(|kept| kept.limit_minutes = minutes)
    }

    /// What the rules that are on say of a move.
    pub fn check(&self, step: &Move) -> Result<Verdict, String> {
        judge(&self.kept(), step)
    }
}

/// Writes what is kept readable by this account alone, whole or not at all.
fn keep(file: &Path, kept: &Kept) -> Result<(), String> {
    let unkept = |error: std::io::Error| format!("The app could not keep the rules: {error}.");
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder).map_err(unkept)?;
    }
    let text = serde_json::to_string_pretty(kept).map_err(|error| error.to_string())?;
    let part = file.with_extension("part");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(&part).map_err(unkept)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&part, fs::Permissions::from_mode(0o600)).map_err(unkept)?;
    }
    out.write_all(text.as_bytes())
        .and_then(|()| out.sync_all())
        .map_err(unkept)?;
    drop(out);
    fs::rename(&part, file).map_err(unkept)
}

#[tauri::command]
pub fn rules_status(rules: tauri::State<'_, Rules>) -> RulesView {
    rules.view()
}

#[tauri::command]
pub fn set_rule(rules: tauri::State<'_, Rules>, id: String, on: bool) -> Result<RulesView, String> {
    rules.set(&id, on)
}

#[tauri::command]
pub fn set_rule_limit(rules: tauri::State<'_, Rules>, minutes: u32) -> Result<RulesView, String> {
    rules.set_limit(minutes)
}

#[tauri::command]
pub fn check_move(rules: tauri::State<'_, Rules>, step: Move) -> Result<Verdict, String> {
    rules.check(&step)
}

#[cfg(test)]
mod tests {
    // Brad, 2026-11-27: "it's a rule of the rusty business™ code, and it's
    // enforced by toggle, and it's used in workflows". On the labels: "same".
    // On the drift threshold: "Time".

    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("rusty-rules-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn step(from: &str, to: &str, by: &str) -> Move {
        Move {
            from: from.to_string(),
            to: to.to_string(),
            by: by.to_string(),
            took_ms: None,
        }
    }

    fn rules_of(verdict: &Verdict) -> Vec<&'static str> {
        verdict
            .refusals
            .iter()
            .map(|refusal| refusal.rule)
            .collect()
    }

    fn all_on() -> Kept {
        Kept::default()
    }

    fn with_off(ids: &[&str]) -> Kept {
        Kept {
            off: ids.iter().map(|id| id.to_string()).collect(),
            ..Kept::default()
        }
    }

    #[test]
    fn every_rule_starts_on() {
        let place = folder("start");
        let view = Rules::new(place.join("rules.json")).view();
        assert_eq!(
            view.rules.iter().map(|rule| rule.id).collect::<Vec<_>>(),
            [IN_ORDER, OWN_PARTY, MAIN_IS_YOURS, ON_TIME]
        );
        assert!(view.rules.iter().all(|rule| rule.on), "a rule starts off");
        assert_eq!(view.limit_minutes, LIMIT_START);
        let locked: Vec<_> = view
            .rules
            .iter()
            .filter(|rule| rule.locked)
            .map(|rule| rule.id)
            .collect();
        assert_eq!(locked, [MAIN_IS_YOURS], "only main has no toggle");
    }

    #[test]
    fn a_slice_goes_through_the_lanes_in_order() {
        let kept = all_on();
        let moves = [
            ("", "sliced", "you", "model"),
            ("sliced", "written", "model", "check"),
            ("written", "checked", "check", "review"),
            ("checked", "reviewed", "review", "you"),
            ("reviewed", "main", "you", ""),
        ];
        for (from, to, by, turn) in moves {
            let verdict = judge(&kept, &step(from, to, by)).unwrap();
            assert!(
                verdict.allowed,
                "{from} to {to} by {by}: {:?}",
                verdict.refusals
            );
            assert_eq!(verdict.turn, turn, "the next move after {to}");
        }
        let skip = judge(&kept, &step("sliced", "checked", "check")).unwrap();
        assert_eq!(rules_of(&skip), [IN_ORDER], "a lane was skipped");
        assert!(skip.refusals[0].words.contains("written"));
        let from_main = judge(&kept, &step("main", "sliced", "you")).unwrap();
        assert_eq!(rules_of(&from_main), [IN_ORDER], "a slice left main");
        // Back to an earlier lane is a redo.
        assert!(
            judge(&kept, &step("checked", "sliced", "you"))
                .unwrap()
                .allowed
        );
        // Off, a lane may be skipped.
        assert!(
            judge(&with_off(&[IN_ORDER]), &step("sliced", "checked", "check"))
                .unwrap()
                .allowed
        );
    }

    #[test]
    fn each_lane_is_worked_by_its_own_party_or_by_you() {
        let kept = all_on();
        let wrong = judge(&kept, &step("sliced", "written", "review")).unwrap();
        assert_eq!(rules_of(&wrong), [OWN_PARTY], "a reviewer wrote a slice");
        assert!(
            judge(&kept, &step("sliced", "written", "you"))
                .unwrap()
                .allowed,
            "you may write it yourself"
        );
        // The check that holds a written slice may send it back to be written again.
        assert!(
            judge(&kept, &step("written", "sliced", "check"))
                .unwrap()
                .allowed
        );
        assert_eq!(
            rules_of(&judge(&kept, &step("written", "sliced", "review")).unwrap()),
            [OWN_PARTY]
        );
        assert!(
            judge(
                &with_off(&[OWN_PARTY]),
                &step("sliced", "written", "review")
            )
            .unwrap()
            .allowed
        );
    }

    #[test]
    fn only_you_take_a_slice_into_main_whatever_is_off() {
        let everything_off = with_off(&[IN_ORDER, OWN_PARTY, ON_TIME]);
        for by in ["model", "check", "review"] {
            let verdict = judge(&everything_off, &step("reviewed", "main", by)).unwrap();
            assert_eq!(
                rules_of(&verdict),
                [MAIN_IS_YOURS],
                "{by} took a slice into main"
            );
            assert!(!verdict.allowed);
        }
        let place = folder("locked");
        let rules = Rules::new(place.join("rules.json"));
        assert!(
            rules.set(MAIN_IS_YOURS, false).is_err(),
            "the main rule was turned off"
        );
        // Nor can a kept file turn it off.
        fs::write(
            place.join("rules.json"),
            r#"{"off":["main-is-yours","nonsense"],"limitMinutes":30}"#,
        )
        .unwrap();
        let rules = Rules::new(place.join("rules.json"));
        assert!(
            rules.kept.lock().unwrap().off.is_empty(),
            "the kept file's main and unknown ids were read back"
        );
        assert!(rules.view().rules.iter().all(|rule| rule.on));
        assert!(
            !rules
                .check(&step("reviewed", "main", "model"))
                .unwrap()
                .allowed
        );
    }

    #[test]
    fn a_runner_past_its_time_has_drifted_and_the_slice_is_cut_smaller() {
        let kept = all_on();
        let mut late = step("sliced", "written", "model");
        late.took_ms = Some(u64::from(LIMIT_START) * 60_000 + 1);
        let verdict = judge(&kept, &late).unwrap();
        assert_eq!(rules_of(&verdict), [ON_TIME]);
        assert!(
            verdict.split,
            "a late runner's slice was not sent to be cut"
        );
        assert_eq!(verdict.turn, "you", "a drifted slice goes back to you");
        late.took_ms = Some(u64::from(LIMIT_START) * 60_000);
        assert!(
            judge(&kept, &late).unwrap().allowed,
            "on the limit is on time"
        );
        late.took_ms = Some(u64::from(LIMIT_MOST) * 60_000);
        assert!(judge(&with_off(&[ON_TIME]), &late).unwrap().allowed);
    }

    #[test]
    fn a_move_the_app_does_not_know_is_refused() {
        let kept = with_off(&[IN_ORDER, OWN_PARTY, ON_TIME]);
        assert!(judge(&kept, &step("sliced", "shipped", "you")).is_err());
        assert!(judge(&kept, &step("drafted", "written", "you")).is_err());
        assert!(judge(&kept, &step("sliced", "written", "anyone")).is_err());
    }

    #[test]
    fn the_toggles_are_kept_for_this_account_alone_and_read_back() {
        let place = folder("kept");
        let rules = Rules::new(place.join("rules.json"));
        rules.set(OWN_PARTY, false).unwrap();
        let view = rules.set_limit(45).unwrap();
        assert!(
            !view
                .rules
                .iter()
                .find(|rule| rule.id == OWN_PARTY)
                .unwrap()
                .on
        );
        assert_eq!(
            Rules::new(place.join("rules.json")).view(),
            view,
            "the toggles were not kept"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(place.join("rules.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "other accounts can read the rules");
        }
        assert!(rules.set_limit(LIMIT_LEAST - 1).is_err());
        assert!(rules.set_limit(LIMIT_MOST + 1).is_err());
        assert!(rules.set("nonsense", false).is_err());
        assert_eq!(rules.view(), view, "a refused change changed something");
        // A file that cannot be read is every rule on.
        fs::write(place.join("rules.json"), "not json").unwrap();
        assert!(
            Rules::new(place.join("rules.json"))
                .view()
                .rules
                .iter()
                .all(|rule| rule.on)
        );
        fs::write(
            place.join("rules.json"),
            r#"{"off":[],"limitMinutes":9999}"#,
        )
        .unwrap();
        assert_eq!(
            Rules::new(place.join("rules.json")).view().limit_minutes,
            LIMIT_START
        );
    }

    #[test]
    fn the_rules_know_the_boards_lanes_and_moves() {
        for lane in LANES {
            assert!(
                party(lane).is_some_and(|party| MOVES.contains(&party)),
                "{lane} has no party"
            );
        }
    }
}
