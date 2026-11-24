//! The Utilities page: small tools for everyday work that run on this
//! computer alone. None of them reaches past it, so they work with both gates
//! off.
//!
//! Brad, 2026-11-22: "I want a new "utilities" page or button, and it's job is
//! to list helpful utilities I used all the time. convert HEIC to JPEG is
//! goal." Then: "Mac only for now, strip first with a toggle to keep".
//!
//! HEIC to JPEG makes a JPEG copy of each picture beside it, with the Mac's
//! own image reader, ImageIO, so nothing is downloaded. The picture it reads
//! is never changed, and a file already there is never replaced: the copy
//! takes the next free name. Where the picture was taken is left out of the
//! copy unless the person keeps it. Windows and Linux are told it needs a Mac
//! for now.
//!
//! Brad, 2026-11-24: "heic worked great, but I need a confirmation I looked in
//! folder and saw they converted." So each copy is read back from its folder
//! before the page is told it was saved, and the answer says which folder.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

/// The largest picture read, so one file cannot take the app's memory.
const LARGEST: u64 = 256 * 1024 * 1024;

/// The most copies of one name tried before giving up: `IMG_1234-999.jpg`.
const MOST_NAMES: u32 = 999;

/// Said wherever the system cannot convert.
const MAC_ONLY: &str = "HEIC to JPEG needs a Mac for now.";

/// What the page shows each utility as able to do here.
#[derive(Serialize)]
pub struct Status {
    pub heic: bool,
}

/// A HEIC picture the page may convert.
#[derive(Serialize, Debug, PartialEq)]
pub struct Picture {
    pub path: String,
    pub name: String,
    pub bytes: u64,
}

/// The pictures dropped or picked, and how many other files were passed over.
#[derive(Serialize, Debug, PartialEq)]
pub struct Pictures {
    pub pictures: Vec<Picture>,
    pub passed: usize,
}

/// The JPEG one picture became, found in its folder after it was saved.
#[derive(Serialize, Debug)]
pub struct Converted {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    /// The folder it was found in, the same one as the picture.
    pub folder: String,
    /// Whether where the picture was taken was kept in the copy.
    pub kept_location: bool,
}

/// The JPEGs this window made. Show opens only these, so the page cannot ask
/// it to open anything else.
#[derive(Default)]
pub struct Made(Mutex<HashSet<PathBuf>>);

#[tauri::command]
pub fn utilities_status() -> Status {
    Status {
        heic: cfg!(target_os = "macos"),
    }
}

/// The HEIC pictures among dropped or picked paths. A folder gives the HEIC
/// pictures directly inside it. Links and anything that is not a HEIC
/// picture are passed over.
#[tauri::command]
pub async fn heic_pictures(paths: Vec<String>) -> Result<Pictures, String> {
    tauri::async_runtime::spawn_blocking(move || Ok(pictures_in(&paths)))
        .await
        .map_err(|_| "The pictures could not be listed.".to_string())?
}

/// Makes a JPEG copy of one HEIC picture, beside it, and answers with the
/// copy once it is found in the folder. `quality` runs from 10 to 100.
#[tauri::command]
pub async fn heic_to_jpeg(
    path: String,
    quality: u8,
    keep_location: bool,
    made: tauri::State<'_, Made>,
) -> Result<Converted, String> {
    let source = picture_at(&path)?;
    let out =
        tauri::async_runtime::spawn_blocking(move || convert(&source, quality, keep_location))
            .await
            .map_err(|_| "The picture could not be converted.".to_string())??;
    let bytes = fs::metadata(&out).map(|meta| meta.len()).unwrap_or(0);
    let name = file_name(&out);
    let folder = out
        .parent()
        .map(|folder| folder.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Ok(mut list) = made.0.lock() {
        list.insert(out.clone());
    }
    Ok(Converted {
        path: out.to_string_lossy().into_owned(),
        name,
        bytes,
        folder,
        kept_location: keep_location,
    })
}

/// Shows a JPEG this window made in the Finder. Any other path is refused.
#[tauri::command]
pub fn show_jpeg(path: String, made: tauri::State<'_, Made>) -> Result<(), String> {
    let path = PathBuf::from(path);
    let known = made
        .0
        .lock()
        .map(|list| list.contains(&path))
        .unwrap_or(false);
    if !known {
        return Err("Only a JPEG made here can be shown.".to_string());
    }
    if !cfg!(target_os = "macos") {
        return Err(MAC_ONLY.to_string());
    }
    std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(&path)
        .status()
        .map_err(|_| "The Finder could not be opened.".to_string())
        .map(|_| ())
}

/// The system's own picker: HEIC pictures, or one folder. Empty when cancelled.
#[tauri::command]
pub async fn pick_heic(folder: bool) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !cfg!(target_os = "macos") {
            return Err(MAC_ONLY.to_string());
        }
        let script = if folder {
            "return POSIX path of (choose folder with prompt \"Pick a folder of HEIC pictures\")"
        } else {
            "set picked to choose file with prompt \"Pick HEIC pictures\" of type {\"public.heic\", \"public.heif\"} with multiple selections allowed\n\
             set out to \"\"\n\
             repeat with one in picked\n\
             set out to out & POSIX path of one & linefeed\n\
             end repeat\n\
             return out"
        };
        let output = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", script])
            .output()
            .map_err(|_| "Could not open the picker.".to_string())?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    })
    .await
    .map_err(|_| "Could not open the picker.".to_string())?
}

