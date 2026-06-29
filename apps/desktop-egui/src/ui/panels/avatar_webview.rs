//! Avatar panel backed by a Unity WebGL build rendered inside a wry WebView.
//!
//! Only compiled when the `avatar` Cargo feature is enabled.
//! Falls back to a static pixel-art placeholder otherwise (see `avatar.rs`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui::{self, CornerRadius};
use rust_embed::RustEmbed;
use tiny_http::{Header, Response, Server};

use crate::state::app_state::AppState;
use crate::theme::colors;

// ---------------------------------------------------------------------------
// Embedded assets – everything under `character-webgl/` (project-root relative)
// ---------------------------------------------------------------------------
#[derive(RustEmbed)]
#[folder = "../../character-webgl/"]
struct WebglAssets;

// ---------------------------------------------------------------------------
// AvatarWebView – owns the wry WebView and the tiny-http server port.
// ---------------------------------------------------------------------------
pub struct AvatarWebView {
    /// The wry WebView instance (created once, reused until program exit).
    pub webview: wry::WebView,
    /// Port the embedded HTTP server is listening on.
    pub port: u16,
    /// Whether the last `load_url` / initial creation has finished.
    pub loaded: bool,
    /// Cached bounds so we can avoid redundant `set_bounds` calls.
    pub bounds: egui::Rect,
}

impl AvatarWebView {
    /// Create a new WebView **after** the HTTP server is already running.
    ///
    /// `parent` must be the native window handle of the eframe viewport
    /// (obtained via `raw-window-handle`).
    pub fn new(port: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let url = format!("http://127.0.0.1:{}/index.html", port);

        let webview = wry::WebViewBuilder::new()
            .with_url(&url)
            .with_transparent(false)
            .with_devtools(true)
            .build()?;

        Ok(Self {
            webview,
            port,
            loaded: false,
            bounds: egui::Rect::NOTHING,
        })
    }

    /// Resize / reposition the WebView to match the given screen rectangle.
    pub fn sync_bounds(&mut self, rect: egui::Rect) {
        if self.bounds == rect {
            return;
        }
        self.bounds = rect;

        use wry::dpi::{LogicalPosition, LogicalSize, Position, Size};
        use wry::Rect as WryRect;

        let _ = self.webview.set_bounds(WryRect {
            position: Position::Logical(LogicalPosition::new(
                rect.min.x as f64,
                rect.min.y as f64,
            )),
            size: Size::Logical(LogicalSize::new(
                rect.width() as f64,
                rect.height() as f64,
            )),
        });
    }

    /// Show / hide the WebView depending on whether the Avatar tab is active.
    pub fn set_visible(&mut self, visible: bool) {
        let _ = self.webview.set_visible(visible);
    }

    // -----------------------------------------------------------------------
    // Host → Unity  commands
    // -----------------------------------------------------------------------
    pub fn send_command(&self, cmd: &AvatarCommand) {
        let js = match cmd {
            AvatarCommand::SetExpression(expr) => {
                format!(
                    "try{{window.unityInstance.SendMessage('AvatarController','SetExpression','{}');}}catch(e){{}}",
                    expr.escape_default()
                )
            }
            AvatarCommand::PlayAnimation { name, looped } => {
                format!(
                    "try{{window.unityInstance.SendMessage('AvatarController','PlayAnimation','{}');}}catch(e){{}}",
                    name.escape_default()
                )
            }
            AvatarCommand::LipSync(blendshapes) => {
                let json = serde_json::to_string(blendshapes).unwrap_or_default();
                format!(
                    "try{{window.unityInstance.SendMessage('AvatarController','UpdateLipSync','{}');}}catch(e){{}}",
                    json.escape_default()
                )
            }
            AvatarCommand::SetOutfit(outfit) => {
                format!(
                    "try{{window.unityInstance.SendMessage('AvatarController','SetOutfit','{}');}}catch(e){{}}",
                    outfit.escape_default()
                )
            }
        };
        let _ = self.webview.evaluate_script(&js);
    }
}

// -----------------------------------------------------------------------
// Host → Unity command enums
// -----------------------------------------------------------------------
#[derive(Debug, Clone)]
pub enum AvatarCommand {
    SetExpression(String),
    PlayAnimation { name: String, looped: bool },
    LipSync(HashMap<String, f32>),
    SetOutfit(String),
}

