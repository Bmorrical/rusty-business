//! Saved sessions: one JSON file per chat, in the app's own data folder.
//!
//! Nothing here deletes. "Trash" moves a session's file into a `trash`
//! folder beside the others, where it stays until a person removes it.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const TITLE_LIMIT: usize = 120;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
    /// True when the person pressed Stop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped: Option<bool>,
    /// True when the reply ran into the length limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_short: Option<bool>,
    /// A step the model took in the project's folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<serde_json::Value>,
    /// True on what a step found, which goes back to the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<bool>,
    /// Files attached to a message, with their text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments: Option<serde_json::Value>,
    /// The person's rating of an answer: "up" or "down".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<String>,
    /// True on the answer the person voted best of those to its question.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best: Option<bool>,
    /// When the person last voted on this answer, in milliseconds since 1970.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voted_at: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub model: String,
    /// Milliseconds since 1970.
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub messages: Vec<Turn>,
    /// The project the chat belongs to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub model: String,
    pub updated_at: u64,
    pub turns: usize,
    pub project: Option<String>,
    pub pinned: bool,
    /// The models that answered the latest question, in the order they
    /// answered, each once.
    pub models: Vec<String>,
}

/// The models that answered the latest question: the replies after the last
/// question, in order, each model once.
pub fn latest_answers(messages: &[Turn]) -> Vec<String> {
    let start = messages
        .iter()
        .rposition(|turn| turn.role == "user")
        .map(|at| at + 1)
        .unwrap_or(0);
    let mut models: Vec<String> = Vec::new();
    for turn in &messages[start..] {
        if let Some(model) = turn
            .model
            .as_ref()
            .filter(|model| turn.role == "assistant" && !model.is_empty())
            && !models.contains(model)
        {
            models.push(model.clone());
        }
    }
    models
}

/// One model's answer to a question someone voted on.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VotedAnswer {
    pub model: String,
    /// "up", "down", or none.
    pub rating: Option<String>,
    pub best: bool,
}

/// A question with at least one vote on its answers, and the chat it is in.
/// Brad, 2026-11-20: "it keeps track of which answers were asked when it got
/// voted the best one."
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VotedRound {
    pub session: String,
    pub title: String,
    /// The question, cut to `QUESTION_LIMIT` characters.
    pub question: String,
    /// The latest vote on its answers, in milliseconds since 1970.
    pub voted_at: u64,
    /// Every model that answered, each once, in the order they answered.
    pub answers: Vec<VotedAnswer>,
}

const QUESTION_LIMIT: usize = 160;

/// The questions in a chat that have a vote on an answer. A rating counts on
/// any answer. A best counts only where two or more models answered, and only
/// the first answer marked best, in case a file was edited by hand.
pub fn voted_rounds(session: &Session) -> Vec<VotedRound> {
    let mut rounds: Vec<(String, Vec<&Turn>)> = Vec::new();
    for turn in &session.messages {
        if turn.role == "user" || rounds.is_empty() {
            // A step's result goes back to the model as a message; the
            // question stays the one the person asked.
            let question = if turn.role == "user" && turn.tool_result != Some(true) {
                turn.content.clone()
            } else {
                rounds
                    .last()
                    .map(|round| round.0.clone())
                    .unwrap_or_default()
            };
            rounds.push((question, Vec::new()));
        }
        let named = turn.model.as_ref().is_some_and(|model| !model.is_empty());
        if turn.role == "assistant" && named {
            rounds.last_mut().expect("a round was pushed").1.push(turn);
        }
    }
    let mut voted = Vec::new();
    for (question, turns) in rounds {
        let mut answers: Vec<VotedAnswer> = Vec::new();
        let mut voted_at = 0;
        let mut any = false;
        let mut best_taken = false;
        let several = turns.len() > 1;
        for turn in turns {
            let model = turn.model.clone().unwrap_or_default();
            let rating = turn
                .rating
                .clone()
                .filter(|rating| rating == "up" || rating == "down");
            let best = several && !best_taken && turn.best == Some(true);
            best_taken |= best;
            if rating.is_some() || best {
                any = true;
                voted_at = voted_at.max(turn.voted_at.unwrap_or(0));
            }
            match answers.iter_mut().find(|answer| answer.model == model) {
                Some(answer) => {
                    answer.rating = answer.rating.take().or(rating);
                    answer.best |= best;
                }
                None => answers.push(VotedAnswer {
                    model,
                    rating,
                    best,
                }),
            }
        }
        if any {
            voted.push(VotedRound {
                session: session.id.clone(),
                title: session.title.clone(),
                question: question.trim().chars().take(QUESTION_LIMIT).collect(),
                voted_at,
                answers,
            });
        }
    }
    voted
}

