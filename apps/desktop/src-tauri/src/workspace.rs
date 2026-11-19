//! A project's folder, for coding work: the model lists, reads, and searches
//! the files in it, and writes or edits them once the person says yes.
//!
//! Every path the model names is relative to the folder and is checked here,
//! after links are followed, to still be inside it. The page never names the
//! folder itself: it names the project, and the folder comes from the
//! project's own file. What the model reads is capped, so one large file
//! never fills a small model's window.

use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::projects::Projects;

/// Folders that are built, fetched, or kept by tools, never read through.
const SKIPPED: &[&str] = &[
    ".git", "node_modules", "target", "dist", "build", ".next", ".nuxt", "vendor", "__pycache__", ".venv", "venv",
    ".idea", ".vscode", ".DS_Store", "coverage", ".cache",
];
const LIST_LIMIT: usize = 300;
const OVERVIEW_LIMIT: usize = 150;
const READ_LINES: usize = 300;
const READ_BYTES: usize = 24_000;
const SEARCH_HITS: usize = 60;
const SEARCH_FILE_BYTES: u64 = 1_000_000;
const SEARCH_FILES: usize = 5_000;
const WRITE_BYTES: usize = 1_000_000;

fn skipped(name: &str) -> bool {
    SKIPPED.contains(&name)
}

/// The folder, as it really is on the disk.
fn root_of(folder: &str) -> Result<PathBuf, String> {
    let folder = folder.trim();
    if folder.is_empty() {
        return Err("This project has no folder.".to_string());
    }
    let root = fs::canonicalize(folder).map_err(|_| "The project's folder is not on this computer.".to_string())?;
    if !root.is_dir() {
        return Err("The project's folder is not a folder.".to_string());
    }
    Ok(root)
}

/// `relative` inside `root`, or an error when it would lead anywhere else.
/// A path that is not there yet is checked through the nearest folder that is.
pub fn inside(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = relative.trim();
    let relative = if relative.is_empty() { "." } else { relative };
    let path = Path::new(relative);
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err(format!("{relative} is outside the project's folder. Paths are relative to it.")),
        }
    }
    let joined = root.join(path);
    let mut existing = joined.as_path();
    let mut rest = Vec::new();
    while !existing.exists() {
        rest.push(existing.file_name().map(|name| name.to_os_string()).unwrap_or_default());
        existing = existing.parent().ok_or_else(|| format!("{relative} is outside the project's folder."))?;
    }
    let mut real = fs::canonicalize(existing).map_err(|_| format!("Could not open {relative}."))?;
    if !real.starts_with(root) {
        return Err(format!("{relative} is outside the project's folder."));
    }
    for name in rest.into_iter().rev() {
        real.push(name);
    }
    Ok(real)
}

fn shown(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path).to_string_lossy().to_string();
    if relative.is_empty() { ".".to_string() } else { relative }
}

fn sorted_entries(dir: &Path) -> Vec<fs::DirEntry> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)
        .map(|read| read.flatten().filter(|entry| !skipped(&entry.file_name().to_string_lossy())).collect())
        .unwrap_or_default();
    entries.sort_by_key(|entry| (!entry.path().is_dir(), entry.file_name().to_string_lossy().to_lowercase()));
    entries
}

/// The folder's top two levels, for the model to start from.
pub fn overview(root: &Path) -> String {
    let mut lines = Vec::new();
    let mut more = false;
    for entry in sorted_entries(root) {
        let name = entry.file_name().to_string_lossy().to_string();
        let dir = entry.path().is_dir();
        if lines.len() >= OVERVIEW_LIMIT {
            more = true;
            break;
        }
        lines.push(if dir { format!("{name}/") } else { name.clone() });
        if dir {
            for inner in sorted_entries(&entry.path()) {
                if lines.len() >= OVERVIEW_LIMIT {
                    more = true;
                    break;
                }
                let inner_name = inner.file_name().to_string_lossy().to_string();
                lines.push(if inner.path().is_dir() { format!("  {inner_name}/") } else { format!("  {inner_name}") });
            }
        }
    }
    if more {
        lines.push("(more not shown; use list)".to_string());
    }
    if lines.is_empty() { "(empty)".to_string() } else { lines.join("\n") }
}

