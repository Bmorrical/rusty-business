//! Model downloads delegated to a private-network llama-server router.

use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::watch;

use crate::downloads::{DownloadReport, Progress, cancelled};
use crate::engine::{self, ModelList, ModelRow};
use crate::gate::Gate;

const POLL: Duration = Duration::from_secs(2);
const REQUEST_WAIT: Duration = Duration::from_secs(15);
const START_WAIT: Duration = Duration::from_secs(60);
const UNSUPPORTED: &str = "This LAN server does not support downloads. Use a recent llama-server in router mode, or install the GGUF on the server and refresh the app.";

#[derive(Deserialize)]
struct Acknowledgment {
    success: bool,
}

async fn models(client: &Client, url: &str, refresh: bool) -> Result<ModelList, String> {
    let mut request = client.get(url).timeout(REQUEST_WAIT);
    if refresh {
        request = request.query(&[("reload", "1")]);
    }
    let response = request.send().await.map_err(|_| "The app lost contact with the LAN server. Check the server before retrying the download.".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "The LAN server returned HTTP {} while listing its models.",
            response.status().as_u16()
        ));
    }
    response
        .json()
        .await
        .map_err(|_| "The LAN server's model list could not be read.".to_string())
}

async fn send(client: &Client, url: &str, model: &str, start: bool) -> Result<(), String> {
    let (action, timeout, body) = if start {
        (
            "start",
            START_WAIT,
            json!({ "model": model, "sidecars": [] }),
        )
    } else {
        ("pause", REQUEST_WAIT, json!({ "model": model }))
    };
    let unknown = || {
        format!(
            "The LAN server did not confirm the {action} request. Check the server to see if the download is running."
        )
    };
    let response = client
        .post(url)
        .timeout(timeout)
        .json(&body)
        .send()
        .await
        .map_err(|_| unknown())?;
    let status = response.status();
    if start && matches!(status.as_u16(), 404 | 405 | 501) {
        return Err(UNSUPPORTED.to_string());
    }
    if !status.is_success() {
        let error = response
            .json::<serde_json::Value>()
            .await
            .unwrap_or_default();
        let detail: String = error["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(250)
            .collect();
        return Err(format!(
            "The LAN server could not {action} the download (HTTP {}). {detail}",
            status.as_u16()
        )
        .trim_end()
        .to_string());
    }
    match response.json::<Acknowledgment>().await {
        Ok(reply) if reply.success => Ok(()),
        _ if start => Err(UNSUPPORTED.to_string()),
        _ => Err(unknown()),
    }
}

fn matching<'a>(list: &'a ModelList, model: &str) -> Option<&'a ModelRow> {
    list.data
        .iter()
        .find(|row| row.id.eq_ignore_ascii_case(model))
}

fn progress(row: &ModelRow) -> Progress {
    let mut progress = Progress { done: 0, total: 0 };
    if let Some(files) = row.status["progress"].as_object() {
        for file in files.values() {
            progress.done = progress
                .done
                .saturating_add(file["done"].as_u64().unwrap_or(0));
            progress.total = progress
                .total
                .saturating_add(file["total"].as_u64().unwrap_or(0));
        }
    }
    progress
}

