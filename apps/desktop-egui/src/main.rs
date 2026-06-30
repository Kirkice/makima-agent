//! Makima Agent Desktop Frontend
//!
//! A native desktop application for interacting with the Makima agent backend.
//! Built with egui/eframe, heavily inspired by the Zoo-Code (Roo Code) project architecture.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod app;
mod backend_launcher;
mod config;
mod state;
mod theme;
mod ui;
mod voice;
mod websocket_bridge;

use eframe::egui::ViewportBuilder;

fn main() -> Result<(), eframe::Error> {
    // Load .env from project root (not from cwd which may be apps/desktop-egui)
    let project_root = backend_launcher::find_project_root();
    let env_path = project_root.join(".env");
    if env_path.exists() {
        let _ = dotenvy::from_path(&env_path);
    } else {
        let _ = dotenvy::dotenv();
    }

    // Install panic hook for better crash diagnostics
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        eprintln!("\n=== PANIC OCCURRED ===");
        if let Some(location) = panic_info.location() {
            eprintln!("Location: {}:{}:{}", location.file(), location.line(), location.column());
        }
        if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            eprintln!("Message: {}", s);
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            eprintln!("Message: {}", s);
        }
        eprintln!("=====================\n");
        default_panic(panic_info);
    }));

    // Initialize tracing/logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,makima_desktop=debug".into()),
        )
        .init();

    tracing::info!("Starting Makima Agent Desktop v{}", env!("CARGO_PKG_VERSION"));
    if env_path.exists() {
        tracing::info!("Loaded .env from {:?}", env_path);
    }

    // Load config for window size
    let config = config::app_config::AppConfig::load().unwrap_or_default();

    // Auto-start backend if configured
    let backend_process = if config.auto_start_backend {
        tracing::info!("Auto-starting backend...");
        backend_launcher::ensure_backend_running()
    } else {
        tracing::info!("Auto-start backend disabled in config");
        backend_launcher::BackendProcess::none()
    };

    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([config.window_width, config.window_height])
            .with_min_inner_size([800.0, 600.0])
            .with_title("Makima Agent"),
        ..Default::default()
    };

    eframe::run_native(
        "Makima Agent",
        native_options,
        Box::new(|_cc| {
            // Create the application
            Ok(Box::new(app::MakimaApp::new(backend_process)))
        }),
    )
}
