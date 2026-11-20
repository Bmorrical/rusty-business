//! The model makers the person uses.
//!
//! The app's list holds models from several makers. The person picks one or
//! more of them on the first run, with Microsoft picked to begin with, and
//! can add or remove one in Settings with one click. The engine's models,
//! the models on this computer, and the downloads offered are shown only for
//! the makers picked. Nothing is removed from the disk when a maker is.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;

use crate::downloads::Offer;
use crate::engine::{self, ModelInfo};

/// The maker picked when nothing else is.
pub const FIRST: &str = "Microsoft";

/// The makers the person picked, kept in the app's own folder, one to a line.
#[derive(Clone)]
pub struct Makers {
    file: PathBuf,
    picked: Arc<Mutex<Vec<String>>>,
}

/// The makers on the app's list among `names`, in the list's order, each once.
fn on_the_list<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let wanted: Vec<&str> = names.into_iter().map(str::trim).collect();
    engine::makers()
        .into_iter()
        .filter(|maker| wanted.contains(maker))
        .map(str::to_string)
        .collect()
}

impl Makers {
    /// The makers as the person last left them. With none kept, Microsoft,
    /// and the maker of every model of the app's already on this computer,
    /// so a model downloaded before makers could be picked stays in sight.
    pub fn new(file: PathBuf, on_disk: &[ModelInfo]) -> Self {
        let kept = fs::read_to_string(&file)
            .map(|text| on_the_list(text.lines()))
            .unwrap_or_default();
        let picked = if kept.is_empty() {
            on_the_list(
                std::iter::once(FIRST).chain(on_disk.iter().map(|model| model.maker.as_str())),
            )
        } else {
            kept
        };
        Self {
            file,
            picked: Arc::new(Mutex::new(picked)),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        self.picked
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn picked(&self) -> Vec<String> {
        self.lock().clone()
    }

    /// Picks these makers and keeps them for the next time the app opens.
    /// At least one is picked, each is on the app's list, and nothing changes
    /// when they cannot be kept.
    pub fn set(&self, makers: &[String]) -> Result<Vec<String>, String> {
        if let Some(stranger) = makers
            .iter()
            .find(|maker| !engine::makers().contains(&maker.as_str()))
        {
            return Err(format!("{stranger} is not a maker on the app's list."));
        }
        let picked = on_the_list(makers.iter().map(String::as_str));
        if picked.is_empty() {
            return Err("Pick at least one maker.".to_string());
        }
        let mut current = self.lock();
        match self.file.parent() {
            Some(folder) => fs::create_dir_all(folder),
            None => Ok(()),
        }
        .and_then(|()| fs::write(&self.file, picked.join("\n") + "\n"))
        .map_err(|error| format!("The app could not keep the makers you picked: {error}."))?;
        current.clone_from(&picked);
        Ok(picked)
    }

    /// Leaves only the picked makers' models in `models`.
    pub fn keep_picked<T>(&self, models: &mut Vec<T>, maker_of: impl Fn(&T) -> &str) {
        let picked = self.lock();
        models.retain(|model| picked.iter().any(|maker| maker == maker_of(model)));
    }

    /// Every maker on the app's list, whether it is picked, and its models.
    pub fn status(&self, on_disk: &[ModelInfo], offers: &[Offer]) -> Vec<Maker> {
        let picked = self.picked();
        engine::makers()
            .into_iter()
            .map(|maker| Maker {
                name: maker.to_string(),
                picked: picked.iter().any(|name| name == maker),
                models: engine::models_by(maker)
                    .into_iter()
                    .map(|(name, license)| Listed {
                        name: name.to_string(),
                        license: license.to_string(),
                        on_disk: on_disk.iter().any(|model| model.name == name),
                        ids: on_disk
                            .iter()
                            .filter(|model| model.name == name)
                            .map(|model| model.id.clone())
                            .collect(),
                        offer: offers.iter().find(|offer| offer.name == name).cloned(),
                    })
                    .collect(),
            })
            .collect()
    }
}

/// A maker on the app's list, as the page shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Maker {
    pub name: String,
    pub picked: bool,
    pub models: Vec<Listed>,
}

/// One of a maker's models: its license, whether it is on this computer, the
/// ids it has there (the switch and Uninstall in Settings act on them), and
/// its download, when the app can download it here.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    pub name: String,
    pub license: String,
    pub on_disk: bool,
    pub ids: Vec<String>,
    pub offer: Option<Offer>,
}