/// True for a name ending in .heic or .heif, in any case.
fn is_heic(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("heic") || ext.eq_ignore_ascii_case("heif"))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A HEIC picture as the page names it: a full path to a file, not a link.
fn picture_at(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    if !path.is_absolute() || !is_heic(&path) {
        return Err("Only a HEIC picture can be converted.".to_string());
    }
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_file() => Ok(path),
        _ => Err(format!("{} is not there any more.", file_name(&path))),
    }
}

fn pictures_in(paths: &[String]) -> Pictures {
    let mut pictures = Vec::new();
    let mut seen = HashSet::new();
    let mut passed = 0;
    let mut take = |path: PathBuf, pictures: &mut Vec<Picture>, passed: &mut usize| {
        let meta = fs::symlink_metadata(&path);
        match meta {
            Ok(meta) if meta.is_file() && is_heic(&path) => {
                if seen.insert(path.clone()) {
                    pictures.push(Picture {
                        name: file_name(&path),
                        path: path.to_string_lossy().into_owned(),
                        bytes: meta.len(),
                    });
                }
            }
            _ => *passed += 1,
        }
    };
    for given in paths {
        let path = PathBuf::from(given);
        if !path.is_absolute() {
            passed += 1;
            continue;
        }
        let is_folder = fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir());
        if !is_folder {
            take(path, &mut pictures, &mut passed);
            continue;
        }
        let Ok(entries) = fs::read_dir(&path) else {
            passed += 1;
            continue;
        };
        let mut inside: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        inside.sort();
        for one in inside {
            // A folder's other files are not counted as passed over: only
            // what was dropped or picked by itself is.
            if is_heic(&one) {
                take(one, &mut pictures, &mut passed);
            }
        }
    }
    Pictures { pictures, passed }
}

/// `IMG_1234.jpg`, then `IMG_1234-1.jpg`, `IMG_1234-2.jpg`, and on.
fn jpeg_name(stem: &str, try_number: u32) -> String {
    if try_number == 0 {
        format!("{stem}.jpg")
    } else {
        format!("{stem}-{try_number}.jpg")
    }
}

/// Writes `bytes` under the first free name beside `source`. A file already
/// there is never opened, so it is never replaced.
fn write_beside(source: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let folder = source
        .parent()
        .ok_or_else(|| "The picture has no folder.".to_string())?;
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "picture".to_string());
    for try_number in 0..=MOST_NAMES {
        let out = folder.join(jpeg_name(&stem, try_number));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&out);
        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("The folder cannot be written to.".to_string()),
        };
        if file.write_all(bytes).and_then(|_| file.sync_all()).is_err() {
            // Only the file this call just made, and nothing else.
            drop(file);
            let _ = fs::remove_file(&out);
            return Err("The copy could not be written.".to_string());
        }
        return Ok(out);
    }
    Err(format!(
        "Every name from {stem}.jpg to {stem}-{MOST_NAMES}.jpg is taken."
    ))
}