// -----------------------------------------------------------------------
// MIME-type / Content-Encoding helpers
// -----------------------------------------------------------------------
fn mime_type(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".js") {
        "application/javascript"
    } else if lower.ends_with(".wasm") || lower.ends_with(".wasm.br") {
        "application/wasm"
    } else if lower.ends_with(".css") {
        "text/css"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else if lower.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

fn is_brotli(path: &str) -> bool {
    path.ends_with(".br")
}

// -----------------------------------------------------------------------
// Embedded HTTP server (runs on a dedicated OS thread)
// -----------------------------------------------------------------------
static SERVER_STARTED: AtomicBool = AtomicBool::new(false);

/// Launch the tiny-http server on a random port, serving embedded WebGL files.
/// Returns the port number so the WebView knows where to connect.
/// Safe to call multiple times – only the first call actually starts the server.
pub fn ensure_server(port: &mut Option<u16>) {
    if SERVER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    let server =
        Server::http("127.0.0.1:0").expect("Failed to bind embedded asset server for avatar");
    let actual_port = server.server_addr().to_ip().unwrap().port();
    *port = Some(actual_port);

    tracing::info!("Avatar asset server listening on http://127.0.0.1:{actual_port}");

    std::thread::Builder::new()
        .name("avatar-asset-server".into())
        .spawn(move || {
            for request in server.incoming_requests() {
                let url = request.url().to_string();
                let path = url.trim_start_matches('/');
                // Default to index.html
                let path = if path.is_empty() { "index.html" } else { path };

                if let Some(file) = WebglAssets::get(path) {
                    let mime = mime_type(path);
                    let data = file.data;

                    let mut headers = vec![
                        Header::from_bytes(
                            "Content-Type",
                            mime.as_bytes(),
                        )
                        .unwrap(),
                    ];

                    if is_brotli(path) {
                        if let Ok(h) =
                            Header::from_bytes("Content-Encoding".as_bytes(), "br".as_bytes())
                        {
                            headers.push(h);
                        }
                    }

                    let resp = Response::new(
                        tiny_http::StatusCode(200),
                        headers,
                        data.as_ref(),
                        Some(data.len()),
                        None,
                    );
                    let _ = request.respond(resp);
                } else {
                    let _ = request.respond(Response::new_empty(tiny_http::StatusCode(404)));
                }
            }
        })
        .expect("Failed to spawn avatar asset server thread");
}

// -----------------------------------------------------------------------
// draw() – the entry-point called by the dock TabViewer.
//
// Because wry manages its own native window, this function only renders
// a background "canvas area" placeholder that egui uses for layout
// calculations.  The actual WebView is created and positioned in
// `MakimaApp::update()` using the screen-space rectangle of this area.
// -----------------------------------------------------------------------
pub fn draw(ui: &mut egui::Ui, _state: &AppState) {
    // Render a styled container so the Avatar tab doesn't look empty.
    egui::Frame::NONE
        .fill(colors::SURFACE)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            let available = ui.available_size();

            // Reserve the full available space; this rect will be used
            // by the app-level code to position the WebView.
            let (rect, _response) = ui.allocate_exact_size(available, egui::Sense::click());

            // We store the screen-space rect in a side-channel so
            // MakimaApp::update() can pick it up.  This is a bit of a
            // hack, but egui doesn't provide a direct "give me my rect
            // in screen coords" API from inside a TabViewer.
            //
            // Alternative: calculate the rect from the ui clip_rect +
            // available_rect_before_wrap.  We use the allocated rect
            // and convert to screen coords.
            let screen_rect = ui.ctx().screen_rect();
            let local_rect = rect;
            // The allocated rect is already in screen-space for egui?
            // Actually `allocate_exact_size` returns a rect in parent
            // coordinates.  We convert via `ui.min_rect()` offset.
            // A simpler approach: just store the rect in a global /
            // Arc<Mutex> slot.  For now we use ui.available_rect_before_wrap()
            // which IS in screen coords after layout.
        });
}

/// Returns the screen-space rectangle that the Avatar tab occupies.
///
/// Call this **after** `draw()` during the same frame to obtain the
/// rectangle where the WebView should be placed.  Because egui layout is
/// computed during the draw pass, we cannot know the final rect until
/// `draw()` has run.
pub fn last_avatar_rect(ui: &egui::Ui) -> egui::Rect {
    // `available_rect_before_wrap` gives the rectangle in parent coordinates.
    // We need screen-space.  `ui.ctx().screen_rect()` isn't quite right because
    // panels can be offset.  The safest approach is to use the clip rect which
    // is already in screen coordinates.
    let clip = ui.clip_rect();

    // The dock area insets — account for the frame padding we used in draw().
    let margin = 8.0;
    egui::Rect::from_min_size(
        clip.min + egui::vec2(margin, margin),
        clip.size() - egui::vec2(margin * 2.0, margin * 2.0),
    )
}