/// Every maker on the app's list, and which are picked.
#[tauri::command]
pub fn makers_status(makers: tauri::State<'_, Makers>) -> Vec<Maker> {
    inventory(&makers)
}

/// Picks these makers, and answers as `makers_status` does.
#[tauri::command]
pub fn set_makers(
    picked: Vec<String>,
    makers: tauri::State<'_, Makers>,
) -> Result<Vec<Maker>, String> {
    makers.set(&picked)?;
    Ok(inventory(&makers))
}

fn inventory(makers: &Makers) -> Vec<Maker> {
    if engine::base_url().is_ok_and(|base| engine::is_remote(&base)) {
        makers.status(&[], &crate::downloads::remote_offers())
    } else {
        makers.status(&crate::models::on_disk(), &crate::downloads::offers())
    }
}

#[cfg(test)]
mod tests {
    // Brad, 2026-11-17: "user can pick google, gemini, meta from the app if
    // they want those instead. I think the user can pick one or more at
    // install with microsoft default, and in settings they can one click more
    // of them if they want."
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("rusty-makers-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    fn model(name: &str, maker: &str) -> ModelInfo {
        ModelInfo {
            id: format!("someone/{name}"),
            name: name.to_string(),
            maker: maker.to_string(),
            context: 8,
        }
    }

    fn picked(makers: &[&str]) -> Vec<String> {
        makers.iter().map(|maker| maker.to_string()).collect()
    }