fn list(root: &Path, relative: &str) -> Result<String, String> {
    let dir = inside(root, relative)?;
    if !dir.is_dir() {
        return Err(format!("{relative} is not a folder."));
    }
    let entries = sorted_entries(&dir);
    let mut lines: Vec<String> = entries
        .iter()
        .take(LIST_LIMIT)
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_dir() { format!("{name}/") } else { name }
        })
        .collect();
    if entries.len() > LIST_LIMIT {
        lines.push(format!("({} more not shown)", entries.len() - LIST_LIMIT));
    }
    Ok(if lines.is_empty() { "(empty)".to_string() } else { lines.join("\n") })
}

fn text_of(path: &Path, relative: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|_| format!("Could not read {relative}."))?;
    if bytes.iter().take(8_000).any(|byte| *byte == 0) {
        return Err(format!("{relative} is not a text file."));
    }
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// Lines `from` to `to` (counted from 1), numbered, within the caps.
fn read(root: &Path, relative: &str, from: Option<usize>, to: Option<usize>) -> Result<String, String> {
    let path = inside(root, relative)?;
    if !path.is_file() {
        return Err(format!("{relative} is not a file."));
    }
    let text = text_of(&path, relative)?;
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    let start = from.unwrap_or(1).max(1);
    let end = to.unwrap_or(start + READ_LINES - 1).min(total).min(start + READ_LINES - 1);
    let mut out = String::new();
    let mut last = start.saturating_sub(1);
    for (index, line) in lines.iter().enumerate().take(end).skip(start - 1) {
        let numbered = format!("{:>5}  {line}\n", index + 1);
        if out.len() + numbered.len() > READ_BYTES {
            break;
        }
        out.push_str(&numbered);
        last = index + 1;
    }
    if total == 0 {
        return Ok(format!("{relative} is empty."));
    }
    if start > total {
        return Err(format!("{relative} has only {total} lines."));
    }
    if last < total {
        out.push_str(&format!("(lines {start}-{last} of {total}; read again with from {} for more)", last + 1));
    }
    Ok(out)
}

fn search(root: &Path, text: &str, relative: &str) -> Result<String, String> {
    let needle = text.trim().to_lowercase();
    if needle.is_empty() {
        return Err("Search for some text.".to_string());
    }
    let start = inside(root, relative)?;
    let mut hits = Vec::new();
    let mut files = 0;
    let mut stack = vec![start];
    'walk: while let Some(path) = stack.pop() {
        if path.is_dir() {
            let mut inner: Vec<PathBuf> = sorted_entries(&path).into_iter().map(|entry| entry.path()).collect();
            inner.reverse();
            stack.extend(inner);
            continue;
        }
        files += 1;
        if files > SEARCH_FILES {
            break;
        }
        if fs::metadata(&path).map(|meta| meta.len() > SEARCH_FILE_BYTES).unwrap_or(true) {
            continue;
        }
        let Ok(body) = text_of(&path, "") else { continue };
        for (index, line) in body.lines().enumerate() {
            if line.to_lowercase().contains(&needle) {
                let line: String = line.trim().chars().take(200).collect();
                hits.push(format!("{}:{}: {line}", shown(root, &path), index + 1));
                if hits.len() >= SEARCH_HITS {
                    hits.push("(more not shown; search for something narrower)".to_string());
                    break 'walk;
                }
            }
        }
    }
    Ok(if hits.is_empty() { format!("Nothing has \"{}\".", text.trim()) } else { hits.join("\n") })
}

fn writable(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = inside(root, relative)?;
    if path == root || path.components().any(|part| part.as_os_str() == ".git") {
        return Err(format!("{relative} cannot be written."));
    }
    if path.is_dir() {
        return Err(format!("{relative} is a folder."));
    }
    Ok(path)
}

fn write(root: &Path, relative: &str, content: &str) -> Result<String, String> {
    if content.len() > WRITE_BYTES {
        return Err("That is too much to write at once.".to_string());
    }
    let path = writable(root, relative)?;
    let existed = path.exists();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| format!("Could not make the folder for {relative}."))?;
    }
    fs::write(&path, content).map_err(|_| format!("Could not write {relative}."))?;
    let lines = content.lines().count();
    Ok(format!("{} {relative} ({lines} lines).", if existed { "Wrote" } else { "Made" }))
}

fn edit(root: &Path, relative: &str, find: &str, replace: &str) -> Result<String, String> {
    if find.is_empty() {
        return Err("Say what text to find.".to_string());
    }
    let path = writable(root, relative)?;
    if !path.is_file() {
        return Err(format!("{relative} is not a file. Use write to make it."));
    }
    let text = text_of(&path, relative)?;
    match text.matches(find).count() {
        0 => Err(format!("{relative} does not have that text. Read it again and copy the text exactly.")),
        1 => {
            fs::write(&path, text.replacen(find, replace, 1)).map_err(|_| format!("Could not write {relative}."))?;
            Ok(format!("Edited {relative}."))
        }
        count => Err(format!("{relative} has that text {count} times. Include more lines so it is found once.")),
    }
}