/// A chat that matched a search, with the words around the first match.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    #[serde(flatten)]
    pub session: SessionSummary,
    pub snippet: String,
}

const SNIPPET_AROUND: usize = 40;
const FOUND_LIMIT: usize = 50;

fn summary(session: Session) -> SessionSummary {
    SessionSummary {
        turns: session.messages.len(),
        id: session.id,
        title: session.title,
        model: session.model,
        updated_at: session.updated_at,
        project: session.project,
        pinned: session.pinned,
        models: latest_answers(&session.messages),
    }
}

/// The words around the first place `needle` (lower case) appears in `text`,
/// on one line, or None when it does not.
fn snippet(text: &str, needle: &str) -> Option<String> {
    let characters: Vec<char> = text.chars().collect();
    let lower: Vec<char> = characters
        .iter()
        .map(|character| character.to_lowercase().next().unwrap_or(*character))
        .collect();
    let wanted: Vec<char> = needle.chars().collect();
    let at = lower
        .windows(wanted.len())
        .position(|window| window == wanted.as_slice())?;
    let start = at.saturating_sub(SNIPPET_AROUND);
    let end = (at + wanted.len() + SNIPPET_AROUND).min(characters.len());
    let middle: String = characters[start..end].iter().collect();
    let middle = middle.split_whitespace().collect::<Vec<_>>().join(" ");
    let before = if start > 0 { "..." } else { "" };
    let after = if end < characters.len() { "..." } else { "" };
    Some(format!("{before}{middle}{after}"))
}

pub struct Store {
    dir: PathBuf,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

/// An id is letters, digits, and hyphens only, so it can never name a file
/// outside the sessions folder.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn failed(what: &str) -> impl Fn(std::io::Error) -> String + '_ {
    move |error| format!("{what}: {error}")
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Store { dir }
    }

    fn ready(&self) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(failed("Could not open the sessions folder"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700));
        }
        Ok(())
    }

    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("That is not a session.".to_string());
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    /// Every saved session, newest first. A file that cannot be read is
    /// skipped, never removed.
    pub fn list(&self) -> Result<Vec<SessionSummary>, String> {
        let mut summaries = Vec::new();
        self.each(|session| summaries.push(summary(session)))?;
        summaries.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
        Ok(summaries)
    }

    /// Hands each saved session to `visit`, one file at a time, so only what
    /// `visit` keeps of them is held at once.
    fn each(&self, mut visit: impl FnMut(Session)) -> Result<(), String> {
        self.ready()?;
        for entry in fs::read_dir(&self.dir)
            .map_err(failed("Could not read the sessions folder"))?
            .flatten()
        {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(session) = serde_json::from_str::<Session>(&text) else {
                continue;
            };
            if !valid_id(&session.id) {
                continue;
            }
            visit(session);
        }
        Ok(())
    }

    /// Every question with a vote on its answers, from every saved chat, the
    /// latest vote first. A chat in the trash is left out.
    pub fn votes(&self) -> Result<Vec<VotedRound>, String> {
        let mut rounds = Vec::new();
        self.each(|session| rounds.extend(voted_rounds(&session)))?;
        rounds.sort_by_key(|round| std::cmp::Reverse(round.voted_at));
        Ok(rounds)
    }

    /// Chats whose title or words hold `query`, ignoring case, newest first.
    pub fn search(&self, query: &str) -> Result<Vec<Found>, String> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        self.each(|session| {
            let hit = snippet(&session.title, &needle)
                .map(|_| String::new())
                .or_else(|| {
                    session
                        .messages
                        .iter()
                        .find_map(|turn| snippet(&turn.content, &needle))
                });
            if let Some(snippet) = hit {
                found.push(Found {
                    session: summary(session),
                    snippet,
                });
            }
        })?;
        found.sort_by_key(|a| std::cmp::Reverse(a.session.updated_at));
        found.truncate(FOUND_LIMIT);
        Ok(found)
    }

    /// Pins a chat, or moves it into a project or out of one, without
    /// changing when it was last written to.
    pub fn arrange(
        &self,
        id: &str,
        pinned: bool,
        project: Option<String>,
    ) -> Result<SessionSummary, String> {
        let mut session = self.load(id)?;
        session.pinned = pinned;
        session.project = project.filter(|project| !project.is_empty());
        let text = serde_json::to_string_pretty(&session)
            .map_err(|_| "Could not write the session.".to_string())?;
        let path = self.path(id)?;
        let partial = path.with_extension("json.partial");
        fs::write(&partial, text).map_err(failed("Could not save the session"))?;
        fs::rename(&partial, &path).map_err(failed("Could not save the session"))?;
        Ok(summary(session))
    }

    pub fn load(&self, id: &str) -> Result<Session, String> {
        let text =
            fs::read_to_string(self.path(id)?).map_err(failed("Could not open that session"))?;
        serde_json::from_str(&text)
            .map_err(|_| "That session's file could not be read.".to_string())
    }

    /// Saves a session, giving a new one its id. The file is written beside
    /// its final place and then moved over it, so a save cut short never
    /// leaves half a session.
    pub fn save(&self, mut session: Session) -> Result<Session, String> {
        self.ready()?;
        if session.id.is_empty() {
            session.id = uuid::Uuid::new_v4().to_string();
        }
        let path = self.path(&session.id)?;
        let now = now_ms();
        if session.created_at == 0 {
            session.created_at = now;
        }
        session.updated_at = now;
        let title: String = session.title.trim().chars().take(TITLE_LIMIT).collect();
        session.title = if title.is_empty() {
            "New chat".to_string()
        } else {
            title
        };
        let text = serde_json::to_string_pretty(&session)
            .map_err(|_| "Could not write the session.".to_string())?;
        let partial = path.with_extension("json.partial");
        fs::write(&partial, text).map_err(failed("Could not save the session"))?;
        fs::rename(&partial, &path).map_err(failed("Could not save the session"))?;
        Ok(session)
    }

    /// Moves a session's file into the trash folder. Nothing is deleted.
    pub fn trash(&self, id: &str) -> Result<(), String> {
        let path = self.path(id)?;
        let trash = self.dir.join("trash");
        fs::create_dir_all(&trash).map_err(failed("Could not open the trash folder"))?;
        fs::rename(&path, trash.join(format!("{id}-{}.json", now_ms())))
            .map_err(failed("Could not move that session to the trash"))
    }
}

