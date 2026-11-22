//! The picture check: a small vision model that scores a picture on this
//! computer.
//!
//! Brad, 2026-11-22: "[Rusty Business™] can make a new pr to main with tests that
//! allows a one click install on marqo should the user choose, and uninstall
//! too".
//!
//! The model is Marqo's nsfw-image-detection-384, a 22 MB vision transformer.
//! It is never part of the app: the person installs it from the Installers
//! page, and Uninstall moves it to the Trash.
//!
//! Five rules live here:
//! - it is downloaded only when the person asks, through the WAN gate, from
//!   Hugging Face, at one pinned revision, and its size and SHA-256 are
//!   checked as it arrives and again each time it is read;
//! - it lives in the app's own folder, never in the models folder;
//! - a picture is read into memory and scored there: nothing about it is
//!   written anywhere, no copy, no hash, and no score;
//! - the score goes back to the page and nowhere else, never to a log, a
//!   saved chat, or a file;
//! - a score is a score, never a verdict. The page says the model can make
//!   mistakes, and a high score only shows the score.

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use candle_core::{D, DType, Device, Module, Tensor};
use candle_nn::{Conv2d, Conv2dConfig, LayerNorm, Linear, VarBuilder};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::ipc::Channel;

use crate::downloads::{self, DownloadReport, Downloads, Pinned, PinnedFile, Progress};
use crate::gate::Gate;
use crate::uninstall;

/// What the page calls it.
const NAME: &str = "Picture check";

/// The model on Hugging Face, and the revision the app was tried with.
const ID: &str = "Marqo/nsfw-image-detection-384";
const REVISION: &str = "0c26ec22111b83f106d72a55f611ec35962bcb65";
const WEIGHTS: &str = "model.safetensors";
const WEIGHTS_BYTES: u64 = 22_404_720;
const WEIGHTS_SHA256: &str = "6bf2e0f64a1d20169736c2836e3a787b12379fdc08ba87f7d94a7a3d58eeefce";

/// The one download the picture check needs.
static PIN: LazyLock<Pinned> = LazyLock::new(|| Pinned {
    id: ID.to_string(),
    revision: REVISION.to_string(),
    files: vec![PinnedFile {
        name: WEIGHTS.to_string(),
        bytes: WEIGHTS_BYTES,
        sha256: WEIGHTS_SHA256.to_string(),
    }],
});

/// Its folder inside the app's own picture-check folder.
const FOLDER: &str = "Marqo--nsfw-image-detection-384";

/// The biggest picture file the check reads, and the most pixels on a side.
const MOST_BYTES: u64 = 64 * 1024 * 1024;
const MOST_SIDE: u32 = 16_384;

/// Said whenever a score is shown.
pub const CAUTION: &str =
    "This is a score from a small model, not a verdict. It can make mistakes.";

// ---------------------------------------------------------------------------
// Where it lives
// ---------------------------------------------------------------------------

/// The app's own picture-check folder.
pub struct PictureCheck {
    home: PathBuf,
}

impl PictureCheck {
    pub fn new(home: PathBuf) -> Self {
        Self { home }
    }

    fn folder(&self) -> PathBuf {
        self.home.join(FOLDER)
    }

    fn installed(&self) -> bool {
        downloads::whole_own(&PIN, &self.folder())
    }
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub name: &'static str,
    pub installed: bool,
    /// The download, in bytes.
    pub bytes: u64,
    pub caution: &'static str,
    /// Whether the page can offer the system's own picker: a Mac, for now.
    /// A dropped picture works everywhere.
    pub picker: bool,
}

fn status_of(check: &PictureCheck) -> Status {
    Status {
        name: NAME,
        installed: check.installed(),
        bytes: WEIGHTS_BYTES,
        caution: CAUTION,
        picker: cfg!(target_os = "macos"),
    }
}

#[tauri::command]
pub fn picture_check_status(check: tauri::State<'_, PictureCheck>) -> Status {
    status_of(&check)
}

/// One click: downloads the model through the WAN gate. Cancel is the same
/// Cancel as a model's download.
#[tauri::command]
pub async fn install_picture_check(
    check: tauri::State<'_, PictureCheck>,
    downloads: tauri::State<'_, Downloads>,
    gate: tauri::State<'_, Gate>,
    on_progress: Channel<Progress>,
) -> Result<DownloadReport, String> {
    let client = downloads::client(&gate)?;
    let open = move |url: String| downloads::open_url(client.clone(), url);
    let report = move |progress: Progress| {
        let _ = on_progress.send(progress);
    };
    downloads
        .download_own(&gate, &PIN, NAME, &check.home, FOLDER, open, report)
        .await
}

