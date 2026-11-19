//! Projects and memory, kept as plain files in the app's own data folder.
//!
//! A project is one JSON file in `projects`: a name, instructions the model
//! is given in each of its chats, and the project's memory. The memory that
//! goes with every chat is one Markdown file, `memory.md`, beside it. Both
//! are written however the person likes, and the app adds to them as they
//! talk, as Markdown bullets at the end.
//!
//! As with the chats, nothing here deletes: "Trash" moves a project's file
//! into a `trash` folder beside the others.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const NAME_LIMIT: usize = 80;
/// How much memory, global or a project's, the app keeps: enough for pages of
/// notes, and little enough to fit beside a chat in the smallest model.
pub const MEMORY_LIMIT: usize = 16_000;
const FACT_LIMIT: usize = 300;
const FACTS_AT_ONCE: usize = 5;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub memory: String,
    /// A folder on this computer the model may read and, with a yes, change.
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
}

pub struct Projects {
    dir: PathBuf,
    memory: PathBuf,
    /// One write at a time, so memory the app adds never crosses a save.
    writing: Mutex<()>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|since| since.as_millis() as u64).unwrap_or(0)
}

/// Letters, digits, and hyphens only, so an id never names a file elsewhere.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn failed(what: &str) -> impl Fn(std::io::Error) -> String + '_ {
    move |error| format!("{what}: {error}")
}

fn cut(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// Writes beside the final place, then moves over it, so a write cut short
/// never leaves half a file.
fn write_whole(path: &PathBuf, text: &str) -> Result<(), String> {
    let partial = path.with_extension("partial");
    fs::write(&partial, text).map_err(failed("Could not save"))?;
    fs::rename(&partial, path).map_err(failed("Could not save"))
}

/// `memory` with each fact added as a bullet at its end. A fact already in
/// it, or said twice, is added once; blank ones are skipped.
pub fn with_facts(memory: &str, facts: &[String]) -> String {
    let mut text = memory.trim_end().to_string();
    let known = text.to_lowercase();
    let mut added: Vec<String> = Vec::new();
    for fact in facts.iter().take(FACTS_AT_ONCE) {
        let line = cut(fact.trim().trim_start_matches(['-', '*', ' ']).trim(), FACT_LIMIT);
        let lower = line.to_lowercase();
        if line.is_empty() || known.contains(&lower) || added.iter().any(|seen| seen.to_lowercase() == lower) {
            continue;
        }
        added.push(line);
    }
    for line in added {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str("- ");
        text.push_str(&line);
    }
    text
}

impl Projects {
    pub fn new(dir: PathBuf, memory: PathBuf) -> Self {
        Projects { dir, memory, writing: Mutex::new(()) }
    }

    fn ready(&self) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(failed("Could not open the projects folder"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700));
        }
        Ok(())
    }

    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("That is not a project.".to_string());
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    /// Every project, by name. A file that cannot be read is skipped.
    pub fn list(&self) -> Result<Vec<Project>, String> {
        self.ready()?;
        let mut projects = Vec::new();
        for entry in fs::read_dir(&self.dir).map_err(failed("Could not read the projects folder"))?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else { continue };
            let Ok(project) = serde_json::from_str::<Project>(&text) else { continue };
            if valid_id(&project.id) {
                projects.push(project);
            }
        }
        projects.sort_by_key(|project| project.name.to_lowercase());
        Ok(projects)
    }

    pub fn load(&self, id: &str) -> Result<Project, String> {
        let text = fs::read_to_string(self.path(id)?).map_err(failed("Could not open that project"))?;
        serde_json::from_str(&text).map_err(|_| "That project's file could not be read.".to_string())
    }

    pub fn save(&self, mut project: Project) -> Result<Project, String> {
        let _held = self.writing.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.ready()?;
        if project.id.is_empty() {
            project.id = uuid::Uuid::new_v4().to_string();
        }
        let path = self.path(&project.id)?;
        let name = cut(project.name.trim(), NAME_LIMIT);
        project.name = if name.is_empty() { "New project".to_string() } else { name };
        project.memory = cut(&project.memory, MEMORY_LIMIT);
        project.folder = project.folder.trim().trim_end_matches('/').to_string();
        if !project.folder.is_empty() && !std::path::Path::new(&project.folder).is_dir() {
            return Err("That folder is not on this computer.".to_string());
        }
        let now = now_ms();
        if project.created_at == 0 {
            project.created_at = now;
        }
        project.updated_at = now;
        let text = serde_json::to_string_pretty(&project).map_err(|_| "Could not write the project.".to_string())?;
        write_whole(&path, &text)?;
        Ok(project)
    }

    pub fn trash(&self, id: &str) -> Result<(), String> {
        let path = self.path(id)?;
        let trash = self.dir.join("trash");
        fs::create_dir_all(&trash).map_err(failed("Could not open the trash folder"))?;
        fs::rename(&path, trash.join(format!("{id}-{}.json", now_ms()))).map_err(failed("Could not move that project to the trash"))
    }

    /// The memory that goes with every chat. None yet is an empty one.
    pub fn memory(&self) -> Result<String, String> {
        match fs::read_to_string(&self.memory) {
            Ok(text) => Ok(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(error) => Err(format!("Could not read the memory: {error}")),
        }
    }

    pub fn save_memory(&self, text: &str) -> Result<String, String> {
        let _held = self.writing.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.write_memory(text)
    }

    fn write_memory(&self, text: &str) -> Result<String, String> {
        if let Some(parent) = self.memory.parent() {
            fs::create_dir_all(parent).map_err(failed("Could not open the data folder"))?;
        }
        let text = cut(text, MEMORY_LIMIT);
        write_whole(&self.memory, &text)?;
        Ok(text)
    }

    /// Adds facts to the global memory, or to a project's when one is named.
    /// Facts past what the memory can hold are not added.
    pub fn remember(&self, facts: &[String], project: Option<&str>) -> Result<(), String> {
        let _held = self.writing.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match project {
            None => {
                let text = with_facts(&self.memory()?, facts);
                if text.chars().count() <= MEMORY_LIMIT {
                    self.write_memory(&text)?;
                }
            }
            Some(id) => {
                let mut kept = self.load(id)?;
                let text = with_facts(&kept.memory, facts);
                if text.chars().count() <= MEMORY_LIMIT {
                    kept.memory = text;
                    kept.updated_at = now_ms();
                    let json = serde_json::to_string_pretty(&kept).map_err(|_| "Could not write the project.".to_string())?;
                    write_whole(&self.path(id)?, &json)?;
                }
            }
        }
        Ok(())
    }
}

