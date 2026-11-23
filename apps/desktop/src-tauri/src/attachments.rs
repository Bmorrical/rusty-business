//! Files attached to a message, read into text on this computer: PDFs, Word
//! and PowerPoint files, spreadsheets, and anything that is already text,
//! such as CSV, Markdown, JSON, or code. The model is given the text, never
//! the file. Images are turned away until the app has a model that can see.

use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use serde::Serialize;

/// The most of one file's text a message carries: about 15,000 tokens.
pub const TEXT_LIMIT: usize = 60_000;
const FILE_LIMIT: u64 = 50_000_000;
/// The most an Office file's parts may unpack to, all together, so a small
/// zip bomb cannot fill the memory.
const UNPACKED_LIMIT: u64 = 200_000_000;
const SHEET_ROWS: usize = 2_000;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub name: String,
    /// "PDF", "Word", "CSV", and so on, for the label in the chat.
    pub kind: String,
    pub text: String,
    /// How long the whole text was, before any of it was cut.
    pub chars: usize,
    pub cut: bool,
}

const IMAGES: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "heic", "heif", "bmp", "tiff", "svg",
];

fn kind_of(extension: &str) -> String {
    match extension {
        "pdf" => "PDF".to_string(),
        "docx" => "Word".to_string(),
        "pptx" => "PowerPoint".to_string(),
        "xlsx" | "xlsm" | "xls" | "ods" => "Spreadsheet".to_string(),
        "" => "Text".to_string(),
        other => other.to_uppercase(),
    }
}

/// The text of the XML parts of an Office file, a paragraph to a line.
fn office_text(
    bytes: &[u8],
    parts: impl Fn(&str) -> bool,
    paragraph: &str,
    text_tag: &str,
) -> Result<String, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| "The file could not be opened.".to_string())?;
    // A name that can't be read can't be one of the parts, so it is passed over.
    let mut names: Vec<String> = archive
        .file_names()
        .filter_map(Result::ok)
        .filter(|name| parts(name))
        .map(|name| name.into_owned())
        .collect();
    // Slides in order: slide2 before slide10.
    names.sort_by_key(|name| {
        let digits: String = name.chars().filter(char::is_ascii_digit).collect();
        (digits.parse::<u64>().unwrap_or(0), name.clone())
    });
    office_parts(&mut archive, &names, UNPACKED_LIMIT, paragraph, text_tag)
}

/// The named parts' text, stopping when what they unpack to passes `budget`.
fn office_parts<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    names: &[String],
    mut budget: u64,
    paragraph: &str,
    text_tag: &str,
) -> Result<String, String> {
    let too_big = || "The file unpacks to more than the app can read.".to_string();
    let mut out = String::new();
    for (index, name) in names.iter().enumerate() {
        let part = archive
            .by_name(name)
            .map_err(|_| "The file could not be read.".to_string())?;
        let mut bytes = Vec::new();
        // One byte past the budget shows a part that would go over it.
        part.take(budget + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "The file could not be read.".to_string())?;
        if bytes.len() as u64 > budget {
            return Err(too_big());
        }
        budget -= bytes.len() as u64;
        let xml =
            String::from_utf8(bytes).map_err(|_| "The file could not be read.".to_string())?;
        if names.len() > 1 {
            out.push_str(&format!("--- Slide {} ---\n", index + 1));
        }
        out.push_str(&xml_text(&xml, paragraph, text_tag));
        out.push('\n');
    }
    Ok(out)
}

