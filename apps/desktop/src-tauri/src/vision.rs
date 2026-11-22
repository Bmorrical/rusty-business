//! Models that read pictures, on a Mac for now.
//!
//! Brad, 2026-11-22: "allow all 3 models, user can on click to install, start
//! a grouping, maybe tabbed screens, for each "type" collections of models,
//! they can exists in more than one tab where applicable."
//!
//! A model that reads pictures runs on its own engine, `mlx_vlm.server`, on
//! this computer only, at `PICTURE_BASE_URL`. The app starts it the way it
//! starts the chat engine, with no model named and nothing downloaded. A
//! picture goes in a chat message as a `data:` URL, and only ever to a model
//! on this computer that reads pictures: an engine outside this computer is
//! never sent one, and neither is a model that does not read them.

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::engine::{self, ChatMessage, ModelInfo};
use crate::starter::{self, Engine, Found, StartReport, Starter};

/// The picture engine's address. It is on this computer, always: there is no
/// setting that moves it.
pub const PICTURE_BASE_URL: &str = "http://127.0.0.1:8081/v1";

/// The picture engine's program.
pub const PROGRAM: &str = "mlx_vlm.server";

/// How to put the picture engine on a Mac, shown when it is not there.
pub const START_HINT: &str = "pip install mlx-vlm";

/// Said when a picture would leave this computer.
pub const STAYS_HERE: &str =
    "Pictures go only to models on this computer that read them, so nothing was sent.";

/// Said when a model that does not read pictures is sent one.
pub const CANNOT_SEE: &str = "That model does not read pictures, so nothing was sent.";

/// Said on a system without the picture engine.
pub const MAC_ONLY: &str = "Models that read pictures need a Mac for now.";

/// The most characters one picture may take, as a `data:` URL. The page
/// shrinks each picture well under this before it is sent.
pub const PICTURE_LIMIT: usize = 8_000_000;

/// The most pictures one message may carry.
pub const PICTURES_PER_MESSAGE: usize = 4;

/// The largest picture file the app reads from the disk.
pub const FILE_LIMIT: u64 = 20_000_000;

/// The kinds of picture a model is sent, by their `data:` URL's start.
const KINDS: [&str; 3] = ["image/jpeg", "image/png", "image/webp"];

// ---------------------------------------------------------------------------
// The picture engine
// ---------------------------------------------------------------------------

/// The picture engine this app started, kept apart from the chat engine's.
pub struct PictureStarter(pub Starter);

/// What the page is told about the picture engine.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PictureEngine {
    /// The picture engine runs on this system.
    pub supported: bool,
    pub url: String,
    pub reachable: bool,
    /// The models it serves that are on the app's list and read pictures.
    pub models: Vec<ModelInfo>,
    /// The app's models that read pictures and are whole on the disk.
    pub on_disk: Vec<ModelInfo>,
    pub start_hint: String,
}

/// The app's models that read pictures, whole on the disk.
fn pictures_on_disk() -> Vec<ModelInfo> {
    crate::models::on_disk()
        .into_iter()
        .filter(|model| engine::reads_pictures(&model.id))
        .collect()
}

/// The models that read pictures, out of an engine's list of ids.
pub fn picture_models(ids: &[String]) -> Vec<ModelInfo> {
    engine::allowed_models(ids)
        .into_iter()
        .filter(|model| engine::reads_pictures(&model.id))
        .collect()
}

#[tauri::command]
pub async fn picture_engine_status() -> PictureEngine {
    let supported = cfg!(target_os = "macos");
    let mut status = PictureEngine {
        supported,
        url: PICTURE_BASE_URL.to_string(),
        reachable: false,
        models: Vec::new(),
        on_disk: Vec::new(),
        start_hint: START_HINT.to_string(),
    };
    if !supported {
        return status;
    }
    status.on_disk = pictures_on_disk();
    if let Ok(ids) = engine::fetch_model_ids(PICTURE_BASE_URL).await {
        status.reachable = true;
        status.models = picture_models(&ids);
    }
    status
}