#[tauri::command]
pub fn list_projects(projects: tauri::State<'_, Projects>) -> Result<Vec<Project>, String> {
    projects.list()
}

#[tauri::command]
pub fn save_project(projects: tauri::State<'_, Projects>, project: Project) -> Result<Project, String> {
    projects.save(project)
}

#[tauri::command]
pub fn trash_project(projects: tauri::State<'_, Projects>, id: String) -> Result<(), String> {
    projects.trash(&id)
}

#[tauri::command]
pub fn load_memory(projects: tauri::State<'_, Projects>) -> Result<String, String> {
    projects.memory()
}

#[tauri::command]
pub fn save_memory(projects: tauri::State<'_, Projects>, text: String) -> Result<String, String> {
    projects.save_memory(&text)
}

#[tauri::command]
pub fn remember(projects: tauri::State<'_, Projects>, facts: Vec<String>, project: Option<String>) -> Result<(), String> {
    projects.remember(&facts, project.as_deref().filter(|id| !id.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> Projects {
        let dir = std::env::temp_dir().join(format!("rusty-business-projects-{}", uuid::Uuid::new_v4()));
        Projects::new(dir.join("projects"), dir.join("memory.md"))
    }

    fn facts(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| line.to_string()).collect()
    }

    #[test]
    fn a_saved_project_comes_back_and_lists_by_name() {
        let store = scratch();
        let zebra = store.save(Project { name: "Zebra".into(), ..Project::default() }).unwrap();
        store.save(Project { name: "apple".into(), instructions: "Be brief.".into(), ..Project::default() }).unwrap();
        assert!(valid_id(&zebra.id));
        assert_eq!(store.load(&zebra.id).unwrap(), zebra);
        let names: Vec<String> = store.list().unwrap().into_iter().map(|project| project.name).collect();
        assert_eq!(names, ["apple", "Zebra"]);
    }

    #[test]
    fn a_blank_name_gets_one() {
        assert_eq!(scratch().save(Project::default()).unwrap().name, "New project");
    }

    #[test]
    fn trash_moves_the_project_and_deletes_nothing() {
        let store = scratch();
        let saved = store.save(Project { name: "Odd".into(), ..Project::default() }).unwrap();
        store.trash(&saved.id).unwrap();
        assert!(store.list().unwrap().is_empty());
        assert_eq!(fs::read_dir(store.dir.join("trash")).unwrap().count(), 1);
    }

    #[test]
    fn an_id_cannot_reach_outside_the_folder() {
        let store = scratch();
        for id in ["../x", "a/b", "", "a.json"] {
            assert!(store.load(id).is_err());
            assert!(store.trash(id).is_err());
            assert!(store.remember(&facts(&["x"]), Some(id)).is_err());
        }
    }

    #[test]
    fn global_memory_starts_empty_and_keeps_what_is_written() {
        let store = scratch();
        assert_eq!(store.memory().unwrap(), "");
        store.save_memory("# Me\nLives in Jacksonville.").unwrap();
        assert_eq!(store.memory().unwrap(), "# Me\nLives in Jacksonville.");
    }

    #[test]
    fn remembered_facts_land_as_bullets_once_each() {
        let store = scratch();
        store.save_memory("Lives in Jacksonville.").unwrap();
        store.remember(&facts(&["Prefers Rust", " ", "- prefers rust", "lives in jacksonville."]), None).unwrap();
        assert_eq!(store.memory().unwrap(), "Lives in Jacksonville.\n- Prefers Rust");

        let project = store.save(Project { name: "Gledit".into(), ..Project::default() }).unwrap();
        store.remember(&facts(&["Targets small businesses"]), Some(&project.id)).unwrap();
        assert_eq!(store.load(&project.id).unwrap().memory, "- Targets small businesses");
        assert_eq!(store.memory().unwrap(), "Lives in Jacksonville.\n- Prefers Rust", "the global memory is untouched");
    }

    #[test]
    fn a_full_memory_takes_no_more() {
        let store = scratch();
        let full = "x".repeat(MEMORY_LIMIT);
        store.save_memory(&format!("{full}more")).unwrap();
        assert_eq!(store.memory().unwrap(), full, "a save is cut to the limit");
        store.remember(&facts(&["One more"]), None).unwrap();
        assert_eq!(store.memory().unwrap(), full);
    }
}
