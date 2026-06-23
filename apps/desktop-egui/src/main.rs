//! Makima Agent Desktop Frontend
//!
//! A native desktop application for interacting with the Makima agent backend.
//! Built with egui/eframe, heavily inspired by the Zoo-Code (Roo Code) project architecture.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod app;
mod config;
mod state;
mod theme;
mod ui;
mod voice;

use eframe::egui::ViewportBuilder;

fn main() -> Result<(), eframe::Error> {
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

    // Load config for window size
    let config = config::app_config::AppConfig::load().unwrap_or_default();

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
            Ok(Box::new(app::MakimaApp::default()))
        }),
    )
}