pub(crate) async fn download(
    base: &str,
    model: &str,
    gate: &Gate,
    cancel: &mut watch::Receiver<bool>,
    mut report: impl FnMut(Progress),
) -> Result<DownloadReport, String> {
    let client = engine::client()?;
    let root = base.strip_suffix("/v1").unwrap_or(base);
    let url = format!("{root}/models");
    let whole = DownloadReport {
        whole: true,
        stopped: false,
    };
    let stopped = DownloadReport {
        whole: false,
        stopped: true,
    };
    let list = models(&client, &url, false).await?;
    let current = matching(&list, model);
    if current.is_some_and(ModelRow::available) {
        return Ok(whole);
    }
    if !gate.is_open() || *cancel.borrow() {
        return Ok(stopped);
    }
    if current.is_none_or(|row| row.status() != "downloading") {
        send(&client, &url, model, true).await?;
    }

    loop {
        tokio::select! {
            biased;
            _ = cancelled(cancel) => {
                let list = models(&client, &url, false).await?;
                if let Some(row) = matching(&list, model) {
                    if row.available() {
                        return Ok(whole);
                    }
                    if row.status() == "downloading" {
                        send(&client, &format!("{url}/unload"), model, false).await?;
                    }
                }
                return Ok(stopped);
            }
            _ = tokio::time::sleep(POLL) => {}
        }
        let list = models(&client, &url, false).await?;
        if let Some(row) = matching(&list, model) {
            if row.available() {
                return Ok(whole);
            }
            if row.status() == "downloading" {
                report(progress(row));
                continue;
            }
        }
        let refreshed = models(&client, &url, true).await?;
        if matching(&refreshed, model).is_some_and(ModelRow::available) {
            return Ok(whole);
        }
        return Err("The LAN server did not finish installing the model. Check its logs, then retry the download.".to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    const MODEL: &str = "microsoft/phi-4-gguf:Q4_K";

    struct Exchange<'a> {
        request: &'a str,
        body: Option<serde_json::Value>,
        status: u16,
        reply: serde_json::Value,
    }

    fn router(exchanges: Vec<Exchange<'static>>) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            for exchange in exchanges {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(REQUEST_WAIT)).unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert_eq!(line.trim_end(), format!("{} HTTP/1.1", exchange.request));
                let mut length = 0;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                if let Some(expected) = exchange.body {
                    let mut body = vec![0; length];
                    reader.read_exact(&mut body).unwrap();
                    assert_eq!(
                        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
                        expected
                    );
                }
                let reply = exchange.reply.to_string();
                write!(
                    stream,
                    "HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    exchange.status,
                    reply.len(),
                ).unwrap();
            }
        });
        (base, thread)
    }

    fn list(request: &'static str, value: &str) -> Exchange<'static> {
        let data = if value.is_empty() {
            json!([])
        } else {
            json!([{ "id": MODEL, "status": {
                "value": value, "progress": { "file.gguf": { "done": 256, "total": 512 } }
            }}])
        };
        Exchange {
            request,
            body: None,
            status: 200,
            reply: json!({ "data": data }),
        }
    }

    fn start(status: u16) -> Exchange<'static> {
        Exchange {
            request: "POST /models",
            body: Some(json!({ "model": MODEL, "sidecars": [] })),
            status,
            reply: json!({ "success": status == 200 }),
        }
    }

    #[test]
    fn a_remote_install_tracks_progress_and_waits_until_the_model_is_listed() {
        let (base, thread) = router(vec![
            list("GET /models", ""),
            start(200),
            list("GET /models", "downloading"),
            list("GET /models", "downloaded"),
            list("GET /models?reload=1", "unloaded"),
        ]);
        let (_stop, mut cancel) = watch::channel(false);
        let mut updates = Vec::new();
        let result = tauri::async_runtime::block_on(download(
            &base,
            MODEL,
            &Gate::at(true),
            &mut cancel,
            |progress| updates.push(progress),
        ))
        .unwrap();
        assert!(result.whole);
        assert_eq!(
            updates,
            [Progress {
                done: 256,
                total: 512
            }]
        );
        thread.join().unwrap();
    }

    #[test]
    fn a_chat_only_server_explains_how_to_install() {
        let (base, thread) = router(vec![list("GET /models", ""), start(404)]);
        let (_stop, mut cancel) = watch::channel(false);
        let result = tauri::async_runtime::block_on(download(
            &base,
            MODEL,
            &Gate::at(true),
            &mut cancel,
            |_| {},
        ));
        assert_eq!(result.unwrap_err(), UNSUPPORTED);
        thread.join().unwrap();
    }

    #[test]
    fn pause_uses_unload_and_leaves_the_servers_partial_file() {
        let (base, thread) = router(vec![
            list("GET /models", "downloading"),
            list("GET /models", "downloading"),
            list("GET /models", "downloading"),
            Exchange {
                request: "POST /models/unload",
                body: Some(json!({ "model": MODEL })),
                status: 200,
                reply: json!({ "success": true }),
            },
        ]);
        let (stop, mut cancel) = watch::channel(false);
        let result = tauri::async_runtime::block_on(download(
            &base,
            MODEL,
            &Gate::at(true),
            &mut cancel,
            |_| {
                stop.send(true).unwrap();
            },
        ))
        .unwrap();
        assert!(result.stopped);
        thread.join().unwrap();
    }
}
