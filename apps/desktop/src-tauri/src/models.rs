//! The models already on this computer.
//!
//! The engine keeps each model it has downloaded in a folder of its own,
//! inside the Hugging Face cache folder. This file looks there, so the app
//! can say which of its models are on the disk before the engine is running.
//!
//! Three rules live here:
//! - it only reads: nothing in the folder is written, moved, or removed;
//! - the app keeps no list of its own: the disk is asked every time;
//! - only models on the app's list are reported, and only when whole: the
//!   files the engine asks for, and every weights file the model names.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::engine::{self, ModelInfo};

/// How the cache names a model's folder: `models--` and then the model's id,
/// with `--` where the id has a slash.
const MODEL_FOLDER: &str = "models--";

/// The file that names a model's weights files.
const WEIGHTS_INDEX: &str = "model.safetensors.index.json";

/// The files the engine asks for before it calls a folder a model.
const NEEDED: [&str; 3] = ["config.json", WEIGHTS_INDEX, "tokenizer_config.json"];

// ---------------------------------------------------------------------------
// Where the models are kept
// ---------------------------------------------------------------------------

/// The cache folder that holds the models, worked out the way the engine
/// works it out: `HF_HUB_CACHE`, then the older `HUGGINGFACE_HUB_CACHE`, then
/// `hub` under `HF_HOME`, then under `XDG_CACHE_HOME/huggingface`, then under
/// `.cache/huggingface` in the home folder. A leading `~` is the home folder.
///
/// `variable` reads one environment variable. A folder that is not a full
/// path is no answer: the app does not start where the engine starts.
pub fn hub_folder(variable: impl Fn(&str) -> Option<String>, home: Option<&Path>) -> Option<PathBuf> {
    let set = |name: &str| variable(name).map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
    let folder = if let Some(hub) = set("HF_HUB_CACHE").or_else(|| set("HUGGINGFACE_HUB_CACHE")) {
        from_home(&hub, home)
    } else if let Some(hugging_face) = set("HF_HOME") {
        from_home(&hugging_face, home).map(|folder| folder.join("hub"))
    } else if let Some(cache) = set("XDG_CACHE_HOME") {
        from_home(&cache, home).map(|folder| folder.join("huggingface").join("hub"))
    } else {
        home.map(|home| home.join(".cache").join("huggingface").join("hub"))
    };
    folder.filter(|folder| folder.is_absolute())
}

/// A folder as a variable gives it, with a leading `~` read as the home folder.
fn from_home(written: &str, home: Option<&Path>) -> Option<PathBuf> {
    if written == "~" {
        return home.map(Path::to_path_buf);
    }
    match written.strip_prefix("~/") {
        Some(rest) => home.map(|home| home.join(rest)),
        None => Some(PathBuf::from(written)),
    }
}

// ---------------------------------------------------------------------------
// Which of the app's models are there
// ---------------------------------------------------------------------------

/// The app's models that are on this computer right now.
pub fn on_disk() -> Vec<ModelInfo> {
    let home = std::env::home_dir();
    match hub_folder(|name| std::env::var(name).ok(), home.as_deref()) {
        Some(hub) => models_in(&hub),
        None => Vec::new(),
    }
}

/// The app's models that are whole in `hub`, in the list's order. A folder
/// that is missing, or cannot be read, holds none.
pub fn models_in(hub: &Path) -> Vec<ModelInfo> {
    let Ok(entries) = fs::read_dir(hub) else {
        return Vec::new();
    };
    let mut folders = BTreeMap::new();
    for entry in entries.flatten() {
        if let Some(id) = entry.file_name().to_str().and_then(model_id) {
            folders.insert(id, entry.path());
        }
    }
    // The list is asked first, so a folder for a model that is not on it is
    // never opened.
    let ids: Vec<String> = folders.keys().cloned().collect();
    engine::allowed_models(&ids)
        .into_iter()
        .filter(|model| folders.get(&model.id).is_some_and(|folder| whole(folder)))
        .collect()
}

/// `models--mlx-community--phi-4-4bit` is `mlx-community/phi-4-4bit`.
fn model_id(folder: &str) -> Option<String> {
    let name = folder.strip_prefix(MODEL_FOLDER)?;
    (!name.is_empty()).then(|| name.replace("--", "/"))
}