/// Moves `folder` with `to_trash` when it is there.
fn uninstall_from(
    folder: &Path,
    to_trash: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    if fs::symlink_metadata(folder).is_err() {
        return Err(format!(
            "The {} is not on this computer.",
            NAME.to_lowercase()
        ));
    }
    to_trash(folder).map_err(|error| {
        format!(
            "The {} could not be moved to {}: {}.",
            NAME.to_lowercase(),
            uninstall::TRASH,
            error.trim_end_matches('.')
        )
    })
}

/// One click: moves the model's folder to the Trash, where the person can
/// take it back. Refused while a download is running.
#[tauri::command]
pub fn uninstall_picture_check(
    check: tauri::State<'_, PictureCheck>,
    downloads: tauri::State<'_, Downloads>,
) -> Result<(), String> {
    if downloads.running() {
        return Err(
            "A download is running. Uninstall once it is done, or cancel it first.".to_string(),
        );
    }
    uninstall_from(&check.folder(), uninstall::to_trash)
}

// ---------------------------------------------------------------------------
// Scoring, in memory
// ---------------------------------------------------------------------------

/// What the page shows: the model's two numbers, which add up to 1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    /// The model's "NSFW" number.
    pub nsfw: f32,
    /// The model's "SFW" number.
    pub sfw: f32,
}

/// Scores one picture on this computer. Works with both gates off.
#[tauri::command]
pub async fn score_picture(
    path: String,
    check: tauri::State<'_, PictureCheck>,
) -> Result<Score, String> {
    let folder = check.folder();
    tauri::async_runtime::spawn_blocking(move || {
        let model = Vit::load(&folder)?;
        let picture = read_picture(Path::new(&path))?;
        model.score(&picture)
    })
    .await
    .map_err(|_| "The picture could not be checked.".to_string())?
}

/// The system's own picker, for one picture. Gives back nothing when the
/// person closes it.
#[tauri::command]
pub async fn pick_picture() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        if !cfg!(target_os = "macos") {
            return Err(
                "Choosing a picture needs a Mac for now. Drop the picture here instead."
                    .to_string(),
            );
        }
        let script = "return POSIX path of (choose file with prompt \"Pick a picture to check\" \
                      of type {\"public.jpeg\", \"public.png\", \"org.webmproject.webp\"})";
        let output = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", script])
            .output()
            .map_err(|_| "Could not open the picker.".to_string())?;
        let picked = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok((!picked.is_empty()).then_some(picked))
    })
    .await
    .map_err(|_| "Could not open the picker.".to_string())?
}

/// The picture's bytes, read once into memory and never written anywhere.
fn read_picture(path: &Path) -> Result<Vec<u8>, String> {
    let meta =
        fs::metadata(path).map_err(|_| "The app could not open that picture.".to_string())?;
    if !meta.is_file() {
        return Err("That is not a picture.".to_string());
    }
    if meta.len() > MOST_BYTES {
        return Err("That picture is over 64 MB, more than the check reads.".to_string());
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    fs::File::open(path)
        .and_then(|file| file.take(MOST_BYTES).read_to_end(&mut bytes))
        .map_err(|_| "The app could not read that picture.".to_string())?;
    Ok(bytes)
}

/// The model's input: 3 × 384 × 384, each value (x / 255 − 0.5) / 0.5.
/// The shorter side is resized to 384 by bicubic, then the middle is cut out,
/// as timm does for this model.
fn prepare(bytes: &[u8], side: u32) -> Result<Vec<f32>, String> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MOST_SIDE);
    limits.max_image_height = Some(MOST_SIDE);
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| UNREADABLE.to_string())?;
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| UNREADABLE.to_string())?;
    let rgb = decoded.to_rgb8();
    let (width, height) = rgb.dimensions();
    if width == 0 || height == 0 {
        return Err(UNREADABLE.to_string());
    }
    let (new_width, new_height) = if width <= height {
        (side, scaled(height, width, side))
    } else {
        (scaled(width, height, side), side)
    };
    let resized = if (new_width, new_height) == (width, height) {
        rgb
    } else {
        image::imageops::resize(
            &rgb,
            new_width,
            new_height,
            image::imageops::FilterType::CatmullRom,
        )
    };
    let left = middle(new_width, side);
    let top = middle(new_height, side);
    let plane = (side * side) as usize;
    let mut out = vec![0f32; plane * 3];
    for y in 0..side {
        for x in 0..side {
            let pixel = resized.get_pixel(left + x, top + y);
            let at = (y * side + x) as usize;
            for (channel, value) in pixel.0.iter().enumerate() {
                out[channel * plane + at] = (f32::from(*value) / 255.0 - 0.5) / 0.5;
            }
        }
    }
    Ok(out)
}

