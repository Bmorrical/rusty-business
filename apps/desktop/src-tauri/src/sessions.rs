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
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub model: String,
    pub updated_at: u64,
    pub turns: usize,
    /// The models that answered the latest question, in the order they
    /// answered, each once.
    pub models: Vec<String>,
}

/// The models that answered the latest question: the replies after the last
/// question, in order, each model once.
pub fn latest_answers(messages: &[Turn]) -> Vec<String> {
    let start = messages.iter().rposition(|turn| turn.role == "user").map(|at| at + 1).unwrap_or(0);
    let mut models: Vec<String> = Vec::new();
    for turn in &messages[start..] {
        if let Some(model) = turn.model.as_ref().filter(|model| turn.role == "assistant" && !model.is_empty()) {
            if !models.contains(model) {
                models.push(model.clone());
            }
        }
    }
    models
}

pub struct Store {
    dir: PathBuf,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|since| since.as_millis() as u64).unwrap_or(0)
}

/// An id is letters, digits, and hyphens only, so it can never name a file
/// outside the sessions folder.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|character| character.is_ascii_alphanumeric() || character == '-')
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
        self.ready()?;
        let mut summaries = Vec::new();
        for entry in fs::read_dir(&self.dir).map_err(failed("Could not read the sessions folder"))?.flatten() {
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
            summaries.push(SessionSummary {
                id: session.id,
                title: session.title,
                model: session.model,
                updated_at: session.updated_at,
                turns: session.messages.len(),
                models: latest_answers(&session.messages),
            });
        }
        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }

    pub fn load(&self, id: &str) -> Result<Session, String> {
        let text = fs::read_to_string(self.path(id)?).map_err(failed("Could not open that session"))?;
        serde_json::from_str(&text).map_err(|_| "That session's file could not be read.".to_string())
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
        session.title = if title.is_empty() { "New chat".to_string() } else { title };
        let text = serde_json::to_string_pretty(&session).map_err(|_| "Could not write the session.".to_string())?;
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
        fs::rename(&path, trash.join(format!("{id}-{}.json", now_ms()))).map_err(failed("Could not move that session to the trash"))
    }
}

#[tauri::command]
pub fn list_sessions(store: tauri::State<'_, Store>) -> Result<Vec<SessionSummary>, String> {
    store.list()
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
pub fn trash_session(store: tauri::State<'_, Store>, id: String) -> Result<(), String> {
    store.trash(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_store() -> Store {
        Store::new(std::env::temp_dir().join(format!("rusty-business-test-{}", uuid::Uuid::new_v4())))
    }

    fn chat(title: &str) -> Session {
        Session {
            title: title.to_string(),
            model: "mlx-community/phi-4-4bit".to_string(),
            messages: vec![
                Turn { role: "user".to_string(), content: "What is 2+2?".to_string(), ..Turn::default() },
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
        edited.messages.push(Turn { role: "user".to_string(), content: "And 3+3?".to_string(), ..Turn::default() });
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
        fs::write(store.path(&older.id).unwrap(), serde_json::to_string(&older).unwrap()).unwrap();
        fs::write(store.dir.join("broken.json"), "not json").unwrap();
        let titles: Vec<String> = store.list().unwrap().into_iter().map(|summary| summary.title).collect();
        assert_eq!(titles, ["Newer", "Older"]);
        assert!(store.dir.join("broken.json").exists());
        assert_eq!(store.load(&newer.id).unwrap().title, "Newer");
    }

    #[test]
    fn an_id_cannot_reach_outside_the_folder() {
        let store = scratch_store();
        for id in ["../secrets", "..", "a/b", "a\\b", "", "name.json", &"x".repeat(65)] {
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
        let kept: Vec<_> = fs::read_dir(store.dir.join("trash")).unwrap().flatten().collect();
        assert_eq!(kept.len(), 1);
        let text = fs::read_to_string(kept[0].path()).unwrap();
        assert_eq!(serde_json::from_str::<Session>(&text).unwrap(), saved);
    }

    fn answer(model: &str, content: &str) -> Turn {
        Turn { role: "assistant".to_string(), content: content.to_string(), model: Some(model.to_string()), ..Turn::default() }
    }

    // Brad, 2026-11-17: "the next step is user can run the same question in
    // 1-nth models and checks the results of each one".
    #[test]
    fn the_list_names_every_model_that_answered_the_latest_question() {
        let store = scratch_store();
        let mut session = chat("Nails");
        session.messages.push(Turn { role: "user".to_string(), content: "Why do nails rust?".to_string(), ..Turn::default() });
        session.messages.push(answer("mlx-community/gpt-oss-20b-MXFP4-Q8", "Water and air."));
        session.messages.push(answer("mlx-community/phi-4-4bit", "Oxygen."));
        session.messages.push(answer("mlx-community/gpt-oss-20b-MXFP4-Q8", "Again."));
        store.save(session).unwrap();
        assert_eq!(
            store.list().unwrap()[0].models,
            ["mlx-community/gpt-oss-20b-MXFP4-Q8", "mlx-community/phi-4-4bit"],
            "each model that answered, in order, once; the earlier question's answer is not counted",
        );
    }

    #[test]
    fn a_question_with_no_answer_yet_names_no_model() {
        let mut messages = chat("Sums").messages;
        assert!(latest_answers(&messages).is_empty(), "an answer saved without its model names none");
        messages.push(answer("mlx-community/phi-4-4bit", "4."));
        assert_eq!(latest_answers(&messages), ["mlx-community/phi-4-4bit"]);
        messages.push(Turn { role: "user".to_string(), content: "And 3+3?".to_string(), ..Turn::default() });
        assert!(latest_answers(&messages).is_empty());
        // A saved file may hold any role; only an answer names its model.
        messages.push(Turn {
            role: "system".to_string(),
            content: "Be short.".to_string(),
            model: Some("mlx-community/phi-4-4bit".to_string()),
            ..Turn::default()
        });
        assert!(latest_answers(&messages).is_empty(), "a turn that is no answer was counted");
        assert!(latest_answers(&[]).is_empty());
    }

    #[test]
    fn an_empty_title_gets_a_name_and_a_long_one_is_cut() {
        let store = scratch_store();
        assert_eq!(store.save(chat("   ")).unwrap().title, "New chat");
        assert_eq!(store.save(chat(&"t".repeat(500))).unwrap().title.chars().count(), TITLE_LIMIT);
    }
}
