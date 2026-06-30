//! Avatar panel backed by a Unity WebGL build rendered inside a wry WebView.
//!
//! Only compiled when the `avatar` Cargo feature is enabled.
//! Falls back to a static pixel-art placeholder otherwise (see `avatar.rs`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, CornerRadius};
use rust_embed::RustEmbed;
use tiny_http::{Header, Response, Server};

use crate::state::app_state::AppState;
use crate::theme::colors;

#[derive(RustEmbed)]
#[folder = "../../character-webgl/"]
struct WebglAssets;

pub struct AvatarWebView {
    pub webview: wry::WebView,
    pub port: u16,
    pub loaded: bool,
    pub bounds: egui::Rect,
}

impl AvatarWebView {
    pub fn new(
        port: u16,
        window: &impl raw_window_handle::HasWindowHandle,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let url = format!("http://127.0.0.1:{}/index.html", port);

        let webview = wry::WebViewBuilder::new_as_child(window)
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

    pub fn set_visible(&mut self, visible: bool) {
        let _ = self.webview.set_visible(visible);
    }

    pub fn send_command(&self, cmd: &AvatarCommand) {
        let js = match cmd {
            AvatarCommand::SetExpression(expr) => {
                format!(
                    "try{{window.unityInstance.SendMessage('AvatarController','SetExpression','{}');}}catch(e){{}}",
                    expr.escape_default()
                )
            }
            AvatarCommand::PlayAnimation { name, looped: _ } => {
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

#[derive(Debug, Clone)]
pub enum AvatarCommand {
    SetExpression(String),
    PlayAnimation { name: String, looped: bool },
    LipSync(HashMap<String, f32>),
    SetOutfit(String),
}

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

static SERVER_STARTED: AtomicBool = AtomicBool::new(false);

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
                let path = if path.is_empty() { "index.html" } else { path };

                if let Some(file) = WebglAssets::get(path) {
                    let mime = mime_type(path);
                    let data = file.data;

                    let mut headers = vec![
                        Header::from_bytes("Content-Type", mime.as_bytes()).unwrap(),
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

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Frame::NONE
        .fill(colors::SURFACE)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            let available = ui.available_size();
            let (rect, _response) = ui.allocate_exact_size(available, egui::Sense::click());
            state.avatar_panel_rect = Some(rect);
        });
}
