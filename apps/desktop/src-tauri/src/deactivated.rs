//! The models the person switched off.
//!
//! A model switched off stays where it is, on this computer or with its
//! maker, but the page leaves it out of the models a question can go to, out
//! of the grader's place, and out of every comparison. One switch in
//! Settings turns it back on. The ids are kept in the app's own folder, one
//! to a line, and only ids of models the app could offer are kept.

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use crate::{engine, outside};

/// The longest id kept. The app's own ids and its outside engines' ids are
/// far shorter.
const LONGEST: usize = 200;

/// The models switched off, kept in the app's own folder, one to a line.
pub struct Deactivated {
    file: PathBuf,
    off: Mutex<Vec<String>>,
}

/// True for the id of a model the app could offer: one on its own list, or
/// one of an outside engine's.
fn known(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= LONGEST
        && id.trim() == id
        && !id.chars().any(char::is_control)
        && (outside::is_outside(id) || !engine::allowed_models(&[id.to_string()]).is_empty())
}

impl Deactivated {
    /// The models as the person last left them. A file that cannot be read
    /// keeps none off, and a line that names no model the app knows is
    /// passed over.
    pub fn new(file: PathBuf) -> Self {
        let mut off: Vec<String> = Vec::new();
        for id in fs::read_to_string(&file).unwrap_or_default().lines() {
            if known(id) && !off.iter().any(|kept| kept == id) {
                off.push(id.to_string());
            }
        }
        Self {
            file,
            off: Mutex::new(off),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        self.off
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The ids of the models switched off, in the order they were.
    pub fn ids(&self) -> Vec<String> {
        self.lock().clone()
    }

    /// Switches a model on or off, and keeps that for the next time the app
    /// opens. Nothing changes when it cannot be kept.
    pub fn set(&self, id: &str, active: bool) -> Result<Vec<String>, String> {
        if !known(id) {
            return Err("That is not a model the app knows.".to_string());
        }
        let mut off = self.lock();
        // Already as asked: nothing to keep.
        if off.iter().any(|kept| kept == id) != active {
            return Ok(off.clone());
        }
        let mut next = off.clone();
        if active {
            next.retain(|kept| kept != id);
        } else {
            next.push(id.to_string());
        }
        let text: String = next.iter().map(|kept| format!("{kept}\n")).collect();
        match self.file.parent() {
            Some(folder) => fs::create_dir_all(folder),
            None => Ok(()),
        }
        .and_then(|()| fs::write(&self.file, text))
        .map_err(|error| format!("The app could not keep the models switched off: {error}."))?;
        off.clone_from(&next);
        Ok(next)
    }
}

/// The ids of the models switched off.
#[tauri::command]
pub fn deactivated_models(deactivated: tauri::State<'_, Deactivated>) -> Vec<String> {
    deactivated.ids()
}

/// Switches a model on or off, and answers with the models switched off.
#[tauri::command]
pub fn set_model_active(
    id: String,
    active: bool,
    deactivated: tauri::State<'_, Deactivated>,
) -> Result<Vec<String>, String> {
    deactivated.set(&id, active)
}

#[cfg(test)]
mod tests {
    // Brad, 2026-11-19: "next story, I should be able to deactivate and
    // uninstall a model. I can't do that currently from what I can see."
    use super::*;

    const PHI: &str = "mlx-community/phi-4-4bit";
    const GPT_OSS: &str = "mlx-community/gpt-oss-20b-MXFP4-Q8";
    const GROK: &str = "xai:grok-4";

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "rusty-deactivated-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn a_model_switched_off_stays_off_until_it_is_switched_on() {
        let place = folder("kept");
        let file = place.join("app").join("deactivated");
        let off = Deactivated::new(file.clone());
        assert!(off.ids().is_empty(), "every model starts on");

        assert_eq!(off.set(PHI, false).unwrap(), [PHI]);
        assert_eq!(off.set(GROK, false).unwrap(), [PHI, GROK]);
        // Switching off a model already off changes nothing.
        assert_eq!(off.set(PHI, false).unwrap(), [PHI, GROK]);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            format!("{PHI}\n{GROK}\n")
        );

        // The next time the app opens, they are still off.
        let reopened = Deactivated::new(file.clone());
        assert_eq!(reopened.ids(), [PHI, GROK]);

        assert_eq!(reopened.set(PHI, true).unwrap(), [GROK]);
        assert_eq!(reopened.set(GPT_OSS, true).unwrap(), [GROK]);
        assert_eq!(Deactivated::new(file).ids(), [GROK]);
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn only_models_the_app_knows_are_kept_off() {
        let place = folder("known");
        fs::create_dir_all(&place).unwrap();
        let file = place.join("deactivated");
        fs::write(
            &file,
            format!("\n{PHI}\nsomeone/a-model-not-on-the-list\n {GPT_OSS}\n{PHI}\n{GROK}\n"),
        )
        .unwrap();
        let off = Deactivated::new(file.clone());
        assert_eq!(
            off.ids(),
            [PHI, GROK],
            "a stranger, a padded line, and a repeat are passed over"
        );

        let long = format!("mlx-community/phi-4-{}", "x".repeat(LONGEST));
        for stranger in [
            "",
            "someone/a-model-not-on-the-list",
            " mlx-community/phi-4-4bit",
            "mlx-community/phi-4-4bit\nsomeone/else",
            long.as_str(),
        ] {
            assert!(off.set(stranger, false).is_err(), "{stranger:?}");
        }
        assert_eq!(off.ids(), [PHI, GROK]);
        assert_eq!(Deactivated::new(file).ids(), [PHI, GROK]);
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn models_that_cannot_be_kept_off_are_left_as_they_were() {
        let place = folder("unkept");
        fs::create_dir_all(&place).unwrap();
        // A file where the folder should be, so nothing can be written.
        fs::write(place.join("app"), "").unwrap();
        let off = Deactivated::new(place.join("app").join("deactivated"));
        assert!(off.set(PHI, false).is_err());
        assert!(off.ids().is_empty());
        fs::remove_dir_all(&place).unwrap();
    }
}