const UNREADABLE: &str = "The app could not read that picture. It reads JPEG, PNG, and WebP.";

/// The longer side once the shorter is `side`, as torchvision works it out.
fn scaled(long: u32, short: u32, side: u32) -> u32 {
    (u64::from(side) * u64::from(long) / u64::from(short)) as u32
}

/// Where the middle `side` starts, rounding half to even as torchvision does.
fn middle(length: u32, side: u32) -> u32 {
    let spare = length - side;
    let half = spare / 2;
    if spare % 2 == 1 && half % 2 == 1 {
        half + 1
    } else {
        half
    }
}

// ---------------------------------------------------------------------------
// The model: timm's vit_tiny_patch16_384, two classes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Shape {
    side: usize,
    patch: usize,
    width: usize,
    depth: usize,
    heads: usize,
    hidden: usize,
}

const MARQO: Shape = Shape {
    side: 384,
    patch: 16,
    width: 192,
    depth: 12,
    heads: 3,
    hidden: 768,
};

struct Block {
    norm1: LayerNorm,
    qkv: Linear,
    proj: Linear,
    norm2: LayerNorm,
    fc1: Linear,
    fc2: Linear,
}

struct Vit {
    shape: Shape,
    patch: Conv2d,
    cls: Tensor,
    position: Tensor,
    blocks: Vec<Block>,
    norm: LayerNorm,
    head: Linear,
}

const EPS: f64 = 1e-6;

fn broken(_: candle_core::Error) -> String {
    "The picture check's model could not be read. Uninstall it and install it again.".to_string()
}

impl Vit {
    /// Reads the installed model, checking its size and SHA-256 first.
    fn load(folder: &Path) -> Result<Self, String> {
        let missing = || {
            format!(
                "The {} is not installed. Install it from the Installers page.",
                NAME.to_lowercase()
            )
        };
        let path = folder.join(WEIGHTS);
        let meta = fs::symlink_metadata(&path).map_err(|_| missing())?;
        if !meta.is_file() || meta.len() != WEIGHTS_BYTES {
            return Err(broken_text());
        }
        let bytes = fs::read(&path).map_err(|_| missing())?;
        if downloads::hex(Sha256::digest(&bytes).as_slice()) != WEIGHTS_SHA256 {
            return Err(broken_text());
        }
        let vb = VarBuilder::from_buffered_safetensors(bytes, DType::F32, &Device::Cpu)
            .map_err(broken)?;
        Self::new(MARQO, vb).map_err(broken)
    }

    fn new(shape: Shape, vb: VarBuilder) -> candle_core::Result<Self> {
        let config = Conv2dConfig {
            stride: shape.patch,
            ..Default::default()
        };
        let patch = candle_nn::conv2d(
            3,
            shape.width,
            shape.patch,
            config,
            vb.pp("patch_embed.proj"),
        )?;
        let tokens = (shape.side / shape.patch).pow(2) + 1;
        let cls = vb.get((1, 1, shape.width), "cls_token")?;
        let position = vb.get((1, tokens, shape.width), "pos_embed")?;
        let blocks = (0..shape.depth)
            .map(|index| {
                let vb = vb.pp(format!("blocks.{index}"));
                Ok(Block {
                    norm1: candle_nn::layer_norm(shape.width, EPS, vb.pp("norm1"))?,
                    qkv: candle_nn::linear(shape.width, shape.width * 3, vb.pp("attn.qkv"))?,
                    proj: candle_nn::linear(shape.width, shape.width, vb.pp("attn.proj"))?,
                    norm2: candle_nn::layer_norm(shape.width, EPS, vb.pp("norm2"))?,
                    fc1: candle_nn::linear(shape.width, shape.hidden, vb.pp("mlp.fc1"))?,
                    fc2: candle_nn::linear(shape.hidden, shape.width, vb.pp("mlp.fc2"))?,
                })
            })
            .collect::<candle_core::Result<Vec<_>>>()?;
        Ok(Self {
            shape,
            patch,
            cls,
            position,
            blocks,
            norm: candle_nn::layer_norm(shape.width, EPS, vb.pp("norm"))?,
            head: candle_nn::linear(shape.width, 2, vb.pp("head"))?,
        })
    }