/// What this computer has for starting the picture engine. A program named
/// for the chat engine is never taken for this one.
pub fn look() -> Found {
    let engine = cfg!(target_os = "macos").then_some(Engine::MlxVlm);
    let home = std::env::home_dir();
    let program = engine.as_ref().and_then(|_| {
        starter::find_program(
            PROGRAM,
            |key| {
                (key != starter::PROGRAM_VARIABLE)
                    .then(|| std::env::var(key).ok())
                    .flatten()
            },
            home.as_deref(),
            None,
        )
    });
    Found {
        engine,
        program,
        models: pictures_on_disk().len(),
    }
}

/// Starts the picture engine when none answers and a model that reads
/// pictures is on the disk. Safe to call at any time.
#[tauri::command]
pub async fn start_picture_engine(
    starter: tauri::State<'_, PictureStarter>,
) -> Result<StartReport, String> {
    if !cfg!(target_os = "macos") {
        return Ok(StartReport {
            answering: false,
            started: false,
            reason: Some(MAC_ONLY.to_string()),
        });
    }
    Ok(starter
        .0
        .bring_up(PICTURE_BASE_URL, &look(), starter::PATIENCE)
        .await)
}

// ---------------------------------------------------------------------------
// A picture from the disk
// ---------------------------------------------------------------------------

/// A picture read from the disk, for the page to shrink before it is sent.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Picture {
    pub name: String,
    pub url: String,
}

/// The kind of picture these bytes hold, by how they start.
pub fn picture_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// A picture's bytes as a `data:` URL, or why not.
pub fn picture_url(name: &str, bytes: &[u8]) -> Result<String, String> {
    let kind = picture_kind(bytes)
        .ok_or_else(|| format!("{name} is not a JPEG, PNG, or WebP picture."))?;
    Ok(format!("data:{kind};base64,{}", encode(bytes)))
}

/// Reads one picture for a chat. Nothing is written or kept here: the page
/// shrinks it and keeps it with the chat.
#[tauri::command]
pub fn read_picture(path: String) -> Result<Picture, String> {
    let path = Path::new(&path);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "The picture".to_string());
    let meta = fs::metadata(path).map_err(|_| format!("{name} could not be read."))?;
    if !meta.is_file() {
        return Err(format!("{name} is not a picture."));
    }
    if meta.len() > FILE_LIMIT {
        return Err(format!("{name} is over 20 MB, so it was left out."));
    }
    let bytes = fs::read(path).map_err(|_| format!("{name} could not be read."))?;
    Ok(Picture {
        url: picture_url(&name, &bytes)?,
        name,
    })
}

// ---------------------------------------------------------------------------
// Pictures in a chat
// ---------------------------------------------------------------------------

/// True when any message carries a picture.
pub fn has_pictures(messages: &[ChatMessage]) -> bool {
    messages.iter().any(|message| !message.images.is_empty())
}

/// Checks every picture in the messages, and that the model may be sent them.
pub fn checked_pictures(model: &str, messages: &[ChatMessage]) -> Result<(), String> {
    if !has_pictures(messages) {
        return Ok(());
    }
    if !engine::reads_pictures(model) {
        return Err(CANNOT_SEE.to_string());
    }
    for message in messages {
        if message.images.is_empty() {
            continue;
        }
        if message.role != "user" {
            return Err("Only a question can carry a picture.".to_string());
        }
        if message.images.len() > PICTURES_PER_MESSAGE {
            return Err(format!(
                "A question can carry at most {PICTURES_PER_MESSAGE} pictures."
            ));
        }
        for url in &message.images {
            checked_picture(url)?;
        }
    }
    Ok(())
}

/// One picture: a `data:` URL of a JPEG, PNG, or WebP picture, in base64,
/// under `PICTURE_LIMIT`, whose bytes are the kind it says.
pub fn checked_picture(url: &str) -> Result<(), String> {
    let refused = || "A picture is not a JPEG, PNG, or WebP picture.".to_string();
    if url.len() > PICTURE_LIMIT {
        return Err("A picture is too big to send.".to_string());
    }
    let (kind, data) = url
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(";base64,"))
        .ok_or_else(refused)?;
    if !KINDS.contains(&kind) {
        return Err(refused());
    }
    let bytes = decode(data).ok_or_else(refused)?;
    if picture_kind(&bytes) != Some(kind) {
        return Err(refused());
    }
    Ok(())
}

