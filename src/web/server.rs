/*  This file is part of the CodeDiff code diffing tool.
 *
 *  Copyright (C) 2026 Marko Ivankovic
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published
 *  by the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
//! The server: one listener, one [`Session`] behind a mutex, one task per connection, and a
//! route table. Diffs run on a blocking thread through the same `compute_diff_with_options` the
//! TUI and headless mode call, wrapped in the same `catch_unwind`.
//!
//! Two checks guard every request, because a server on localhost is reachable by every page the
//! user has open in the same browser, not just this one:
//!
//! 1. **`Host` must name this server.** Otherwise a hostile page could point a name it controls
//!    at 127.0.0.1 (DNS rebinding), load `/` as if it were same-origin, and read the token out of
//!    it.
//! 2. **Every `/api/` call must carry the per-run token** the page received in `/`. Any page can
//!    *send* a request to localhost; the token is in a custom header, which makes the browser ask
//!    permission first (a CORS preflight) - and this server never grants it. So a foreign page can
//!    neither read a diff nor make the session diff, quit, or launch an editor.
//!
//! The API is `POST` + JSON throughout, including reads: one shape, one parser, and no endpoint
//! that a plain `<img src>` or link could trigger.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use serde::Deserialize;
use tokio::io::BufReader;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

use crate::diff::text::RenderOptions;
use crate::tui::app::compute_diff_with_options;
use crate::web::http::{HttpError, Request, Response, read_request};
use crate::web::session::{
    DiffJob, FinishOutcome, RenderOptionsOutcome, Session, SettingsUpdate, list_directory,
    run_editor,
};

const INDEX_HTML: &str = include_str!("../../assets/web/index.html");
const APP_JS: &str = include_str!("../../assets/web/app.js");
const MODEL_JS: &str = include_str!("../../assets/web/model.js");
const STYLE_CSS: &str = include_str!("../../assets/web/style.css");

/// The placeholder in `index.html` the per-run token replaces.
const TOKEN_PLACEHOLDER: &str = "__CODEDIFF_TOKEN__";
/// The header the page sends the token in. A custom header on purpose - see the module comment.
pub const TOKEN_HEADER: &str = "X-Codediff-Token";

struct Context {
    session: Mutex<Session>,
    token: String,
    host: String,
    port: u16,
    shutdown: Notify,
}

pub struct Server {
    listener: TcpListener,
    context: Arc<Context>,
}

/// 128 bits from the standard library's own randomly-keyed hasher: the process gets fresh SipHash
/// keys from the OS at startup, and two hashes of a fixed input under two such keys are as good a
/// token as this needs without a `rand` dependency (which is `stats`-only here on purpose).
fn fresh_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut token = String::with_capacity(32);
    for _ in 0..2 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write(b"codediff-web");
        token.push_str(&format!("{:016x}", hasher.finish()));
    }
    token
}

/// Whether a `Host` header names this server: a loopback name or the bound host, at the bound
/// port. No header at all is refused too - every browser sends one.
pub fn host_allowed(header: Option<&str>, bound_host: &str, bound_port: u16) -> bool {
    let Some(header) = header else {
        return false;
    };
    let (name, port) = match header.rsplit_once(':') {
        // `[::1]:8080` splits at the last colon; a bare IPv6 literal without a port has more
        // colons but no brackets-then-port shape, so it lands in the no-port arm below.
        Some((name, port)) if !name.ends_with(']') || name.starts_with('[') => {
            match port.parse::<u16>() {
                Ok(port) => (name, Some(port)),
                Err(_) => return false,
            }
        }
        _ => (header, None),
    };
    let name_ok = matches!(name, "localhost" | "127.0.0.1" | "[::1]") || name == bound_host;
    let port_ok = port.unwrap_or(80) == bound_port;
    name_ok && port_ok
}

impl Server {
    pub async fn bind(host: &str, port: u16, session: Session) -> Result<Self> {
        let listener = TcpListener::bind((host, port))
            .await
            .with_context(|| format!("cannot listen on {host}:{port}"))?;
        let bound = listener.local_addr()?.port();
        Ok(Self {
            listener,
            context: Arc::new(Context {
                session: Mutex::new(session),
                token: fresh_token(),
                host: host.to_string(),
                port: bound,
                shutdown: Notify::new(),
            }),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    pub fn url(&self) -> String {
        format!("http://{}:{}/", self.context.host, self.context.port)
    }

    /// Serves until the page asks to quit (`q`) or the process gets Ctrl-C.
    pub async fn run(self) -> Result<()> {
        loop {
            tokio::select! {
                accepted = self.listener.accept() => {
                    let (stream, _) = accepted?;
                    let context = Arc::clone(&self.context);
                    tokio::spawn(async move {
                        if let Err(err) = handle_connection(stream, context).await {
                            eprintln!("codediff-web: {err}");
                        }
                    });
                }
                _ = self.context.shutdown.notified() => return Ok(()),
                _ = tokio::signal::ctrl_c() => return Ok(()),
            }
        }
    }
}

async fn handle_connection(stream: TcpStream, context: Arc<Context>) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let request = match read_request(&mut reader).await {
        Ok(Some(request)) => request,
        Ok(None) => return Ok(()),
        Err(HttpError::TooLarge) => {
            return Ok(Response::text(413, "request too large")
                .write_to(&mut write_half)
                .await?);
        }
        Err(HttpError::Malformed(what)) => {
            return Ok(Response::text(400, what).write_to(&mut write_half).await?);
        }
        Err(HttpError::Io(err)) => return Err(err.into()),
    };
    let (response, quit) = route(&context, request).await;
    response.write_to(&mut write_half).await?;
    if quit {
        context.shutdown.notify_one();
    }
    Ok(())
}

#[derive(Deserialize)]
struct DiffRequest {
    before: String,
    after: String,
}

#[derive(Deserialize)]
struct HighlightRequest {
    syntax_theme: String,
}

#[derive(Deserialize)]
struct ListRequest {
    path: Option<String>,
}

#[derive(Deserialize)]
struct EditRequest {
    panel: String,
    line: usize,
}

#[derive(Deserialize)]
struct ReviewRequest {
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct ReviewOpenRequest {
    root: String,
    target: crate::review::ReviewTarget,
}

fn body<T: for<'de> Deserialize<'de>>(request: &Request) -> Result<T, Response> {
    serde_json::from_slice(&request.body)
        .map_err(|err| Response::error(400, format!("bad request body: {err}")))
}

/// The response, and whether the server should stop once it has been sent.
async fn route(context: &Context, request: Request) -> (Response, bool) {
    if !host_allowed(request.header("Host"), &context.host, context.port) {
        return (Response::text(403, "unexpected Host header"), false);
    }
    if request.path.starts_with("/api/") {
        if request.header(TOKEN_HEADER) != Some(context.token.as_str()) {
            return (
                Response::error(403, "missing or wrong session token"),
                false,
            );
        }
        if request.method != "POST" {
            return (Response::error(405, "the API is POST only"), false);
        }
        return api(context, &request).await;
    }
    if request.method != "GET" {
        return (Response::text(405, "GET only"), false);
    }
    let response = match request.path.as_str() {
        "/" => Response::html(INDEX_HTML.replace(TOKEN_PLACEHOLDER, &context.token)),
        "/app.js" => Response::asset("text/javascript; charset=utf-8", APP_JS),
        "/model.js" => Response::asset("text/javascript; charset=utf-8", MODEL_JS),
        "/style.css" => Response::asset("text/css; charset=utf-8", STYLE_CSS),
        _ => Response::text(404, "not found"),
    };
    (response, false)
}

async fn api(context: &Context, request: &Request) -> (Response, bool) {
    let response = match request.path.as_str() {
        "/api/state" => Response::json(200, &lock(context).state()),
        "/api/diff" => match body::<DiffRequest>(request) {
            Ok(req) => {
                let job = lock(context).begin_diff(req.before.into(), req.after.into());
                compute(context, job).await
            }
            Err(response) => response,
        },
        "/api/cancel" => {
            lock(context).cancel_diff();
            Response::json(200, &serde_json::json!({ "ok": true }))
        }
        "/api/render_options" => match body::<RenderOptions>(request) {
            Ok(options) => {
                // Bound first so the guard is released before the await below: a `match` on
                // the call directly would hold the lock across the whole computation.
                let outcome = lock(context).set_render_options(options);
                match outcome {
                    RenderOptionsOutcome::Recompute(job) => compute(context, job).await,
                    RenderOptionsOutcome::Filtered(payload) => Response::json(200, &payload),
                }
            }
            Err(response) => response,
        },
        "/api/highlight" => match body::<HighlightRequest>(request) {
            Ok(req) => {
                let payload = lock(context).highlight(&req.syntax_theme);
                Response::json(200, &payload)
            }
            Err(response) => response,
        },
        "/api/settings" => match body::<SettingsUpdate>(request) {
            Ok(update) => {
                lock(context).apply_settings(update);
                Response::json(200, &serde_json::json!({ "ok": true }))
            }
            Err(response) => response,
        },
        "/api/ls" => match body::<ListRequest>(request) {
            Ok(req) => {
                let dir = match req.path {
                    Some(path) => std::path::PathBuf::from(path),
                    None => std::env::current_dir().unwrap_or_else(|_| "/".into()),
                };
                let listing = tokio::task::spawn_blocking(move || list_directory(&dir)).await;
                match listing {
                    Ok(listing) => Response::json(200, &listing),
                    Err(err) => Response::error(500, err.to_string()),
                }
            }
            Err(response) => response,
        },
        "/api/edit" => match body::<EditRequest>(request) {
            Ok(req) => {
                let path = lock(context).path_for(&req.panel).map(|p| p.to_path_buf());
                match path {
                    Some(path) => {
                        let line = req.line.max(1);
                        let result =
                            tokio::task::spawn_blocking(move || run_editor(&path, line)).await;
                        match result {
                            Ok(Ok(())) => Response::json(200, &serde_json::json!({ "ok": true })),
                            Ok(Err(message)) => Response::error(500, message),
                            Err(err) => Response::error(500, err.to_string()),
                        }
                    }
                    None => Response::error(400, "no file is open in that panel"),
                }
            }
            Err(response) => response,
        },
        "/api/review" => match body::<ReviewRequest>(request) {
            Ok(req) => {
                let limit = req.limit.unwrap_or(crate::review::DEFAULT_COMMIT_LIMIT);
                let loaded = tokio::task::spawn_blocking(move || Session::load_review(limit)).await;
                match loaded {
                    Ok(Ok(review)) => Response::json(200, &review),
                    Ok(Err(err)) => Response::error(400, format!("{err:#}")),
                    Err(err) => Response::error(500, err.to_string()),
                }
            }
            Err(response) => response,
        },
        "/api/review/open" => match body::<ReviewOpenRequest>(request) {
            Ok(req) => {
                let root = std::path::PathBuf::from(req.root);
                let job = lock(context).open_review_target(&root, &req.target);
                match job {
                    Ok(job) => compute(context, job).await,
                    Err(err) => Response::error(400, format!("{err:#}")),
                }
            }
            Err(response) => response,
        },
        "/api/quit" => {
            return (
                Response::json(200, &serde_json::json!({ "ok": true })),
                true,
            );
        }
        _ => Response::error(404, "no such endpoint"),
    };
    (response, false)
}

fn lock(context: &Context) -> std::sync::MutexGuard<'_, Session> {
    // A panic while holding the lock would poison it; the session has no invariant a half-applied
    // update could break that the next request cannot recover from, so carry on with it.
    context
        .session
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        message.to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// Runs `job` off the async threads and stores its result if it is still wanted.
async fn compute(context: &Context, job: DiffJob) -> Response {
    let generation = job.generation;
    let computed = tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            compute_diff_with_options(&job.before, &job.after, job.options)
        }))
    })
    .await;
    let outcome = match computed {
        Ok(Ok(Ok(result))) => Ok(result),
        Ok(Ok(Err(err))) => Err(err.to_string()),
        Ok(Err(panic)) => Err(format!(
            "internal error while diffing: {}",
            panic_message(&panic)
        )),
        Err(join) => Err(format!("diff thread failed: {join}")),
    };
    match lock(context).finish_diff(generation, outcome) {
        FinishOutcome::Stored(payload) => Response::json(200, &payload),
        FinishOutcome::Failed(message) => Response::error(500, message),
        FinishOutcome::Stale => Response::error(409, "superseded by a newer diff"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn host_header_must_name_this_server_at_its_port() {
        assert!(host_allowed(Some("127.0.0.1:8080"), "127.0.0.1", 8080));
        assert!(host_allowed(Some("localhost:8080"), "127.0.0.1", 8080));
        assert!(host_allowed(Some("[::1]:8080"), "127.0.0.1", 8080));
        assert!(host_allowed(Some("mybox:8080"), "mybox", 8080));
        assert!(
            host_allowed(Some("localhost"), "127.0.0.1", 80),
            "no port means 80"
        );
        assert!(!host_allowed(Some("localhost"), "127.0.0.1", 8080));
        assert!(!host_allowed(Some("127.0.0.1:8081"), "127.0.0.1", 8080));
        assert!(!host_allowed(Some("evil.example:8080"), "127.0.0.1", 8080));
        assert!(!host_allowed(Some("127.0.0.1:notaport"), "127.0.0.1", 8080));
        assert!(!host_allowed(None, "127.0.0.1", 8080));
    }

    #[test]
    fn tokens_are_long_and_differ_between_calls() {
        let a = fresh_token();
        let b = fresh_token();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn the_page_carries_the_placeholder_the_server_fills() {
        assert!(INDEX_HTML.contains(TOKEN_PLACEHOLDER));
        assert!(
            APP_JS.contains(TOKEN_HEADER),
            "the page must send the token back"
        );
    }

    /// The footer hint line is the one string the page hardcodes rather than fetches; this keeps
    /// it the TUI's, character for character.
    #[test]
    fn the_pages_footer_hints_are_the_tuis() {
        let quoted = format!("{:?}", crate::tui::app::FOOTER_HINTS);
        assert!(
            MODEL_JS.contains(&quoted),
            "model.js must carry {quoted} verbatim"
        );
    }

    async fn start() -> (String, u16, tokio::task::JoinHandle<Result<()>>) {
        let server = Server::bind("127.0.0.1", 0, Session::from_config(None))
            .await
            .unwrap();
        let port = server.local_addr().unwrap().port();
        let token = server.context.token.clone();
        (token, port, tokio::spawn(server.run()))
    }

    async fn exchange(port: u16, raw: String) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        stream.read_to_end(&mut out).await.unwrap();
        String::from_utf8(out).unwrap()
    }

    #[tokio::test]
    async fn the_index_is_served_with_the_token_and_the_api_demands_it_back() {
        let (token, port, _server) = start().await;
        let page = exchange(
            port,
            format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
        )
        .await;
        assert!(page.starts_with("HTTP/1.1 200"));
        assert!(page.contains(&token));
        assert!(!page.contains(TOKEN_PLACEHOLDER));

        let refused = exchange(
            port,
            format!(
                "POST /api/state HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\n\r\n"
            ),
        )
        .await;
        assert!(refused.starts_with("HTTP/1.1 403"), "{refused}");

        let state = exchange(
            port,
            format!(
                "POST /api/state HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{TOKEN_HEADER}: {token}\r\nContent-Length: 0\r\n\r\n"
            ),
        )
        .await;
        assert!(state.starts_with("HTTP/1.1 200"), "{state}");
        assert!(state.contains("\"single_panel_threshold\""));

        let wrong_host = exchange(
            port,
            "GET / HTTP/1.1\r\nHost: evil.example\r\n\r\n".to_string(),
        )
        .await;
        assert!(wrong_host.starts_with("HTTP/1.1 403"));

        let get_api = exchange(
            port,
            format!("GET /api/state HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{TOKEN_HEADER}: {token}\r\n\r\n"),
        )
        .await;
        assert!(get_api.starts_with("HTTP/1.1 405"));

        let missing = exchange(
            port,
            format!("GET /nope HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
        )
        .await;
        assert!(missing.starts_with("HTTP/1.1 404"));
    }

    #[tokio::test]
    async fn a_diff_round_trips_and_quit_stops_the_server() {
        let (token, port, server) = start().await;
        let dir = tempfile::tempdir().unwrap();
        let before = dir.path().join("before.rs");
        let after = dir.path().join("after.rs");
        std::fs::write(&before, "fn a() {}\n").unwrap();
        std::fs::write(&after, "fn b() {}\n").unwrap();
        let body = serde_json::json!({ "before": before, "after": after }).to_string();
        let reply = exchange(
            port,
            format!(
                "POST /api/diff HTTP/1.1\r\nHost: localhost:{port}\r\n{TOKEN_HEADER}: {token}\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        let json_start = reply.find("\r\n\r\n").unwrap() + 4;
        let payload: serde_json::Value = serde_json::from_str(&reply[json_start..]).unwrap();
        assert_eq!(payload["before"]["language"], "Rust");
        assert_eq!(payload["before"]["lines"][0], "fn a() {}");
        assert!(
            payload["before"]["ranges"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["op"] == "update")
        );

        let bad = exchange(
            port,
            format!(
                "POST /api/diff HTTP/1.1\r\nHost: localhost:{port}\r\n{TOKEN_HEADER}: {token}\r\nContent-Length: 2\r\n\r\n{{}}"
            ),
        )
        .await;
        assert!(bad.starts_with("HTTP/1.1 400"), "{bad}");

        let quit = exchange(
            port,
            format!("POST /api/quit HTTP/1.1\r\nHost: localhost:{port}\r\n{TOKEN_HEADER}: {token}\r\nContent-Length: 0\r\n\r\n"),
        )
        .await;
        assert!(quit.starts_with("HTTP/1.1 200"));
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .expect("the server stops after quit")
            .unwrap()
            .unwrap();
    }
}