#[derive(Deserialize)]
struct WeightsIndex {
    /// Each part of the model, and the weights file that holds it.
    #[serde(default)]
    weight_map: HashMap<String, String>,
}

/// The folder a model has in the models folder.
pub(crate) fn folder_name(id: &str) -> String {
    format!("{MODEL_FOLDER}{}", id.replace('/', "--"))
}

/// True when the folder holds a whole model: the download that `refs/main`
/// names has the files the engine asks for, and every weights file the model
/// names. A download that stopped part way leaves some of those out.
pub(crate) fn whole(folder: &Path) -> bool {
    let Ok(named) = fs::read_to_string(folder.join("refs").join("main")) else {
        return false;
    };
    let commit = named.trim();
    if !plain_name(commit) {
        return false;
    }
    let snapshot = folder.join("snapshots").join(commit);
    if !NEEDED.iter().all(|name| is_file(&snapshot.join(name))) {
        return false;
    }
    let Ok(text) = fs::read_to_string(snapshot.join(WEIGHTS_INDEX)) else {
        return false;
    };
    let Ok(index) = serde_json::from_str::<WeightsIndex>(&text) else {
        return false;
    };
    let weights: BTreeSet<&str> = index.weight_map.values().map(String::as_str).collect();
    !weights.is_empty() && weights.iter().all(|name| plain_name(name) && is_file(&snapshot.join(name)))
}

/// A file, or a link that leads to one. A link to nothing is not a file.
fn is_file(path: &Path) -> bool {
    fs::metadata(path).map(|found| found.is_file()).unwrap_or(false)
}

/// One plain name, with nothing in it that steps out of the folder it is
/// joined to.
fn plain_name(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!((parts.next(), parts.next()), (Some(Component::Normal(_)), None))
}

#[cfg(test)]
mod tests {
    // Brad, on what the app does by itself: "it should search and up the local
    // models and stack." These tests cover the search. Each one builds a folder
    // shaped like the Hugging Face cache, in the system's temporary folder.

    use super::*;
    use crate::engine::EngineStatus;