#[tauri::command]
pub fn list_sessions(store: tauri::State<'_, Store>) -> Result<Vec<SessionSummary>, String> {
    store.list()
}

#[tauri::command]
pub fn list_votes(store: tauri::State<'_, Store>) -> Result<Vec<VotedRound>, String> {
    store.votes()
}

#[tauri::command]
pub fn load_session(store: tauri::State<'_, Store>, id: String) -> Result<Session, String> {
    store.load(&id)
}

#[tauri::command]
pub fn save_session(store: tauri::State<'_, Store>, session: Session) -> Result<Session, String> {
    store.save(session)
}

#[tauri::command]
pub fn search_sessions(
    store: tauri::State<'_, Store>,
    query: String,
) -> Result<Vec<Found>, String> {
    store.search(&query)
}

#[tauri::command]
pub fn arrange_session(
    store: tauri::State<'_, Store>,
    id: String,
    pinned: bool,
    project: Option<String>,
) -> Result<SessionSummary, String> {
    store.arrange(&id, pinned, project)
}

#[tauri::command]
pub fn trash_session(store: tauri::State<'_, Store>, id: String) -> Result<(), String> {
    store.trash(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_store() -> Store {
        Store::new(
            std::env::temp_dir().join(format!("rusty-business-test-{}", uuid::Uuid::new_v4())),
        )
    }

    fn chat(title: &str) -> Session {
        Session {
            title: title.to_string(),
            model: "mlx-community/phi-4-4bit".to_string(),
            messages: vec![
                Turn {
                    role: "user".to_string(),
                    content: "What is 2+2?".to_string(),
                    ..Turn::default()
                },
                Turn {
                    role: "assistant".to_string(),
                    content: "4.".to_string(),
                    thinking: Some("Add them.".to_string()),
                    completion_tokens: Some(3),
                    ..Turn::default()
                },
            ],
            ..Session::default()
        }
    }

    #[test]
    fn a_saved_session_comes_back_whole() {
        let store = scratch_store();
        let saved = store.save(chat("Sums")).unwrap();
        assert!(valid_id(&saved.id));
        assert!(saved.created_at > 0 && saved.updated_at >= saved.created_at);
        assert_eq!(store.load(&saved.id).unwrap(), saved);
    }

    #[test]
    fn saving_again_keeps_the_id_and_the_start_time() {
        let store = scratch_store();
        let first = store.save(chat("Sums")).unwrap();
        let mut edited = first.clone();
        edited.messages.push(Turn {
            role: "user".to_string(),
            content: "And 3+3?".to_string(),
            ..Turn::default()
        });
        let second = store.save(edited).unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.created_at, first.created_at);
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.list().unwrap()[0].turns, 3);
    }

    #[test]
    fn the_list_is_newest_first_and_skips_a_file_it_cannot_read() {
        let store = scratch_store();
        let mut older = store.save(chat("Older")).unwrap();
        let newer = store.save(chat("Newer")).unwrap();
        // Force the order whatever the clock's resolution.
        older.updated_at = 1;
        fs::write(
            store.path(&older.id).unwrap(),
            serde_json::to_string(&older).unwrap(),
        )
        .unwrap();
        fs::write(store.dir.join("broken.json"), "not json").unwrap();
        let titles: Vec<String> = store
            .list()
            .unwrap()
            .into_iter()
            .map(|summary| summary.title)
            .collect();
        assert_eq!(titles, ["Newer", "Older"]);
        assert!(store.dir.join("broken.json").exists());
        assert_eq!(store.load(&newer.id).unwrap().title, "Newer");
    }

    #[test]
    fn an_id_cannot_reach_outside_the_folder() {
        let store = scratch_store();
        for id in [
            "../secrets",
            "..",
            "a/b",
            "a\\b",
            "",
            "name.json",
            &"x".repeat(65),
        ] {
            assert!(store.load(id).is_err(), "{id:?} should be refused");
            assert!(store.trash(id).is_err(), "{id:?} should be refused");
            let mut session = chat("Sneaky");
            session.id = id.to_string();
            if !id.is_empty() {
                assert!(store.save(session).is_err(), "{id:?} should be refused");
            }
        }
    }

    #[test]
    fn trash_moves_the_file_and_deletes_nothing() {
        let store = scratch_store();
        let saved = store.save(chat("Sums")).unwrap();
        store.trash(&saved.id).unwrap();
        assert!(store.list().unwrap().is_empty());
        let kept: Vec<_> = fs::read_dir(store.dir.join("trash"))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(kept.len(), 1);
        let text = fs::read_to_string(kept[0].path()).unwrap();
        assert_eq!(serde_json::from_str::<Session>(&text).unwrap(), saved);
    }

    fn answer(model: &str, content: &str) -> Turn {
        Turn {
            role: "assistant".to_string(),
            content: content.to_string(),
            model: Some(model.to_string()),
            ..Turn::default()
        }
    }

    // Brad, 2026-11-17: "the next step is user can run the same question in
    // 1-nth models and checks the results of each one".
    #[test]
    fn the_list_names_every_model_that_answered_the_latest_question() {
        let store = scratch_store();
        let mut session = chat("Nails");
        session.messages.push(Turn {
            role: "user".to_string(),
            content: "Why do nails rust?".to_string(),
            ..Turn::default()
        });
        session.messages.push(answer(
            "mlx-community/gpt-oss-20b-MXFP4-Q8",
            "Water and air.",
        ));
        session
            .messages
            .push(answer("mlx-community/phi-4-4bit", "Oxygen."));
        session
            .messages
            .push(answer("mlx-community/gpt-oss-20b-MXFP4-Q8", "Again."));
        store.save(session).unwrap();
        assert_eq!(
            store.list().unwrap()[0].models,
            [
                "mlx-community/gpt-oss-20b-MXFP4-Q8",
                "mlx-community/phi-4-4bit"
            ],
            "each model that answered, in order, once; the earlier question's answer is not counted",
        );
    }

    #[test]
    fn a_question_with_no_answer_yet_names_no_model() {
        let mut messages = chat("Sums").messages;
        assert!(
            latest_answers(&messages).is_empty(),
            "an answer saved without its model names none"
        );
        messages.push(answer("mlx-community/phi-4-4bit", "4."));
        assert_eq!(latest_answers(&messages), ["mlx-community/phi-4-4bit"]);
        messages.push(Turn {
            role: "user".to_string(),
            content: "And 3+3?".to_string(),
            ..Turn::default()
        });
        assert!(latest_answers(&messages).is_empty());
        // A saved file may hold any role; only an answer names its model.
        messages.push(Turn {
            role: "system".to_string(),
            content: "Be short.".to_string(),
            model: Some("mlx-community/phi-4-4bit".to_string()),
            ..Turn::default()
        });
        assert!(
            latest_answers(&messages).is_empty(),
            "a turn that is no answer was counted"
        );
        assert!(latest_answers(&[]).is_empty());
    }

    // Brad, 2026-11-17: "The results are graded and summarized by grok after
    // all the results are in".
    #[test]
    fn a_grade_is_kept_and_names_no_model_as_an_answer() {
        let store = scratch_store();
        let mut session = chat("Nails");
        session.messages.push(Turn {
            role: "user".to_string(),
            content: "Why do nails rust?".to_string(),
            ..Turn::default()
        });
        session
            .messages
            .push(answer("mlx-community/phi-4-4bit", "Oxygen."));
        session.messages.push(answer(
            "mlx-community/gpt-oss-20b-MXFP4-Q8",
            "Water and air.",
        ));
        session.messages.push(Turn {
            role: "grade".to_string(),
            content: "Answer 1: B. Short.\nAnswer 2: A. Clear.\nSummary: Both are right."
                .to_string(),
            thinking: Some("Read both.".to_string()),
            model: Some("xai:grok-4".to_string()),
            ..Turn::default()
        });
        let saved = store.save(session).unwrap();
        let loaded = store.load(&saved.id).unwrap();
        assert_eq!(loaded, saved, "the grade does not come back whole");
        assert_eq!(
            loaded.messages.last().map(|turn| turn.role.as_str()),
            Some("grade")
        );
        assert_eq!(
            store.list().unwrap()[0].models,
            [
                "mlx-community/phi-4-4bit",
                "mlx-community/gpt-oss-20b-MXFP4-Q8"
            ],
            "the model that graded is named as one that answered",
        );
    }

    fn voted(model: &str, content: &str, rating: Option<&str>, best: bool, at: u64) -> Turn {
        Turn {
            rating: rating.map(str::to_string),
            best: best.then_some(true),
            voted_at: Some(at),
            ..answer(model, content)
        }
    }

    fn question(content: &str) -> Turn {
        Turn {
            role: "user".to_string(),
            content: content.to_string(),
            ..Turn::default()
        }
    }

    // Brad, 2026-11-20: "I should be able to rate answers, and it keeps track
    // of which answers were asked when it got voted the best one."
    #[test]
    fn a_vote_is_saved_with_its_answer_and_comes_back_whole() {
        let store = scratch_store();
        let mut session = chat("Nails");
        session.messages.push(question("Why do nails rust?"));
        session.messages.push(voted(
            "mlx-community/phi-4-4bit",
            "Oxygen.",
            Some("down"),
            false,
            5,
        ));
        session.messages.push(voted(
            "mlx-community/gpt-oss-20b-MXFP4-Q8",
            "Water and air.",
            Some("up"),
            true,
            7,
        ));
        let saved = store.save(session).unwrap();
        let loaded = store.load(&saved.id).unwrap();
        assert_eq!(loaded, saved, "a vote does not come back whole");
        let best = &loaded.messages[4];
        assert_eq!(best.rating.as_deref(), Some("up"));
        assert_eq!(best.best, Some(true));
        assert_eq!(best.voted_at, Some(7));
        let text = serde_json::to_string(&chat("Sums")).unwrap();
        assert!(
            !text.contains("rating") && !text.contains("best") && !text.contains("votedAt"),
            "a chat with no votes saves empty vote fields"
        );
    }

    #[test]
    fn the_votes_name_the_question_every_model_that_answered_and_the_best() {
        let store = scratch_store();
        let mut session = chat("Nails");
        // The first question has an answer with no vote, so it is left out.
        session.messages.push(question("Why do nails rust?"));
        session
            .messages
            .push(voted("mlx-community/phi-4-4bit", "Oxygen.", None, false, 0));
        session.messages.push(voted(
            "mlx-community/gpt-oss-20b-MXFP4-Q8",
            "Water and air.",
            None,
            true,
            9,
        ));
        session.messages.push(Turn {
            role: "grade".to_string(),
            content: "Answer 1: B.".to_string(),
            model: Some("xai:grok-4".to_string()),
            best: Some(true),
            ..Turn::default()
        });
        session.messages.push(question("And copper?"));
        session.messages.push(voted(
            "mlx-community/phi-4-4bit",
            "It goes green.",
            Some("up"),
            false,
            4,
        ));
        let saved = store.save(session).unwrap();
        let votes = store.votes().unwrap();
        assert_eq!(votes.len(), 2, "the question with no vote is in the list");
        assert_eq!(
            votes[0],
            VotedRound {
                session: saved.id.clone(),
                title: "Nails".to_string(),
                question: "Why do nails rust?".to_string(),
                voted_at: 9,
                answers: vec![
                    VotedAnswer {
                        model: "mlx-community/phi-4-4bit".to_string(),
                        rating: None,
                        best: false,
                    },
                    VotedAnswer {
                        model: "mlx-community/gpt-oss-20b-MXFP4-Q8".to_string(),
                        rating: None,
                        best: true,
                    },
                ],
            },
            "the latest vote comes first, with the question and every model that answered; the grader is no answer",
        );
        assert_eq!(votes[1].question, "And copper?");
        assert_eq!(votes[1].answers[0].rating.as_deref(), Some("up"));
    }

    #[test]
    fn a_best_needs_two_answers_and_a_rating_is_up_or_down() {
        let mut session = chat("Sums");
        session.messages[1] = voted("mlx-community/phi-4-4bit", "4.", Some("meh"), true, 3);
        assert!(
            voted_rounds(&session).is_empty(),
            "a lone answer was voted best, or a rating other than up or down counted"
        );
        session.messages.push(question(&"long ".repeat(100)));
        session.messages.push(voted("a", "One.", None, true, 1));
        session.messages.push(voted("b", "Two.", None, true, 2));
        let rounds = voted_rounds(&session);
        assert_eq!(rounds.len(), 1);
        assert_eq!(
            rounds[0]
                .answers
                .iter()
                .filter(|answer| answer.best)
                .count(),
            1,
            "two answers to one question were both best"
        );
        assert_eq!(rounds[0].question.chars().count(), QUESTION_LIMIT);
    }

    #[test]
    fn a_chat_in_the_trash_takes_its_votes_with_it() {
        let store = scratch_store();
        let mut session = chat("Sums");
        session.messages[1] = voted("mlx-community/phi-4-4bit", "4.", Some("up"), false, 3);
        let saved = store.save(session).unwrap();
        assert_eq!(store.votes().unwrap().len(), 1);
        store.trash(&saved.id).unwrap();
        assert!(store.votes().unwrap().is_empty());
    }

    #[test]
    fn search_finds_titles_and_words_ignoring_case() {
        let store = scratch_store();
        let sums = store.save(chat("Sums")).unwrap();
        let mut other = chat("Paleo weekend");
        other.messages[1].content =
            "Buy eggs, bacon, and SWEET potatoes for the weekend.".to_string();
        let paleo = store.save(other).unwrap();
        assert_eq!(store.search("sums").unwrap()[0].session.id, sums.id);
        let found = store.search("sweet").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].session.id, paleo.id);
        assert!(found[0].snippet.contains("SWEET potatoes"));
        assert!(store.search("  ").unwrap().is_empty());
        assert!(store.search("nowhere").unwrap().is_empty());
    }

    #[test]
    fn arranging_pins_and_files_a_chat_without_moving_it_in_time() {
        let store = scratch_store();
        let saved = store.save(chat("Sums")).unwrap();
        let arranged = store
            .arrange(&saved.id, true, Some("gledit".to_string()))
            .unwrap();
        assert!(arranged.pinned);
        assert_eq!(arranged.project.as_deref(), Some("gledit"));
        assert_eq!(arranged.updated_at, saved.updated_at);
        let back = store
            .arrange(&saved.id, false, Some(String::new()))
            .unwrap();
        assert!(!back.pinned && back.project.is_none());
        assert!(store.arrange("../x", true, None).is_err());
    }

    #[test]
    fn an_empty_title_gets_a_name_and_a_long_one_is_cut() {
        let store = scratch_store();
        assert_eq!(store.save(chat("   ")).unwrap().title, "New chat");
        assert_eq!(
            store
                .save(chat(&"t".repeat(500)))
                .unwrap()
                .title
                .chars()
                .count(),
            TITLE_LIMIT
        );
    }
}