/// What is inside `<text_tag>` elements, with a line break after each
/// paragraph and the five XML escapes undone.
pub fn xml_text(xml: &str, paragraph: &str, text_tag: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    let open = format!("<{text_tag}");
    let close = format!("</{text_tag}>");
    let end_paragraph = format!("</{paragraph}>");
    while let Some(at) = rest.find('<') {
        rest = &rest[at..];
        if rest.starts_with(&end_paragraph) {
            out.push('\n');
            rest = &rest[end_paragraph.len()..];
        } else if rest.starts_with("<w:tab/>") || rest.starts_with("<a:tab/>") {
            out.push('\t');
            rest = &rest[1..];
        } else if rest.starts_with("<w:br/>") || rest.starts_with("<a:br/>") {
            out.push('\n');
            rest = &rest[1..];
        } else if rest.starts_with(&open)
            && matches!(rest.as_bytes().get(open.len()), Some(b'>') | Some(b' '))
        {
            let Some(start) = rest.find('>') else { break };
            if rest[..start].ends_with('/') {
                rest = &rest[start + 1..];
                continue;
            }
            let body = &rest[start + 1..];
            let Some(stop) = body.find(&close) else { break };
            out.push_str(&unescape(&body[..stop]));
            rest = &body[stop + close.len()..];
        } else {
            rest = &rest[1..];
        }
    }
    out
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Each sheet as CSV under its name.
fn sheet_text(bytes: Vec<u8>) -> Result<String, String> {
    use calamine::{Reader, open_workbook_auto_from_rs};
    let mut book = open_workbook_auto_from_rs(Cursor::new(bytes))
        .map_err(|_| "The spreadsheet could not be opened.".to_string())?;
    let mut out = String::new();
    for name in book.sheet_names() {
        let Ok(range) = book.worksheet_range(&name) else {
            continue;
        };
        out.push_str(&format!("## {name}\n"));
        for (index, row) in range.rows().enumerate() {
            if index >= SHEET_ROWS {
                out.push_str(&format!("(only the first {SHEET_ROWS} rows)\n"));
                break;
            }
            let cells: Vec<String> = row
                .iter()
                .map(|cell| {
                    let text = cell.to_string();
                    if text.contains([',', '"', '\n']) {
                        format!("\"{}\"", text.replace('"', "\"\""))
                    } else {
                        text
                    }
                })
                .collect();
            out.push_str(&cells.join(","));
            out.push('\n');
        }
        out.push('\n');
    }
    Ok(out)
}

fn pdf_text(bytes: &[u8]) -> Result<String, String> {
    // A broken PDF can make the reader panic; that is caught here, so it
    // costs one attachment, not the app.
    let bytes = bytes.to_vec();
    std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&bytes))
        .map_err(|_| "This PDF could not be read.".to_string())?
        .map_err(|_| {
            "This PDF could not be read. It may be scanned pages, which have no text to read."
                .to_string()
        })
}

/// The text of `bytes`, read as the kind of file `name` says it is.
pub fn read_bytes(name: &str, bytes: Vec<u8>) -> Result<Attachment, String> {
    let extension = Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();
    if IMAGES.contains(&extension.as_str()) {
        return Err(format!(
            "{name} is an image. Only JPEG, PNG, and WebP pictures can be sent, and only to a model that reads them, such as Phi-3.5 Vision on a Mac."
        ));
    }
    let text = match extension.as_str() {
        "pdf" => pdf_text(&bytes)?,
        "docx" => office_text(&bytes, |part| part == "word/document.xml", "w:p", "w:t")?,
        "pptx" => office_text(
            &bytes,
            |part| part.starts_with("ppt/slides/slide") && part.ends_with(".xml"),
            "a:p",
            "a:t",
        )?,
        "xlsx" | "xlsm" | "xls" | "ods" => sheet_text(bytes)?,
        _ => {
            if bytes.iter().take(8_000).any(|byte| *byte == 0) {
                return Err(format!("{name} is not a kind of file the app can read."));
            }
            String::from_utf8_lossy(&bytes).to_string()
        }
    };
    let text = text.replace("\r\n", "\n");
    let text = text.trim();
    if text.is_empty() {
        return Err(format!("{name} has no text to read."));
    }
    let chars = text.chars().count();
    Ok(Attachment {
        name: name.to_string(),
        kind: kind_of(&extension),
        text: text.chars().take(TEXT_LIMIT).collect(),
        chars,
        cut: chars > TEXT_LIMIT,
    })
}

#[tauri::command]
pub async fn read_attachment(path: String) -> Result<Attachment, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&path);
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let meta = fs::metadata(path).map_err(|_| format!("{name} could not be opened."))?;
        if meta.is_dir() {
            return Err(format!(
                "{name} is a folder. Point a project at it to work with its files."
            ));
        }
        if meta.len() > FILE_LIMIT {
            return Err(format!("{name} is larger than 50 MB."));
        }
        read_bytes(
            &name,
            fs::read(path).map_err(|_| format!("{name} could not be read."))?,
        )
    })
    .await
    .map_err(|_| "The file could not be read.".to_string())?
}