/// One step the model asked for.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub find: String,
    #[serde(default)]
    pub replace: String,
    pub from: Option<usize>,
    pub to: Option<usize>,
}

pub fn run(root: &Path, call: &ToolCall) -> Result<String, String> {
    match call.tool.as_str() {
        "list" => list(root, &call.path),
        "read" => read(root, &call.path, call.from, call.to),
        "search" => search(root, &call.text, &call.path),
        "write" => write(root, &call.path, &call.content),
        "edit" => edit(root, &call.path, &call.find, &call.replace),
        other => Err(format!("There is no tool called {other}.")),
    }
}

#[tauri::command]
pub fn workspace_overview(projects: tauri::State<'_, Projects>, project: String) -> Result<String, String> {
    let root = root_of(&projects.load(&project)?.folder)?;
    Ok(overview(&root))
}

#[tauri::command]
pub fn workspace_tool(projects: tauri::State<'_, Projects>, project: String, call: ToolCall) -> Result<String, String> {
    let root = root_of(&projects.load(&project)?.folder)?;
    run(&root, &call)
}

/// The system's own folder picker. Empty when the person cancels.
#[tauri::command]
pub async fn pick_folder() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        if !cfg!(target_os = "macos") {
            return Err("Type the folder's path.".to_string());
        }
        let output = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", "POSIX path of (choose folder with prompt \"Choose the project's folder\")"])
            .output()
            .map_err(|_| "Could not open the folder picker.".to_string())?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().trim_end_matches('/').to_string())
    })
    .await
    .map_err(|_| "Could not open the folder picker.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rb-workspace-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        fs::write(dir.join("src/app.js"), "const a = 1;\nconst greeting = \"Hello\";\nexport default a;\n").unwrap();
        fs::write(dir.join("README.md"), "# Demo\n").unwrap();
        fs::write(dir.join("node_modules/pkg/index.js"), "Hello").unwrap();
        fs::canonicalize(dir).unwrap()
    }

    fn call(tool: &str) -> ToolCall {
        ToolCall { tool: tool.to_string(), ..ToolCall::default() }
    }

    #[test]
    fn paths_stay_inside_the_folder() {
        let root = folder();
        assert!(inside(&root, "../etc/passwd").is_err());
        assert!(inside(&root, "/etc/passwd").is_err());
        assert!(inside(&root, "src/../../x").is_err());
        assert_eq!(inside(&root, "src/app.js").unwrap(), root.join("src/app.js"));
        assert_eq!(inside(&root, "new/dir/file.txt").unwrap(), root.join("new/dir/file.txt"));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/", root.join("escape")).unwrap();
            assert!(inside(&root, "escape/etc").is_err());
        }
    }

    #[test]
    fn lists_reads_and_searches_without_tool_folders() {
        let root = folder();
        assert_eq!(list(&root, "").unwrap(), "src/\nREADME.md");
        assert!(overview(&root).contains("  app.js"));
        assert!(!overview(&root).contains("node_modules"));
        let read = read(&root, "src/app.js", Some(2), Some(2)).unwrap();
        assert!(read.starts_with("    2  const greeting"));
        assert!(read.contains("lines 2-2 of 3"));
        assert_eq!(search(&root, "hello", "").unwrap(), "src/app.js:2: const greeting = \"Hello\";");
    }

    #[test]
    fn writes_and_edits_only_what_they_name() {
        let root = folder();
        let mut make = call("write");
        make.path = "src/new/thing.txt".to_string();
        make.content = "one\ntwo\n".to_string();
        assert_eq!(run(&root, &make).unwrap(), "Made src/new/thing.txt (2 lines).");
        let mut change = call("edit");
        change.path = "src/app.js".to_string();
        change.find = "const a = 1;".to_string();
        change.replace = "const a = 2;".to_string();
        assert_eq!(run(&root, &change).unwrap(), "Edited src/app.js.");
        assert!(fs::read_to_string(root.join("src/app.js")).unwrap().starts_with("const a = 2;"));
        change.find = "const".to_string();
        assert!(run(&root, &change).unwrap_err().contains("2 times"));
        let mut git = call("write");
        git.path = ".git/config".to_string();
        assert!(run(&root, &git).is_err());
        assert!(run(&root, &call("delete")).is_err());
    }
}
