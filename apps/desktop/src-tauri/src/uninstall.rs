//! Uninstalling a model: its files go to the Trash.
//!
//! Nothing here deletes. A model's folder, or its one GGUF file, is moved to
//! the system's Trash (the Recycle Bin on Windows), where the person can take
//! it back. Only a model on the app's list is moved, only from the folder the
//! engine reads, and only that model's own folder or file: a name that steps
//! out of the folder is refused, and nothing else in it is touched.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::deactivated::Deactivated;
use crate::downloads::Downloads;
use crate::engine;
use crate::models::{self, Layout};

/// Where a model goes, as the person's system names it.
const TRASH: &str = if cfg!(windows) {
    "the Recycle Bin"
} else {
    "the Trash"
};

/// The model's name, and its folder or file in `folder`. A model that is not
/// on the app's list, or not in `folder`, has none.
fn place_of(folder: &Path, layout: Layout, id: &str) -> Result<(String, PathBuf), String> {
    let Some(model) = engine::allowed_models(&[id.to_string()]).pop() else {
        return Err("That is not a model the app knows.".to_string());
    };
    let mut names = vec![layout.folder_name(id)];
    // `llama-server` also reads a GGUF file on its own, named by the file.
    if layout == Layout::Gguf {
        names.push(format!("{id}.gguf"));
    }
    names
        .into_iter()
        .filter(|name| models::plain_name(name))
        .map(|name| folder.join(name))
        .find(|place| fs::symlink_metadata(place).is_ok())
        .map(|place| (model.name.clone(), place))
        .ok_or_else(|| format!("{} is not on this computer.", model.name))
}

/// Moves a model's folder or file in `folder` with `to_trash`, and answers
/// with the model's name. Nothing is moved when the model is not there.
fn uninstall(
    folder: &Path,
    layout: Layout,
    id: &str,
    to_trash: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<String, String> {
    let (name, place) = place_of(folder, layout, id)?;
    to_trash(&place).map_err(|error| {
        format!(
            "{name} could not be moved to {TRASH}: {}.",
            error.trim_end_matches('.')
        )
    })?;
    Ok(name)
}

/// Moves a file or folder to the system's Trash. On a Mac it asks the file
/// manager, not Finder, so the person is not asked to let the app control
/// Finder.
fn to_trash(place: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let context = {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        let mut context = trash::TrashContext::default();
        context.set_delete_method(DeleteMethod::NsFileManager);
        context
    };
    #[cfg(not(target_os = "macos"))]
    let context = trash::TrashContext::default();
    context.delete(place).map_err(|error| error.to_string())
}

/// Where an uninstalled model goes, for the question the page asks first.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    /// "the Trash", or "the Recycle Bin" on Windows.
    trash: &'static str,
    /// True where the models sit in the Hugging Face cache, which other
    /// programs on the computer read too.
    shared_cache: bool,
}

/// Where an uninstalled model goes on this computer.
#[tauri::command]
pub fn uninstall_place() -> Place {
    Place {
        trash: TRASH,
        shared_cache: Layout::here() == Layout::Hub,
    }
}