/// Reads one picture and writes its JPEG copy beside it.
fn convert(source: &Path, quality: u8, keep_location: bool) -> Result<PathBuf, String> {
    if !(10..=100).contains(&quality) {
        return Err("Quality runs from 10 to 100.".to_string());
    }
    let size = fs::metadata(source)
        .map_err(|_| format!("{} is not there any more.", file_name(source)))?
        .len();
    if size > LARGEST {
        return Err(format!(
            "{} is larger than the 256 MB this reads.",
            file_name(source)
        ));
    }
    let bytes =
        fs::read(source).map_err(|_| format!("{} could not be read.", file_name(source)))?;
    let jpeg = to_jpeg(&bytes, f64::from(quality) / 100.0, keep_location)?;
    let out = write_beside(source, &jpeg)?;
    found_in_folder(&out, &jpeg)?;
    Ok(out)
}

/// Reads a copy back from its folder, so it is called saved only once it is
/// there: a file of its own, not a link, holding the very JPEG that was made.
/// A copy that reads back wrong is left where it is, and the person is told.
fn found_in_folder(out: &Path, made: &[u8]) -> Result<(), String> {
    let name = file_name(out);
    let on_disk = match fs::symlink_metadata(out) {
        Ok(meta) if meta.is_file() => fs::read(out).ok(),
        _ => None,
    };
    let Some(on_disk) = on_disk else {
        return Err(format!("{name} was saved but is not in the folder."));
    };
    if on_disk != made || !on_disk.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Err(format!(
            "{name} is in the folder but does not read back as the JPEG that was made."
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn to_jpeg(_bytes: &[u8], _quality: f64, _keep_location: bool) -> Result<Vec<u8>, String> {
    Err(MAC_ONLY.to_string())
}

#[cfg(target_os = "macos")]
fn to_jpeg(bytes: &[u8], quality: f64, keep_location: bool) -> Result<Vec<u8>, String> {
    mac::to_jpeg(bytes, quality, keep_location)
}

/// The few calls into the Mac's CoreFoundation and ImageIO this needs, by
/// hand, so no crate is added.
#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;

    pub type CfRef = *const c_void;
    type CfIndex = isize;

    const UTF8: u32 = 0x0800_0100;
    const FLOAT64: CfIndex = 6;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;
        pub static kCFNull: CfRef;
        fn CFRelease(cf: CfRef);
        fn CFDataCreate(alloc: CfRef, bytes: *const u8, length: CfIndex) -> CfRef;
        fn CFDataCreateMutable(alloc: CfRef, capacity: CfIndex) -> CfRef;
        fn CFDataGetLength(data: CfRef) -> CfIndex;
        fn CFDataGetBytePtr(data: CfRef) -> *const u8;
        fn CFStringCreateWithBytes(
            alloc: CfRef,
            bytes: *const u8,
            length: CfIndex,
            encoding: u32,
            external: u8,
        ) -> CfRef;
        fn CFNumberCreate(alloc: CfRef, kind: CfIndex, value: *const c_void) -> CfRef;
        fn CFDictionaryCreate(
            alloc: CfRef,
            keys: *const CfRef,
            values: *const CfRef,
            count: CfIndex,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> CfRef;
        #[cfg(test)]
        pub fn CFDictionaryGetValue(dictionary: CfRef, key: CfRef) -> CfRef;
    }

    #[link(name = "ImageIO", kind = "framework")]
    unsafe extern "C" {
        pub static kCGImagePropertyGPSDictionary: CfRef;
        static kCGImageDestinationLossyCompressionQuality: CfRef;
        fn CGImageSourceCreateWithData(data: CfRef, options: CfRef) -> CfRef;
        fn CGImageSourceGetCount(source: CfRef) -> usize;
        fn CGImageSourceGetPrimaryImageIndex(source: CfRef) -> usize;
        #[cfg(test)]
        pub fn CGImageSourceCopyPropertiesAtIndex(
            source: CfRef,
            index: usize,
            options: CfRef,
        ) -> CfRef;
        fn CGImageDestinationCreateWithData(
            data: CfRef,
            kind: CfRef,
            count: usize,
            options: CfRef,
        ) -> CfRef;
        pub fn CGImageDestinationAddImageFromSource(
            destination: CfRef,
            source: CfRef,
            index: usize,
            properties: CfRef,
        );
        fn CGImageDestinationFinalize(destination: CfRef) -> bool;
    }

    /// A CoreFoundation object this code made, let go when dropped.
    pub struct Owned(pub CfRef);

    impl Owned {
        fn made(raw: CfRef) -> Option<Owned> {
            (!raw.is_null()).then_some(Owned(raw))
        }
    }

    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: made by a CoreFoundation call that hands over ownership,
            // and released once.
            unsafe { CFRelease(self.0) }
        }
    }

    pub fn text(value: &str) -> Owned {
        // SAFETY: the bytes are valid UTF-8 for their length.
        Owned::made(unsafe {
            CFStringCreateWithBytes(
                std::ptr::null(),
                value.as_ptr(),
                value.len() as CfIndex,
                UTF8,
                0,
            )
        })
        .expect("CoreFoundation makes a short string")
    }

    pub fn number(value: f64) -> Owned {
        // SAFETY: FLOAT64 reads one f64 from the pointer.
        Owned::made(unsafe {
            CFNumberCreate(std::ptr::null(), FLOAT64, (&value as *const f64).cast())
        })
        .expect("CoreFoundation makes a number")
    }

    pub fn dictionary(pairs: &[(CfRef, CfRef)]) -> Owned {
        let keys: Vec<CfRef> = pairs.iter().map(|pair| pair.0).collect();
        let values: Vec<CfRef> = pairs.iter().map(|pair| pair.1).collect();
        // SAFETY: the keys and values are live CoreFoundation objects, which
        // the dictionary retains; the callbacks are CoreFoundation's own.
        Owned::made(unsafe {
            CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                pairs.len() as CfIndex,
                &raw const kCFTypeDictionaryKeyCallBacks,
                &raw const kCFTypeDictionaryValueCallBacks,
            )
        })
        .expect("CoreFoundation makes a dictionary")
    }

    pub fn data(bytes: &[u8]) -> Owned {
        // SAFETY: CoreFoundation copies `bytes` for their length.
        Owned::made(unsafe {
            CFDataCreate(std::ptr::null(), bytes.as_ptr(), bytes.len() as CfIndex)
        })
        .expect("CoreFoundation makes data")
    }

    pub fn bytes_of(data: &Owned) -> Vec<u8> {
        // SAFETY: the pointer is valid for the data's length while `data` lives.
        unsafe {
            let length = CFDataGetLength(data.0) as usize;
            let start = CFDataGetBytePtr(data.0);
            if start.is_null() || length == 0 {
                return Vec::new();
            }
            std::slice::from_raw_parts(start, length).to_vec()
        }
    }

    /// A picture's reader, with the index of its main image.
    pub fn source(bytes: &[u8]) -> Option<(Owned, usize)> {
        let data = data(bytes);
        // SAFETY: `data` is a live CFData; the source retains it.
        let source = Owned::made(unsafe { CGImageSourceCreateWithData(data.0, std::ptr::null()) })?;
        // SAFETY: `source` is a live image source.
        let (count, primary) = unsafe {
            (
                CGImageSourceGetCount(source.0),
                CGImageSourceGetPrimaryImageIndex(source.0),
            )
        };
        if count == 0 {
            return None;
        }
        Some((source, if primary < count { primary } else { 0 }))
    }

    /// Writes one image of `source` as `kind` ("public.jpeg"), with
    /// `properties` laid over its own.
    pub fn write(kind: &str, source: &Owned, index: usize, properties: &Owned) -> Option<Vec<u8>> {
        // SAFETY: each object is live for the calls that use it.
        unsafe {
            let out = Owned::made(CFDataCreateMutable(std::ptr::null(), 0))?;
            let kind = text(kind);
            let destination = Owned::made(CGImageDestinationCreateWithData(
                out.0,
                kind.0,
                1,
                std::ptr::null(),
            ))?;
            CGImageDestinationAddImageFromSource(destination.0, source.0, index, properties.0);
            if !CGImageDestinationFinalize(destination.0) {
                return None;
            }
            drop(destination);
            Some(bytes_of(&out))
        }
    }

    pub fn to_jpeg(bytes: &[u8], quality: f64, keep_location: bool) -> Result<Vec<u8>, String> {
        let (source, index) =
            source(bytes).ok_or_else(|| "The picture could not be read.".to_string())?;
        let quality = number(quality);
        // SAFETY: the keys are ImageIO's own constants, and kCFNull is
        // CoreFoundation's; null for a key is how ImageIO is told to leave
        // that part out of the copy.
        let properties = unsafe {
            let mut pairs = vec![(kCGImageDestinationLossyCompressionQuality, quality.0)];
            if !keep_location {
                pairs.push((kCGImagePropertyGPSDictionary, kCFNull));
            }
            dictionary(&pairs)
        };
        write("public.jpeg", &source, index, &properties)
            .filter(|jpeg| jpeg.starts_with(&[0xFF, 0xD8, 0xFF]))
            .ok_or_else(|| "The JPEG could not be made.".to_string())
    }

    /// True when the picture's main image says where it was taken.
    #[cfg(test)]
    pub fn has_location(bytes: &[u8]) -> bool {
        let Some((source, index)) = source(bytes) else {
            return false;
        };
        // SAFETY: `source` is live; the properties are released when dropped.
        unsafe {
            let Some(properties) = Owned::made(CGImageSourceCopyPropertiesAtIndex(
                source.0,
                index,
                std::ptr::null(),
            )) else {
                return false;
            };
            !CFDictionaryGetValue(properties.0, kCGImagePropertyGPSDictionary).is_null()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own for one test, removed at the end.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Scratch {
            let folder = std::env::temp_dir()
                .join(format!("rusty-business-utilities-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&folder).unwrap();
            Scratch(folder)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn heic_is_known_by_its_name_in_any_case() {
        assert!(is_heic(Path::new("/a/IMG_1234.HEIC")));
        assert!(is_heic(Path::new("/a/IMG_1234.heic")));
        assert!(is_heic(Path::new("/a/IMG_1234.Heif")));
        assert!(!is_heic(Path::new("/a/IMG_1234.jpg")));
        assert!(!is_heic(Path::new("/a/heic")));
    }

    #[test]
    fn a_copy_takes_the_next_free_name() {
        assert_eq!(jpeg_name("IMG_1234", 0), "IMG_1234.jpg");
        assert_eq!(jpeg_name("IMG_1234", 1), "IMG_1234-1.jpg");
        assert_eq!(jpeg_name("IMG_1234", 12), "IMG_1234-12.jpg");
    }

    // Brad, 2026-11-22: "take suggested": the JPEG goes beside the picture,
    // and a file already there is never replaced.
    #[test]
    fn a_file_already_there_is_never_replaced() {
        let scratch = Scratch::new();
        let source = scratch.0.join("IMG_1234.HEIC");
        fs::write(&source, b"the picture").unwrap();
        fs::write(scratch.0.join("IMG_1234.jpg"), b"already here").unwrap();
        let first = write_beside(&source, b"copy one").unwrap();
        let second = write_beside(&source, b"copy two").unwrap();
        assert_eq!(file_name(&first), "IMG_1234-1.jpg");
        assert_eq!(file_name(&second), "IMG_1234-2.jpg");
        assert_eq!(
            fs::read(scratch.0.join("IMG_1234.jpg")).unwrap(),
            b"already here"
        );
        assert_eq!(fs::read(&first).unwrap(), b"copy one");
        assert_eq!(fs::read(&source).unwrap(), b"the picture");
    }

    #[test]
    fn only_heic_pictures_are_listed_and_a_folder_gives_its_own() {
        let scratch = Scratch::new();
        let folder = scratch.0.join("trip");
        fs::create_dir_all(folder.join("inner")).unwrap();
        fs::write(folder.join("b.HEIC"), b"12").unwrap();
        fs::write(folder.join("a.heic"), b"1").unwrap();
        fs::write(folder.join("notes.txt"), b"x").unwrap();
        fs::write(folder.join("inner").join("c.heic"), b"x").unwrap();
        let alone = scratch.0.join("d.heif");
        fs::write(&alone, b"123").unwrap();
        let other = scratch.0.join("e.png");
        fs::write(&other, b"x").unwrap();
        let path = |p: &Path| p.to_string_lossy().into_owned();
        let found = pictures_in(&[
            path(&folder),
            path(&alone),
            path(&alone),
            path(&other),
            "relative.heic".to_string(),
        ]);
        let names: Vec<&str> = found.pictures.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            ["a.heic", "b.HEIC", "d.heif"],
            "not the folder's inner folders, and each once"
        );
        assert_eq!(found.pictures[1].bytes, 2);
        assert_eq!(
            found.passed, 2,
            "the PNG and the name that is not a full path"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_never_followed() {
        let scratch = Scratch::new();
        let real = scratch.0.join("real.heic");
        fs::write(&real, b"x").unwrap();
        let link = scratch.0.join("link.heic");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let found = pictures_in(&[link.to_string_lossy().into_owned()]);
        assert!(found.pictures.is_empty());
        assert_eq!(found.passed, 1);
        assert!(picture_at(&link.to_string_lossy()).is_err());
        assert!(picture_at(&real.to_string_lossy()).is_ok());
        assert!(
            picture_at("real.heic").is_err(),
            "a name that is not a full path"
        );
        assert!(picture_at(&scratch.0.join("real.png").to_string_lossy()).is_err());
    }

    // Brad, 2026-11-24: "heic worked great, but I need a confirmation I looked
    // in folder and saw they converted."
    #[test]
    fn a_copy_is_called_saved_only_once_it_is_found_in_its_folder() {
        let scratch = Scratch::new();
        let made = [0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];
        let copy = scratch.0.join("IMG_1.jpg");
        fs::write(&copy, made).unwrap();
        assert!(found_in_folder(&copy, &made).is_ok(), "the copy is there");

        let other = [0xFF, 0xD8, 0xFF, 0xE0, 9, 9, 9];
        assert_eq!(
            found_in_folder(&copy, &other).unwrap_err(),
            "IMG_1.jpg is in the folder but does not read back as the JPEG that was made.",
            "other bytes than the ones made"
        );
        assert_eq!(
            found_in_folder(&scratch.0.join("IMG_2.jpg"), &made).unwrap_err(),
            "IMG_2.jpg was saved but is not in the folder.",
            "a copy that is gone"
        );
        let folder = scratch.0.join("IMG_3.jpg");
        fs::create_dir(&folder).unwrap();
        assert!(
            found_in_folder(&folder, &made).is_err(),
            "a folder, not a file"
        );
        let words = scratch.0.join("IMG_4.jpg");
        fs::write(&words, b"not a jpeg").unwrap();
        assert!(
            found_in_folder(&words, b"not a jpeg").is_err(),
            "the same bytes, but not a JPEG"
        );
        #[cfg(unix)]
        {
            let link = scratch.0.join("IMG_5.jpg");
            std::os::unix::fs::symlink(&copy, &link).unwrap();
            assert!(
                found_in_folder(&link, &made).is_err(),
                "a link is not the copy"
            );
        }
        assert_eq!(
            fs::read(&copy).unwrap(),
            made,
            "reading back changes nothing"
        );
    }

    #[test]
    fn quality_out_of_range_is_refused_before_anything_is_read() {
        let scratch = Scratch::new();
        let source = scratch.0.join("a.heic");
        fs::write(&source, b"x").unwrap();
        assert!(convert(&source, 9, false).is_err());
        assert!(convert(&source, 101, false).is_err());
        assert_eq!(
            fs::read_dir(&scratch.0).unwrap().count(),
            1,
            "nothing was written"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn other_systems_are_told_it_needs_a_mac() {
        let scratch = Scratch::new();
        let source = scratch.0.join("a.heic");
        fs::write(&source, b"x").unwrap();
        assert_eq!(convert(&source, 90, false).unwrap_err(), MAC_ONLY);
        assert!(!utilities_status().heic);
        assert_eq!(
            fs::read_dir(&scratch.0).unwrap().count(),
            1,
            "nothing was written"
        );
    }

    /// A drawn picture: colored bands, 32 by 24, as a BMP.
    #[cfg(target_os = "macos")]
    fn drawn_bmp() -> Vec<u8> {
        let (width, height) = (32u32, 24u32);
        let row = width * 3;
        let size = 54 + row * height;
        let mut bmp = Vec::new();
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&size.to_le_bytes());
        bmp.extend_from_slice(&[0; 4]);
        bmp.extend_from_slice(&54u32.to_le_bytes());
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&width.to_le_bytes());
        bmp.extend_from_slice(&height.to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&24u16.to_le_bytes());
        bmp.extend_from_slice(&[0; 24]);
        for y in 0..height {
            for x in 0..width {
                let band = (x / 8 + y / 8) % 3;
                let pixel = match band {
                    0 => [200, 80, 40],
                    1 => [40, 160, 90],
                    _ => [60, 90, 220],
                };
                bmp.extend_from_slice(&pixel);
            }
        }
        bmp
    }

    /// The drawn picture as `kind`, marked as taken at a made-up place out
    /// at sea. None when this Mac cannot write that kind.
    #[cfg(target_os = "macos")]
    fn drawn(kind: &str) -> Option<Vec<u8>> {
        use mac::*;
        let (source, index) = source(&drawn_bmp()).expect("ImageIO reads a BMP");
        let latitude = number(0.5);
        let longitude = number(0.5);
        let north = text("N");
        let east = text("E");
        let (lat, lat_ref, lon, lon_ref) = (
            text("Latitude"),
            text("LatitudeRef"),
            text("Longitude"),
            text("LongitudeRef"),
        );
        let place = dictionary(&[
            (lat.0, latitude.0),
            (lat_ref.0, north.0),
            (lon.0, longitude.0),
            (lon_ref.0, east.0),
        ]);
        // SAFETY: an ImageIO constant as the key.
        let properties = dictionary(&[(unsafe { kCGImagePropertyGPSDictionary }, place.0)]);
        write(kind, &source, index, &properties).filter(|bytes| !bytes.is_empty())
    }

    /// One drawn picture through the converter, both ways.
    #[cfg(target_os = "macos")]
    fn converts(kind: &str, name: &str) -> bool {
        let Some(picture) = drawn(kind) else {
            return false;
        };
        assert!(
            mac::has_location(&picture),
            "the drawn {kind} says where it was taken"
        );
        let scratch = Scratch::new();
        let source = scratch.0.join(name);
        fs::write(&source, &picture).unwrap();

        // Brad, 2026-11-22: "strip first with a toggle to keep"
        let stripped = convert(&source, 90, false).unwrap();
        let jpeg = fs::read(&stripped).unwrap();
        assert!(jpeg.starts_with(&[0xFF, 0xD8, 0xFF]), "a JPEG");
        assert!(!mac::has_location(&jpeg), "where it was taken is left out");
        assert!(
            !jpeg.windows(3).any(|part| part == b"GPS"),
            "and not kept in any other form"
        );

        let kept = convert(&source, 90, true).unwrap();
        assert!(
            mac::has_location(&fs::read(&kept).unwrap()),
            "kept when asked"
        );

        assert_eq!(
            file_name(&stripped),
            format!("{}.jpg", name.split('.').next().unwrap())
        );
        assert_eq!(
            file_name(&kept),
            format!("{}-1.jpg", name.split('.').next().unwrap())
        );
        assert_eq!(
            fs::read(&stripped).unwrap(),
            jpeg,
            "the first copy is not replaced"
        );
        assert_eq!(
            fs::read(&source).unwrap(),
            picture,
            "the picture read is never changed"
        );
        true
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_drawn_picture_becomes_a_jpeg_without_where_it_was_taken() {
        // A TIFF, which every Mac writes, so the rules are checked on every Mac.
        assert!(converts("public.tiff", "IMG_0001.HEIC"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_drawn_heic_becomes_a_jpeg_without_where_it_was_taken() {
        // A Mac with no HEIC encoder, such as some virtual ones, cannot draw
        // the picture; the TIFF test above still checks every rule there.
        if !converts("public.heic", "IMG_0002.HEIC") {
            eprintln!("This Mac cannot write HEIC, so the drawn HEIC test did not run.");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_file_that_is_not_a_picture_is_refused_and_nothing_is_written() {
        let scratch = Scratch::new();
        let source = scratch.0.join("IMG_0003.HEIC");
        fs::write(&source, b"not a picture").unwrap();
        assert!(convert(&source, 90, false).is_err());
        assert_eq!(
            fs::read_dir(&scratch.0).unwrap().count(),
            1,
            "nothing was written"
        );
    }
}