    /// The two numbers for a picture's bytes.
    fn score(&self, bytes: &[u8]) -> Result<Score, String> {
        let input = prepare(bytes, self.shape.side as u32)?;
        self.score_input(input).map_err(broken)
    }

    fn score_input(&self, input: Vec<f32>) -> candle_core::Result<Score> {
        let side = self.shape.side;
        let x = Tensor::from_vec(input, (1, 3, side, side), &Device::Cpu)?;
        let x = self.patch.forward(&x)?.flatten_from(2)?.transpose(1, 2)?;
        let mut x = (Tensor::cat(&[&self.cls, &x], 1)? + &self.position)?;
        for block in &self.blocks {
            x = (&x + self.attend(block, &block.norm1.forward(&x)?)?)?;
            let hidden = block.fc1.forward(&block.norm2.forward(&x)?)?.gelu_erf()?;
            x = (&x + block.fc2.forward(&hidden)?)?;
        }
        let first = self.norm.forward(&x)?.narrow(1, 0, 1)?.squeeze(1)?;
        let odds = candle_nn::ops::softmax(&self.head.forward(&first)?, D::Minus1)?;
        let odds = odds.squeeze(0)?.to_vec1::<f32>()?;
        Ok(Score {
            nsfw: odds[0],
            sfw: odds[1],
        })
    }

    fn attend(&self, block: &Block, x: &Tensor) -> candle_core::Result<Tensor> {
        let (batch, tokens, width) = x.dims3()?;
        let heads = self.shape.heads;
        let each = width / heads;
        let qkv = block
            .qkv
            .forward(x)?
            .reshape((batch, tokens, 3, heads, each))?
            .permute((2, 0, 3, 1, 4))?;
        let q = (qkv.get(0)?.contiguous()? * (each as f64).powf(-0.5))?;
        let k = qkv.get(1)?.contiguous()?;
        let v = qkv.get(2)?.contiguous()?;
        let weights = candle_nn::ops::softmax_last_dim(&q.matmul(&k.t()?)?)?;
        let mixed = weights
            .matmul(&v)?
            .transpose(1, 2)?
            .reshape((batch, tokens, width))?;
        block.proj.forward(&mixed)
    }
}