    #[test]
    fn microsoft_is_picked_to_begin_with() {
        let place = folder("first");
        assert_eq!(
            Makers::new(place.join("makers"), &[]).picked(),
            ["Microsoft"]
        );
        fs::create_dir_all(&place).unwrap();
        // Nothing on the list kept reads as nothing kept.
        for kept in ["", "\n", "Acme\n", "microsoft\n"] {
            fs::write(place.join("makers"), kept).unwrap();
            assert_eq!(
                Makers::new(place.join("makers"), &[]).picked(),
                ["Microsoft"],
                "{kept:?}"
            );
        }
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn a_model_already_on_this_computer_keeps_its_maker_picked() {
        let place = folder("upgrade");
        let on_disk = [
            model("gpt-oss 20B", "OpenAI"),
            model("Nemotron 3.5 Lightning", "NVIDIA"),
        ];
        assert_eq!(
            Makers::new(place.join("makers"), &on_disk).picked(),
            ["Microsoft", "OpenAI", "NVIDIA"]
        );
        // Once makers are kept, they are what counts.
        let makers = Makers::new(place.join("makers"), &on_disk);
        makers.set(&picked(&["Google"])).unwrap();
        assert_eq!(
            Makers::new(place.join("makers"), &on_disk).picked(),
            ["Google"]
        );
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn the_makers_picked_stay_as_they_were_left() {
        let place = folder("kept");
        let makers = Makers::new(place.join("app").join("makers"), &[]);
        assert_eq!(
            makers.set(&picked(&["Meta", "Microsoft", "Google", "Meta"])),
            Ok(picked(&["Microsoft", "Google", "Meta"]))
        );
        assert_eq!(makers.picked(), ["Microsoft", "Google", "Meta"]);
        assert_eq!(
            Makers::new(place.join("app").join("makers"), &[]).picked(),
            ["Microsoft", "Google", "Meta"]
        );
        // One click takes one away.
        makers.set(&picked(&["Microsoft", "Meta"])).unwrap();
        assert_eq!(
            Makers::new(place.join("app").join("makers"), &[]).picked(),
            ["Microsoft", "Meta"]
        );
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn at_least_one_maker_on_the_list_is_picked() {
        let place = folder("refused");
        let makers = Makers::new(place.join("makers"), &[]);
        makers.set(&picked(&["Google"])).unwrap();
        assert_eq!(makers.set(&[]), Err("Pick at least one maker.".to_string()));
        assert_eq!(
            makers.set(&picked(&["Google", "Acme"])),
            Err("Acme is not a maker on the app's list.".to_string())
        );
        assert_eq!(
            makers.set(&picked(&["google"])),
            Err("google is not a maker on the app's list.".to_string())
        );
        assert_eq!(
            makers.picked(),
            ["Google"],
            "a refused pick changed the makers"
        );
        assert_eq!(Makers::new(place.join("makers"), &[]).picked(), ["Google"]);
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn makers_that_cannot_be_kept_are_not_changed() {
        let place = folder("unkept");
        fs::create_dir_all(&place).unwrap();
        // A file where the app's folder should be: nothing can be kept.
        fs::write(place.join("app"), "").unwrap();
        let makers = Makers::new(place.join("app").join("makers"), &[]);
        let error = makers.set(&picked(&["Google"])).unwrap_err();
        assert!(
            error.starts_with("The app could not keep the makers you picked: "),
            "{error}"
        );
        assert_eq!(makers.picked(), ["Microsoft"]);
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn only_the_picked_makers_models_are_shown() {
        let place = folder("shown");
        let makers = Makers::new(place.join("makers"), &[]);
        makers.set(&picked(&["Microsoft", "Meta"])).unwrap();
        let mut models = vec![
            model("Phi-4", "Microsoft"),
            model("Gemma 4 26B", "Google"),
            model("Llama 3.1 8B", "Meta"),
        ];
        makers.keep_picked(&mut models, |model| &model.maker);
        let names: Vec<&str> = models.iter().map(|model| model.name.as_str()).collect();
        assert_eq!(names, ["Phi-4", "Llama 3.1 8B"]);
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn every_maker_is_listed_with_its_models_licenses_and_downloads() {
        let place = folder("status");
        let makers = Makers::new(place.join("makers"), &[]);
        makers.set(&picked(&["Microsoft", "Google"])).unwrap();
        // Two copies of one model: the row acts on both.
        let mut eight_bit = model("Phi-4", "Microsoft");
        eight_bit.id = "someone/Phi-4-8bit".to_string();
        let on_disk = [model("Phi-4", "Microsoft"), eight_bit];
        let gemma = Offer {
            id: "someone/gemma".to_string(),
            name: "Gemma 4 26B".to_string(),
            maker: "Google".to_string(),
            license: "Apache 2.0".to_string(),
            bytes: 9,
        };
        let status = makers.status(&on_disk, std::slice::from_ref(&gemma));
        let names: Vec<(&str, bool)> = status
            .iter()
            .map(|maker| (maker.name.as_str(), maker.picked))
            .collect();
        assert_eq!(
            names,
            [
                ("Microsoft", true),
                ("OpenAI", false),
                ("NVIDIA", false),
                ("Google", true),
                ("Meta", false)
            ]
        );
        let microsoft = &status[0].models[0];
        assert_eq!(
            (
                microsoft.name.as_str(),
                microsoft.license.as_str(),
                microsoft.on_disk,
                &microsoft.offer
            ),
            ("Phi-4", "MIT", true, &None)
        );
        assert_eq!(microsoft.ids, ["someone/Phi-4", "someone/Phi-4-8bit"]);
        let google = &status[3].models[0];
        assert_eq!(
            (
                google.name.as_str(),
                google.license.as_str(),
                google.on_disk,
                &google.offer
            ),
            ("Gemma 4 26B", "Apache 2.0", false, &Some(gemma))
        );
        assert!(google.ids.is_empty());
        assert_eq!(
            serde_json::to_value(&status[4]).unwrap(),
            serde_json::json!({ "name": "Meta", "picked": false, "models": [{ "name": "Llama 3.1 8B", "license": "Llama 3.1 Community License", "onDisk": false, "ids": [], "offer": null }] })
        );
        fs::remove_dir_all(&place).unwrap();
    }
}