/// The messages as the engine is sent them. A message with pictures has its
/// words and its pictures as parts; any other is as it was.
pub fn body_messages(messages: &[ChatMessage]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|message| {
            if message.images.is_empty() {
                return serde_json::json!({ "role": message.role, "content": message.content });
            }
            let mut parts = vec![serde_json::json!({ "type": "text", "text": message.content })];
            parts.extend(message.images.iter().map(
                |url| serde_json::json!({ "type": "image_url", "image_url": { "url": url } }),
            ));
            serde_json::json!({ "role": message.role, "content": parts })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Base64, both ways
// ---------------------------------------------------------------------------

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn decode(text: &str) -> Option<Vec<u8>> {
    let text = text.as_bytes();
    if text.is_empty() || !text.len().is_multiple_of(4) {
        return None;
    }
    let value = |c: u8| ALPHABET.iter().position(|&a| a == c).map(|v| v as u32);
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let last = text.len() / 4 - 1;
    for (index, chunk) in text.chunks(4).enumerate() {
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && index != last) {
            return None;
        }
        let mut n = 0u32;
        for &c in &chunk[..4 - pad] {
            n = (n << 6) | value(c)?;
        }
        n <<= 6 * pad as u32;
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Brad, 2026-11-22: "allow all 3 models, user can on click to install,
    // start a grouping, maybe tabbed screens, for each "type" collections of
    // models, they can exists in more than one tab where applicable."

    /// A tiny drawn picture: a 2 by 2 PNG, made here in code.
    fn drawn_png() -> Vec<u8> {
        let mut bytes = Vec::new();
        let picture = image::RgbImage::from_fn(2, 2, |x, y| {
            image::Rgb([(x * 200) as u8, (y * 200) as u8, 90])
        });
        picture
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    fn asking(images: Vec<String>) -> Vec<ChatMessage> {
        vec![ChatMessage {
            role: "user".to_string(),
            content: "What is in this picture?".to_string(),
            images,
        }]
    }

    const PHI_VISION: &str = "mlx-community/Phi-3.5-vision-instruct-4bit";

    #[test]
    fn base64_goes_both_ways() {
        for bytes in [&b""[..], b"a", b"ab", b"abc", b"abcd", &drawn_png()] {
            if bytes.is_empty() {
                continue;
            }
            assert_eq!(decode(&encode(bytes)).as_deref(), Some(bytes));
        }
        assert_eq!(encode(b"Man"), "TWFu");
        assert_eq!(encode(b"Ma"), "TWE=");
        assert_eq!(encode(b"M"), "TQ==");
        assert_eq!(decode("TQ=="), Some(b"M".to_vec()));
        assert_eq!(decode("TQ=a"), None);
        assert_eq!(decode("T!=="), None);
        assert_eq!(decode("TQ==TWFu"), None);
    }

    #[test]
    fn phi_vision_reads_pictures_and_runs_on_its_own_engine() {
        assert!(engine::reads_pictures(PHI_VISION));
        assert!(!engine::reads_pictures("mlx-community/phi-4-4bit"));
        assert!(engine::reads_pictures_by_name("Phi-3.5 Vision"));
        assert!(!engine::reads_pictures_by_name("Phi-4"));
        let ids = vec![
            PHI_VISION.to_string(),
            "mlx-community/phi-4-4bit".to_string(),
        ];
        let seen: Vec<String> = picture_models(&ids).into_iter().map(|m| m.name).collect();
        assert_eq!(seen, vec!["Phi-3.5 Vision".to_string()]);
        assert!(PICTURE_BASE_URL.starts_with("http://127.0.0.1:"));
        assert!(!PICTURE_BASE_URL.contains(":8080"));
    }

    #[test]
    fn a_drawn_picture_is_read_and_sent_as_parts() {
        let url = picture_url("drawn.png", &drawn_png()).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!(checked_picture(&url), Ok(()));
        let messages = asking(vec![url.clone()]);
        assert_eq!(checked_pictures(PHI_VISION, &messages), Ok(()));
        let body = body_messages(&messages);
        assert_eq!(body[0]["content"][0]["type"], "text");
        assert_eq!(body[0]["content"][1]["image_url"]["url"], url.as_str());
        // A message with no picture is sent as plain words.
        let plain = body_messages(&asking(Vec::new()));
        assert_eq!(plain[0]["content"], "What is in this picture?");
    }

    #[test]
    fn a_model_that_does_not_read_pictures_is_never_sent_one() {
        let url = picture_url("drawn.png", &drawn_png()).unwrap();
        assert_eq!(
            checked_pictures("mlx-community/phi-4-4bit", &asking(vec![url])),
            Err(CANNOT_SEE.to_string())
        );
        assert_eq!(
            checked_pictures("mlx-community/phi-4-4bit", &asking(Vec::new())),
            Ok(())
        );
    }

    #[test]
    fn only_jpeg_png_and_webp_pass_and_each_must_be_what_it_says() {
        let png = encode(&drawn_png());
        assert!(checked_picture(&format!("data:image/gif;base64,{png}")).is_err());
        assert!(checked_picture(&format!("data:image/jpeg;base64,{png}")).is_err());
        assert!(checked_picture("https://example.com/a.png").is_err());
        assert!(checked_picture("data:image/png;base64,not base64").is_err());
        let huge = format!("data:image/png;base64,{}", "A".repeat(PICTURE_LIMIT));
        assert_eq!(
            checked_picture(&huge),
            Err("A picture is too big to send.".to_string())
        );
        assert_eq!(picture_kind(b"GIF89a"), None);
        assert_eq!(picture_kind(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(picture_kind(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
    }

    #[test]
    fn a_question_carries_at_most_four_pictures_and_answers_carry_none() {
        let url = picture_url("drawn.png", &drawn_png()).unwrap();
        let five = asking(vec![url.clone(); PICTURES_PER_MESSAGE + 1]);
        assert!(checked_pictures(PHI_VISION, &five).is_err());
        let mut answer = asking(vec![url]);
        answer[0].role = "assistant".to_string();
        assert!(checked_pictures(PHI_VISION, &answer).is_err());
    }

    #[test]
    fn a_picture_is_read_from_the_disk_and_nothing_else_is() {
        let folder = std::env::temp_dir().join(format!("rb-vision-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let png = folder.join("drawn.png");
        fs::write(&png, drawn_png()).unwrap();
        let read = read_picture(png.to_string_lossy().into_owned()).unwrap();
        assert_eq!(read.name, "drawn.png");
        assert!(read.url.starts_with("data:image/png;base64,"));
        let text = folder.join("notes.png");
        fs::write(&text, "not a picture").unwrap();
        assert!(read_picture(text.to_string_lossy().into_owned()).is_err());
        assert!(read_picture(folder.to_string_lossy().into_owned()).is_err());
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_picture_never_leaves_this_computer() {
        let code = include_str!("engine.rs");
        assert!(code.contains(
            "if outside::is_outside(&request.model) && vision::has_pictures(&request.messages)"
        ));
        assert!(code.contains("Err(vision::STAYS_HERE.to_string())"));
        assert!(code.contains("Ok(vision::PICTURE_BASE_URL.to_string())"));
    }

    #[test]
    fn the_picture_engine_is_started_on_this_computer_and_runs_no_models_own_code() {
        let code = include_str!("starter.rs");
        assert!(code.contains("Engine::MlxVlm => \"mlx_vlm.server\","));
        assert!(code.contains("Engine::Mlx | Engine::MlxVlm => command(program, address),"));
        assert!(starter::address_of(PICTURE_BASE_URL).is_ok());
        // The program is started with no flag that would run a model's own
        // code.
        assert!(!code.contains("trust-remote-code"));
    }
}