/// Moves a model to the Trash, and answers with its name. A model switched
/// off is switched on again, so it is not kept off if it comes back. Refused
/// while a download is running.
#[tauri::command]
pub async fn uninstall_model(
    id: String,
    downloads: tauri::State<'_, Downloads>,
    deactivated: tauri::State<'_, Deactivated>,
) -> Result<String, String> {
    if engine::base_url().is_ok_and(|base| engine::is_remote(&base)) {
        return Err("Uninstall LAN models on the server that stores them.".to_string());
    }
    if downloads.running() {
        return Err(
            "A model is downloading. Uninstall once it is done, or cancel it first.".to_string(),
        );
    }
    let folder = models::models_folder()
        .ok_or_else(|| "The app cannot find the folder its models are in.".to_string())?;
    let layout = Layout::here();
    let moving = id.clone();
    let name =
        tauri::async_runtime::spawn_blocking(move || uninstall(&folder, layout, &moving, to_trash))
            .await
            .map_err(|_| format!("The model could not be moved to {TRASH}."))??;
    let _ = deactivated.set(&id, true);
    // `llama-server` reads its folder when it starts, so it is asked to read
    // it again. On a Mac, `mlx_lm.server` looks in the cache each time.
    if layout == Layout::Gguf
        && let Ok(base) = engine::base_url()
    {
        engine::reload_models(&base).await;
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    // Brad, 2026-11-19: "next story, I should be able to deactivate and
    // uninstall a model. I can't do that currently from what I can see."
    //
    // Every test builds its own models folder in the system's temporary
    // folder, and moves into a "trash" folder beside it. None of them uses
    // the real Trash or the real Hugging Face cache.
    use super::*;

    const PHI: &str = "mlx-community/phi-4-4bit";
    const GPT_OSS: &str = "mlx-community/gpt-oss-20b-MXFP4-Q8";

    fn scratch() -> PathBuf {
        let folder = std::env::temp_dir().join(format!("rusty-uninstall-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(folder.join("models")).unwrap();
        fs::create_dir_all(folder.join("trash")).unwrap();
        folder
    }

    /// A mover like the Trash, into `trash` beside the models folder.
    fn into(trash: PathBuf) -> impl FnOnce(&Path) -> Result<(), String> {
        move |place: &Path| {
            fs::rename(place, trash.join(place.file_name().unwrap()))
                .map_err(|error| error.to_string())
        }
    }

    fn put(folder: &Path, files: &[&str]) {
        for file in files {
            let path = folder.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "weights").unwrap();
        }
    }

    fn listing(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn uninstall_moves_only_that_model_out_of_the_cache() {
        let place = scratch();
        let models = place.join("models");
        put(
            &models,
            &[
                "models--mlx-community--phi-4-4bit/refs/main",
                "models--mlx-community--phi-4-4bit/snapshots/abc/config.json",
                "models--mlx-community--gpt-oss-20b-MXFP4-Q8/refs/main",
                "models--someone--another-program/refs/main",
            ],
        );
        let name = uninstall(&models, Layout::Hub, PHI, into(place.join("trash"))).unwrap();
        assert_eq!(name, "Phi-4");
        assert_eq!(
            listing(&models),
            [
                "models--mlx-community--gpt-oss-20b-MXFP4-Q8",
                "models--someone--another-program"
            ]
        );
        assert_eq!(
            listing(&place.join("trash")),
            ["models--mlx-community--phi-4-4bit"]
        );
        // Its files went with it, whole.
        assert!(
            place
                .join("trash/models--mlx-community--phi-4-4bit/snapshots/abc/config.json")
                .is_file()
        );
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn uninstall_moves_a_gguf_folder_or_a_gguf_file_on_its_own() {
        let place = scratch();
        let models = place.join("models");
        put(
            &models,
            &[
                "ggml-org--gpt-oss-20b-GGUF/gpt-oss-20b-mxfp4.gguf",
                "phi-4-Q4_K_M.gguf",
                "phi-4-Q8_0.gguf",
            ],
        );
        let trash = place.join("trash");
        assert_eq!(
            uninstall(
                &models,
                Layout::Gguf,
                "ggml-org/gpt-oss-20b-GGUF",
                into(trash.clone())
            ),
            Ok("gpt-oss 20B".to_string())
        );
        assert_eq!(
            uninstall(&models, Layout::Gguf, "phi-4-Q4_K_M", into(trash.clone())),
            Ok("Phi-4".to_string())
        );
        assert_eq!(listing(&models), ["phi-4-Q8_0.gguf"]);
        assert_eq!(
            listing(&trash),
            ["ggml-org--gpt-oss-20b-GGUF", "phi-4-Q4_K_M.gguf"]
        );
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn a_model_not_on_the_list_or_not_here_is_never_moved() {
        let place = scratch();
        let models = place.join("models");
        put(
            &models,
            &[
                "models--someone--a-model/refs/main",
                "a-model.gguf",
                "phi-4.gguf",
            ],
        );
        fs::write(place.join("phi-4"), "outside the models folder").unwrap();
        let before = listing(&models);
        for (layout, id, refusal) in [
            (
                Layout::Hub,
                "someone/a-model",
                "That is not a model the app knows.",
            ),
            (
                Layout::Gguf,
                "a-model",
                "That is not a model the app knows.",
            ),
            (Layout::Hub, GPT_OSS, "gpt-oss 20B is not on this computer."),
            (Layout::Gguf, "../phi-4", "Phi-4 is not on this computer."),
            (
                Layout::Gguf,
                "x/../../phi-4",
                "Phi-4 is not on this computer.",
            ),
            (Layout::Hub, "", "That is not a model the app knows."),
        ] {
            let result = uninstall(&models, layout, id, |_: &Path| -> Result<(), String> {
                panic!("{id} was moved")
            });
            assert_eq!(result, Err(refusal.to_string()), "{id}");
        }
        assert_eq!(listing(&models), before);
        assert!(place.join("phi-4").is_file());
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn a_model_the_trash_refuses_stays_where_it_was() {
        let place = scratch();
        let models = place.join("models");
        put(&models, &["models--mlx-community--phi-4-4bit/refs/main"]);
        let result = uninstall(&models, Layout::Hub, PHI, |_: &Path| {
            Err("The disk is read only.".to_string())
        });
        assert_eq!(
            result,
            Err(format!(
                "Phi-4 could not be moved to {TRASH}: The disk is read only."
            ))
        );
        assert_eq!(listing(&models), ["models--mlx-community--phi-4-4bit"]);
        fs::remove_dir_all(&place).unwrap();
    }
}
