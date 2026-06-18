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

use eframe::egui::ViewportBuilder;

fn main() -> Result<(), eframe::Error> {
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