    const PHI: &str = "mlx-community/phi-4-4bit";
    const GPT_OSS: &str = "mlx-community/gpt-oss-20b-MXFP4-Q8";
    const NEMOTRON: &str = "mlx-community/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-4bit";
    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn scratch() -> PathBuf {
        let folder = std::env::temp_dir().join(format!("rusty-business-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn scratch_hub() -> PathBuf {
        let hub = scratch().join("hub");
        fs::create_dir_all(&hub).unwrap();
        hub
    }

    fn model_folder(hub: &Path, id: &str) -> PathBuf {
        hub.join(format!("{MODEL_FOLDER}{}", id.replace('/', "--")))
    }

    fn shard(number: usize, of: usize) -> String {
        format!("model-{number:05}-of-{of:05}.safetensors")
    }

    fn index_naming(weights: &[String]) -> String {
        let map: HashMap<String, &String> =
            weights.iter().enumerate().map(|(part, name)| (format!("layers.{part}.weight"), name)).collect();
        serde_json::json!({ "metadata": { "format": "mlx" }, "weight_map": map }).to_string()
    }

    /// Puts a whole model in the hub, with its weights in `shards` files, and
    /// returns the folder that holds the model's files.
    fn download(hub: &Path, id: &str, shards: usize) -> PathBuf {
        let folder = model_folder(hub, id);
        let snapshot = folder.join("snapshots").join(COMMIT);
        fs::create_dir_all(&snapshot).unwrap();
        fs::create_dir_all(folder.join("refs")).unwrap();
        fs::write(folder.join("refs").join("main"), COMMIT).unwrap();

        let weights: Vec<String> = (1..=shards).map(|number| shard(number, shards)).collect();
        fs::write(snapshot.join("config.json"), "{}").unwrap();
        fs::write(snapshot.join("tokenizer_config.json"), "{}").unwrap();
        fs::write(snapshot.join(WEIGHTS_INDEX), index_naming(&weights)).unwrap();
        for name in &weights {
            fs::write(snapshot.join(name), "weights").unwrap();
        }
        snapshot
    }

    /// What a download that stopped leaves behind: the file is not there yet.
    fn not_arrived(file: &Path) {
        let mut waiting = file.as_os_str().to_owned();
        waiting.push(".incomplete");
        fs::rename(file, waiting).unwrap();
    }

    fn names(models: &[ModelInfo]) -> Vec<&str> {
        models.iter().map(|model| model.name.as_str()).collect()
    }

    /// Every file and folder under `folder`, with its size and when it was
    /// last changed.
    fn everything_in(folder: &Path) -> Vec<String> {
        let mut seen = Vec::new();
        let mut waiting = vec![folder.to_path_buf()];
        while let Some(next) = waiting.pop() {
            for entry in fs::read_dir(&next).unwrap().flatten() {
                let found = fs::symlink_metadata(entry.path()).unwrap();
                seen.push(format!("{:?} {} {:?}", entry.path(), found.len(), found.modified().unwrap()));
                if found.is_dir() {
                    waiting.push(entry.path());
                }
            }
        }
        seen.sort();
        seen
    }

    #[test]
    fn the_apps_models_on_the_disk_are_found_in_the_lists_order() {
        // "it should search ... the local models"
        let hub = scratch_hub();
        download(&hub, NEMOTRON, 4);
        download(&hub, GPT_OSS, 3);
        download(&hub, PHI, 2);

        // The cache holds other things too. None of them is a model.
        fs::write(hub.join("CACHEDIR.TAG"), "Signature: 8a477f597d28d172789f06886806bc55").unwrap();
        fs::create_dir_all(hub.join("blobs")).unwrap();
        fs::create_dir_all(hub.join(".locks")).unwrap();
        fs::create_dir_all(hub.join("datasets--someone--phi-4-questions")).unwrap();
        fs::write(hub.join("models--mlx-community--phi-4-notes.txt"), "not a folder").unwrap();

        let found = models_in(&hub);
        assert_eq!(names(&found), ["Phi-4", "gpt-oss 20B", "Nemotron 3.5 Lightning"]);
        assert_eq!(found[0], ModelInfo { id: PHI.to_string(), name: "Phi-4".to_string(), maker: "Microsoft".to_string(), context: 16_384 });
        assert_eq!(found[1].id, GPT_OSS);
        assert_eq!(found[2].id, NEMOTRON);
    }

    #[test]
    fn a_folder_name_is_read_the_way_the_engine_reads_it() {
        assert_eq!(model_id("models--mlx-community--phi-4-4bit").as_deref(), Some(PHI));
        assert_eq!(model_id("models--phi-4").as_deref(), Some("phi-4"));
        assert_eq!(model_id("models--"), None);
        assert_eq!(model_id("datasets--someone--phi-4-questions"), None);
        assert_eq!(model_id("blobs"), None);
    }

    #[test]
    fn a_model_off_the_list_is_never_reported() {
        // Only the models on the app's list are reported. The list holds on
        // the disk as it does on the engine.
        let hub = scratch_hub();
        download(&hub, "mlx-community/gemma-3-27b-it-qat-4bit", 12);
        download(&hub, "mlx-community/Qwen3-30B-A3B-4bit", 1);
        assert_eq!(models_in(&hub), Vec::new());

        download(&hub, PHI, 2);
        assert_eq!(names(&models_in(&hub)), ["Phi-4"]);
    }

    #[test]
    fn a_half_download_is_not_counted() {
        type Break = fn(&Path, &Path);
        let breaks: [(&str, Break); 9] = [
            ("one weights file has not arrived", |snapshot, _| not_arrived(&snapshot.join(shard(2, 2)))),
            ("no weights file has arrived", |snapshot, _| {
                not_arrived(&snapshot.join(shard(1, 2)));
                not_arrived(&snapshot.join(shard(2, 2)));
            }),
            ("config.json has not arrived", |snapshot, _| not_arrived(&snapshot.join("config.json"))),
            ("tokenizer_config.json has not arrived", |snapshot, _| not_arrived(&snapshot.join("tokenizer_config.json"))),
            ("the list of weights files has not arrived", |snapshot, _| not_arrived(&snapshot.join(WEIGHTS_INDEX))),
            ("the list of weights files names none", |snapshot, _| fs::write(snapshot.join(WEIGHTS_INDEX), index_naming(&[])).unwrap()),
            ("the list of weights files cannot be read", |snapshot, _| fs::write(snapshot.join(WEIGHTS_INDEX), "{ cut off").unwrap()),
            ("nothing says which download is current", |_, folder| not_arrived(&folder.join("refs").join("main"))),
            ("the current download is not there", |_, folder| {
                fs::write(folder.join("refs").join("main"), "fedcba9876543210fedcba9876543210fedcba98").unwrap()
            }),
        ];

        for (what, break_it) in breaks {
            let hub = scratch_hub();
            let snapshot = download(&hub, PHI, 2);
            assert_eq!(names(&models_in(&hub)), ["Phi-4"], "whole before: {what}");

            break_it(&snapshot, &model_folder(&hub, PHI));
            assert_eq!(models_in(&hub), Vec::new(), "{what}");
        }
    }

    #[test]
    fn a_whole_model_beside_a_half_one_is_still_found() {
        let hub = scratch_hub();
        download(&hub, PHI, 2);
        let half = download(&hub, GPT_OSS, 3);
        not_arrived(&half.join(shard(3, 3)));
        assert_eq!(names(&models_in(&hub)), ["Phi-4"]);
    }

    #[test]
    fn a_name_that_steps_out_of_the_models_folder_is_not_followed() {
        assert!(plain_name(COMMIT));
        assert!(plain_name("model.safetensors"));
        for name in ["", ".", "..", "../elsewhere", "a/b", "/etc/hosts"] {
            assert!(!plain_name(name), "{name:?}");
        }

        // The download named is a real one, in another model's folder.
        let hub = scratch_hub();
        download(&hub, GPT_OSS, 1);
        let folder = model_folder(&hub, PHI);
        fs::create_dir_all(folder.join("refs")).unwrap();
        fs::create_dir_all(folder.join("snapshots")).unwrap();
        let elsewhere = format!("../../{MODEL_FOLDER}mlx-community--gpt-oss-20b-MXFP4-Q8/snapshots/{COMMIT}");
        assert!(is_file(&folder.join("snapshots").join(&elsewhere).join("config.json")), "the other download is there");
        fs::write(folder.join("refs").join("main"), &elsewhere).unwrap();
        assert_eq!(names(&models_in(&hub)), ["gpt-oss 20B"]);

        // The weights file named is a real one, outside the download.
        let hub = scratch_hub();
        let snapshot = download(&hub, PHI, 1);
        fs::write(snapshot.join("..").join("outside.safetensors"), "weights").unwrap();
        fs::write(snapshot.join(WEIGHTS_INDEX), index_naming(&["../outside.safetensors".to_string()])).unwrap();
        assert_eq!(models_in(&hub), Vec::new());
    }

    #[cfg(unix)]
    #[test]
    fn links_are_followed_and_a_link_to_nothing_is_not_a_file() {
        use std::os::unix::fs::symlink;

        // A download keeps each file once, in `blobs`, and the model's folder
        // holds links to them.
        let hub = scratch_hub();
        let snapshot = download(&hub, PHI, 2);
        let blobs = model_folder(&hub, PHI).join("blobs");
        fs::create_dir_all(&blobs).unwrap();
        let mut linked = Vec::new();
        for (number, entry) in fs::read_dir(&snapshot).unwrap().flatten().enumerate() {
            let blob = blobs.join(format!("blob-{number}"));
            fs::rename(entry.path(), &blob).unwrap();
            symlink(Path::new("../../blobs").join(format!("blob-{number}")), entry.path()).unwrap();
            linked.push((entry.file_name(), blob));
        }
        assert_eq!(linked.len(), 5, "three files the engine asks for, and two weights files");
        assert_eq!(names(&models_in(&hub)), ["Phi-4"]);

        let (_, blob) = linked.iter().find(|(name, _)| name.to_str() == Some(shard(2, 2).as_str())).unwrap();
        not_arrived(blob);
        assert_eq!(models_in(&hub), Vec::new());
    }

    #[test]
    fn no_folder_means_no_models() {
        let nowhere = scratch().join("never-made");
        assert_eq!(models_in(&nowhere), Vec::new());
        assert!(!nowhere.exists(), "looking does not make the folder");

        let a_file = scratch().join("hub");
        fs::write(&a_file, "not a folder").unwrap();
        assert_eq!(models_in(&a_file), Vec::new());

        assert_eq!(models_in(&scratch_hub()), Vec::new());
    }

    #[test]
    fn looking_changes_nothing_on_the_disk() {
        let hub = scratch_hub();
        download(&hub, PHI, 2);
        let half = download(&hub, GPT_OSS, 3);
        not_arrived(&half.join(shard(1, 3)));
        download(&hub, "mlx-community/Qwen3-30B-A3B-4bit", 1);

        let before = everything_in(&hub);
        assert_eq!(names(&models_in(&hub)), ["Phi-4"]);
        assert_eq!(names(&models_in(&hub)), ["Phi-4"], "asked again, the same answer");
        assert_eq!(everything_in(&hub), before);
    }

    #[test]
    fn the_folder_is_found_the_way_the_engine_finds_it() {
        let root = scratch();
        let home = root.join("home");
        let at = |name: &str| root.join(name).to_str().unwrap().to_string();
        let with = |pairs: &[(&str, String)]| {
            let set: HashMap<String, String> = pairs.iter().map(|(name, value)| (name.to_string(), value.clone())).collect();
            hub_folder(|name| set.get(name).cloned(), Some(&home))
        };

        // Nothing set: the cache in the home folder.
        assert_eq!(with(&[]), Some(home.join(".cache").join("huggingface").join("hub")));

        // Each variable wins over the ones after it.
        let mut set = vec![("XDG_CACHE_HOME", at("xdg"))];
        assert_eq!(with(&set), Some(root.join("xdg").join("huggingface").join("hub")));
        set.push(("HF_HOME", at("hf-home")));
        assert_eq!(with(&set), Some(root.join("hf-home").join("hub")));
        set.push(("HUGGINGFACE_HUB_CACHE", at("older-name")));
        assert_eq!(with(&set), Some(root.join("older-name")));
        set.push(("HF_HUB_CACHE", at("hub-cache")));
        assert_eq!(with(&set), Some(root.join("hub-cache")));

        // A leading ~ is the home folder.
        assert_eq!(with(&[("HF_HOME", "~/models".to_string())]), Some(home.join("models").join("hub")));
        assert_eq!(with(&[("HF_HUB_CACHE", "~".to_string())]), Some(home.clone()));

        // A variable set to nothing is not set.
        assert_eq!(with(&[("HF_HUB_CACHE", "  ".to_string()), ("HF_HOME", at("hf-home"))]), Some(root.join("hf-home").join("hub")));

        // A folder that is not a full path is no answer, and neither is no home.
        assert_eq!(with(&[("HF_HUB_CACHE", "models/hub".to_string())]), None);
        assert_eq!(hub_folder(|_| None, None), None);
        assert_eq!(hub_folder(|name| (name == "HF_HOME").then(|| "~/models".to_string()), None), None);
    }

    #[test]
    fn the_page_is_told_under_the_name_it_reads() {
        let hub = scratch_hub();
        download(&hub, PHI, 2);
        let status = EngineStatus {
            name: "MLX".to_string(),
            url: engine::DEFAULT_BASE_URL.to_string(),
            reachable: false,
            models: Vec::new(),
            set_aside: Vec::new(),
            hidden: 0,
            on_disk: models_in(&hub),
            offers: Vec::new(),
            error: Some("The engine is not answering.".to_string()),
            start_hint: "mlx_lm.server --port 8080".to_string(),
        };
        let told = serde_json::to_value(&status).unwrap();
        assert_eq!(told["onDisk"], serde_json::json!([{ "id": PHI, "name": "Phi-4", "maker": "Microsoft", "context": 16_384 }]));
        assert_eq!(told["models"], serde_json::json!([]));
    }

    /// Needs one of the app's models on this computer: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_on_this_computer() {
        let found = on_disk();
        println!("On this computer: {:?}", names(&found));
        assert!(!found.is_empty(), "none of the app's models is in the cache folder");
        let ids: Vec<String> = found.iter().map(|model| model.id.clone()).collect();
        assert_eq!(engine::allowed_models(&ids), found, "every model found is on the app's list");
    }
}