/// The system's own picker, for one file or several. Empty when cancelled.
#[tauri::command]
pub async fn pick_files() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        if !cfg!(target_os = "macos") {
            return Err("Drop the files on the window instead.".to_string());
        }
        let script = "set picked to choose file with prompt \"Attach files\" with multiple selections allowed\n\
                      set out to \"\"\n\
                      repeat with one in picked\n\
                      set out to out & POSIX path of one & linefeed\n\
                      end repeat\n\
                      return out";
        let output = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", script])
            .output()
            .map_err(|_| "Could not open the file picker.".to_string())?;
        Ok(String::from_utf8_lossy(&output.stdout).lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect())
    })
    .await
    .map_err(|_| "Could not open the file picker.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn office(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buffer);
            for (name, body) in parts {
                zip.start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buffer.into_inner()
    }

    #[test]
    fn text_files_are_read_as_they_are() {
        let read = read_bytes("sales.csv", b"region,total\r\nwest,10\r\n".to_vec()).unwrap();
        assert_eq!(read.text, "region,total\nwest,10");
        assert_eq!(read.kind, "CSV");
        assert!(!read.cut);
        assert!(
            read_bytes("blob.bin", vec![0, 1, 2])
                .unwrap_err()
                .contains("not a kind of file")
        );
        assert!(
            read_bytes("photo.PNG", vec![1])
                .unwrap_err()
                .contains("is an image")
        );
        assert!(
            read_bytes("empty.txt", b"  \n".to_vec())
                .unwrap_err()
                .contains("no text")
        );
    }

    #[test]
    fn long_text_is_cut_and_says_so() {
        let read = read_bytes("long.md", "a".repeat(TEXT_LIMIT + 5).into_bytes()).unwrap();
        assert!(read.cut);
        assert_eq!(read.chars, TEXT_LIMIT + 5);
        assert_eq!(read.text.len(), TEXT_LIMIT);
    }

    #[test]
    fn word_and_powerpoint_files_give_their_words() {
        let doc = office(&[(
            "word/document.xml",
            "<w:document><w:body><w:p><w:r><w:t>Fish &amp; chips</w:t></w:r><w:tab/><w:r><w:t xml:space=\"preserve\"> today</w:t></w:r></w:p><w:p><w:r><w:t>Line two</w:t></w:r></w:p></w:body></w:document>",
        )]);
        let read = read_bytes("menu.docx", doc).unwrap();
        assert_eq!(read.text, "Fish & chips\t today\nLine two");
        assert_eq!(read.kind, "Word");
        let deck = office(&[
            (
                "ppt/slides/slide10.xml",
                "<p:sld><a:p><a:r><a:t>Ten</a:t></a:r></a:p></p:sld>",
            ),
            (
                "ppt/slides/slide2.xml",
                "<p:sld><a:p><a:r><a:t>Two</a:t></a:r></a:p></p:sld>",
            ),
        ]);
        assert_eq!(
            read_bytes("deck.pptx", deck).unwrap().text,
            "--- Slide 1 ---\nTwo\n\n--- Slide 2 ---\nTen"
        );
        assert!(read_bytes("broken.docx", b"not a zip".to_vec()).is_err());
    }

    // Spreadsheets are read by calamine, so moving it to a new version is
    // checked here: each sheet comes back as CSV under its name.
    #[test]
    fn a_spreadsheet_gives_each_sheet_as_csv() {
        let book = office(&[
            (
                "_rels/.rels",
                "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>",
            ),
            (
                "xl/workbook.xml",
                "<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"Sales\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>",
            ),
            (
                "xl/_rels/workbook.xml.rels",
                "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>",
            ),
            (
                "xl/worksheets/sheet1.xml",
                "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>Region</t></is></c><c r=\"B1\" t=\"inlineStr\"><is><t>Note</t></is></c></row><row r=\"2\"><c r=\"A2\" t=\"inlineStr\"><is><t>West</t></is></c><c r=\"B2\" t=\"inlineStr\"><is><t>Fish, \"fresh\"</t></is></c></row><row r=\"3\"><c r=\"A3\"><v>10</v></c></row></sheetData></worksheet>",
            ),
        ]);
        let read = read_bytes("sales.xlsx", book).unwrap();
        assert_eq!(
            read.text,
            "## Sales\nRegion,Note\nWest,\"Fish, \"\"fresh\"\"\"\n10,"
        );
        assert_eq!(read.kind, "Spreadsheet");
        assert!(read_bytes("broken.xlsx", b"not a zip".to_vec()).is_err());
    }

    #[test]
    fn an_office_file_that_unpacks_too_far_is_turned_away() {
        let body = format!("<w:p><w:t>{}</w:t></w:p>", "a".repeat(1_000));
        let doc = office(&[
            ("ppt/slides/slide1.xml", body.as_str()),
            ("ppt/slides/slide2.xml", body.as_str()),
        ]);
        let mut archive = zip::ZipArchive::new(Cursor::new(doc)).unwrap();
        let names = vec![
            "ppt/slides/slide1.xml".to_string(),
            "ppt/slides/slide2.xml".to_string(),
        ];
        assert!(office_parts(&mut archive, &names, 4_000, "w:p", "w:t").is_ok());
        let error = office_parts(&mut archive, &names, 1_500, "w:p", "w:t").unwrap_err();
        assert!(error.contains("unpacks to more"), "{error}");
    }

    #[test]
    fn a_broken_pdf_is_turned_away_without_a_crash() {
        assert!(
            read_bytes("broken.pdf", b"%PDF-1.4 nonsense".to_vec())
                .unwrap_err()
                .contains("PDF could not be read")
        );
    }
}