fn broken_text() -> String {
    "The picture check's model is not the file the app expects. Uninstall it and install it again."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Brad, 2026-11-22: "one click install on marqo should the user choose,
    // and uninstall too". Every picture here is drawn by the test, in memory.

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("rb-picture-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    /// A drawn picture, from the same integer formulas as the reference.
    fn drawn(kind: &str) -> image::RgbImage {
        let (width, height) = match kind {
            "wide" => (640, 400),
            "tall" => (300, 500),
            _ => (384, 384),
        };
        image::RgbImage::from_fn(width, height, |x, y| {
            let (xi, yi) = (i64::from(x), i64::from(y));
            let p: [u8; 3] = match kind {
                "gradient" => [
                    (x * 255 / 383) as u8,
                    (y * 255 / 383) as u8,
                    (((x / 32 + y / 32) % 2) * 255) as u8,
                ],
                "sky" => {
                    if (xi - 300).pow(2) + (yi - 80).pow(2) <= 1600 {
                        [255, 220, 40]
                    } else if y < 192 {
                        [110, 170, 230]
                    } else {
                        [70, 140, 60]
                    }
                }
                "stripes" => {
                    [[200, 30, 40], [245, 245, 245], [30, 60, 160]][((x + y) / 24 % 3) as usize]
                }
                "wide" => [(x * 255 / 639) as u8, (y * 255 / 399) as u8, 128],
                "tall" => {
                    if (((xi - 150).pow(2) + (yi - 250).pow(2)) / 900) % 2 == 0 {
                        [240, 240, 240]
                    } else {
                        [30, 30, 90]
                    }
                }
                _ => unreachable!(),
            };
            image::Rgb(p)
        })
    }

    fn png(picture: &image::RgbImage) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        picture.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    const TINY: Shape = Shape {
        side: 32,
        patch: 16,
        width: 8,
        depth: 1,
        heads: 2,
        hidden: 16,
    };

    /// A tiny model with every weight zero, and the head's bias as given.
    fn tiny(head_bias: [f32; 2]) -> Vit {
        let zeros = |dims: &[usize]| Tensor::zeros(dims, DType::F32, &Device::Cpu).unwrap();
        let mut ts: HashMap<String, Tensor> = HashMap::new();
        let w = TINY.width;
        ts.insert("patch_embed.proj.weight".into(), zeros(&[w, 3, 16, 16]));
        ts.insert("patch_embed.proj.bias".into(), zeros(&[w]));
        ts.insert("cls_token".into(), zeros(&[1, 1, w]));
        ts.insert("pos_embed".into(), zeros(&[1, 5, w]));
        for name in ["norm1", "norm2"] {
            ts.insert(format!("blocks.0.{name}.weight"), zeros(&[w]));
            ts.insert(format!("blocks.0.{name}.bias"), zeros(&[w]));
        }
        for (name, out, inp) in [
            ("attn.qkv", 3 * w, w),
            ("attn.proj", w, w),
            ("mlp.fc1", TINY.hidden, w),
            ("mlp.fc2", w, TINY.hidden),
        ] {
            ts.insert(format!("blocks.0.{name}.weight"), zeros(&[out, inp]));
            ts.insert(format!("blocks.0.{name}.bias"), zeros(&[out]));
        }
        ts.insert("norm.weight".into(), zeros(&[w]));
        ts.insert("norm.bias".into(), zeros(&[w]));
        ts.insert("head.weight".into(), zeros(&[2, w]));
        ts.insert(
            "head.bias".into(),
            Tensor::new(&head_bias, &Device::Cpu).unwrap(),
        );
        Vit::new(TINY, VarBuilder::from_tensors(ts, DType::F32, &Device::Cpu)).unwrap()
    }

    #[test]
    fn a_tiny_model_scores_a_drawn_picture_through_every_layer() {
        // All weights zero, so only the head's bias is left: softmax(0, ln 3)
        // is a quarter and three quarters.
        let model = tiny([0.0, 3f32.ln()]);
        let score = model.score(&png(&drawn("sky"))).unwrap();
        assert!((score.nsfw - 0.25).abs() < 1e-6, "{score:?}");
        assert!((score.sfw - 0.75).abs() < 1e-6, "{score:?}");
    }

    #[test]
    fn the_numbers_always_add_up_to_one() {
        let model = tiny([2.5, -1.0]);
        for kind in ["gradient", "wide", "tall"] {
            let score = model.score(&png(&drawn(kind))).unwrap();
            assert!((score.nsfw + score.sfw - 1.0).abs() < 1e-6);
            assert!(score.nsfw > score.sfw);
        }
    }

    #[test]
    fn the_shorter_side_becomes_384_and_the_middle_is_kept() {
        assert_eq!(scaled(640, 400, 384), 614);
        assert_eq!(scaled(500, 300, 384), 640);
        // torchvision rounds half to even.
        assert_eq!(middle(614, 384), 115);
        assert_eq!(middle(387, 384), 2);
        assert_eq!(middle(385, 384), 0);
        assert_eq!(middle(384, 384), 0);
        // A square 384 picture is used as it is: black stays -1, white 1.
        let mut picture = image::RgbImage::new(384, 384);
        picture.put_pixel(383, 383, image::Rgb([255, 255, 255]));
        let input = prepare(&png(&picture), 384).unwrap();
        assert_eq!(input.len(), 3 * 384 * 384);
        assert_eq!(input[0], -1.0);
        assert_eq!(input[384 * 384 - 1], 1.0);
    }

    #[test]
    fn something_that_is_not_a_picture_is_refused_plainly() {
        let model = tiny([0.0, 0.0]);
        assert_eq!(model.score(b"not a picture").unwrap_err(), UNREADABLE);
        assert_eq!(model.score(&[]).unwrap_err(), UNREADABLE);
    }

    #[test]
    fn a_picture_wider_than_the_limit_is_refused() {
        // A PNG header claiming 20,000 pixels across is refused before its
        // pixels are made.
        let picture = image::RgbImage::new(MOST_SIDE + 1, 1);
        let model = tiny([0.0, 0.0]);
        assert_eq!(model.score(&png(&picture)).unwrap_err(), UNREADABLE);
    }

    #[test]
    fn scoring_writes_nothing_and_leaves_the_picture_as_it_was() {
        // "No copy, no hash, no score": the picture's folder and the app's
        // folder hold exactly what they held before.
        let pictures = folder("pictures");
        let home = folder("home");
        let path = pictures.join("drawn.png");
        let bytes = png(&drawn("gradient"));
        fs::write(&path, &bytes).unwrap();
        let listing = |dir: &Path| {
            let mut names: Vec<_> = fs::read_dir(dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            names.sort();
            names
        };
        let (before_pictures, before_home) = (listing(&pictures), listing(&home));
        let read = read_picture(&path).unwrap();
        tiny([0.0, 0.0]).score(&read).unwrap();
        assert_eq!(listing(&pictures), before_pictures);
        assert_eq!(listing(&home), before_home);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_dir_all(&pictures).unwrap();
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_folder_or_a_huge_file_is_not_read() {
        let pictures = folder("huge");
        assert_eq!(
            read_picture(&pictures).unwrap_err(),
            "That is not a picture."
        );
        let big = pictures.join("big.png");
        fs::File::create(&big)
            .unwrap()
            .set_len(MOST_BYTES + 1)
            .unwrap();
        assert!(read_picture(&big).unwrap_err().contains("over 64 MB"));
        fs::remove_dir_all(&pictures).unwrap();
    }

    #[test]
    fn not_installed_says_where_to_install_it() {
        let home = folder("missing");
        let check = PictureCheck::new(home.clone());
        assert!(!status_of(&check).installed);
        let error = Vit::load(&check.folder()).err().unwrap();
        assert!(error.contains("Installers page"), "{error}");
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_file_that_is_not_the_pinned_one_is_refused() {
        let home = folder("wrong");
        let check = PictureCheck::new(home.clone());
        fs::create_dir_all(check.folder()).unwrap();
        // The right size, the wrong bytes.
        fs::File::create(check.folder().join(WEIGHTS))
            .unwrap()
            .set_len(WEIGHTS_BYTES)
            .unwrap();
        assert!(status_of(&check).installed);
        assert_eq!(Vit::load(&check.folder()).err().unwrap(), broken_text());
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn uninstall_moves_only_its_own_folder_and_never_deletes() {
        let home = folder("uninstall");
        let check = PictureCheck::new(home.clone());
        fs::create_dir_all(check.folder()).unwrap();
        fs::write(home.join("other"), "kept").unwrap();
        let mut moved = Vec::new();
        uninstall_from(&check.folder(), |place| {
            moved.push(place.to_path_buf());
            Ok(())
        })
        .unwrap();
        assert_eq!(moved, vec![check.folder()]);
        assert!(check.folder().exists(), "only the Trash moves it");
        assert!(home.join("other").exists());
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn uninstall_says_so_when_nothing_is_there_or_the_trash_refuses() {
        let home = folder("uninstall-none");
        let check = PictureCheck::new(home.clone());
        let error = uninstall_from(&check.folder(), |_| panic!("nothing to move")).unwrap_err();
        assert!(error.contains("not on this computer"), "{error}");
        fs::create_dir_all(check.folder()).unwrap();
        let error = uninstall_from(&check.folder(), |_| Err("no room.".into())).unwrap_err();
        assert_eq!(
            error,
            format!(
                "The picture check could not be moved to {}: no room.",
                uninstall::TRASH
            )
        );
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn the_pin_is_one_file_at_one_revision() {
        assert_eq!(PIN.files.len(), 1);
        assert_eq!(PIN.bytes(), 22_404_720);
        assert_eq!(PIN.revision.len(), 40);
        assert_eq!(
            status_of(&PictureCheck::new(PathBuf::from("/nowhere"))).caution,
            CAUTION
        );
        assert!(CAUTION.contains("can make mistakes"));
    }

    /// The real model against timm's own numbers for the same drawn
    /// pictures, from one throwaway container (timm 1.0.12, torch 2.5.1).
    /// Run with RB_PICTURE_WEIGHTS set to a folder holding model.safetensors.
    #[test]
    #[ignore = "needs the real weights, which never go in the repository"]
    fn the_real_model_matches_timm() {
        let folder = PathBuf::from(std::env::var("RB_PICTURE_WEIGHTS").unwrap());
        let model = Vit::load(&folder).unwrap();
        for (kind, nsfw, tolerance) in [
            ("gradient", 0.084_509_88, 1e-5),
            ("sky", 0.117_349_18, 1e-5),
            ("stripes", 0.065_601_5, 1e-5),
            // Resized pictures: bicubic in image and in Pillow differ a little.
            ("wide", 0.154_984_33, 1e-3),
            ("tall", 0.046_087_6, 1e-3),
        ] {
            let score = model.score(&png(&drawn(kind))).unwrap();
            println!("{kind}: {} (timm {nsfw})", score.nsfw);
            assert!((score.nsfw - nsfw).abs() < tolerance, "{kind}: {score:?}");
        }
    }
}
