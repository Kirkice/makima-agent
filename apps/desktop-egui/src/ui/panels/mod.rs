pub mod settings;
pub mod diagnostics;
pub mod login;
pub mod modes;
pub mod memory;
pub mod knowledge;
pub mod voice;
pub mod mcp;
pub mod marketplace;
pub mod audit;
pub mod model_config;
pub mod persona;

// Avatar panel: feature-gated
// With `avatar` feature → Unity WebGL WebView backend
// Without → static pixel-art placeholder
#[cfg(feature = "avatar")]
pub mod avatar_webview;

#[cfg(not(feature = "avatar"))]
pub mod avatar;

/// Unified entry-point for the Avatar dock tab.
/// The actual module used depends on the `avatar` Cargo feature.
#[cfg(feature = "avatar")]
pub mod avatar_impl {
    pub use super::avatar_webview::*;
}

#[cfg(not(feature = "avatar"))]
pub mod avatar_impl {
    pub use super::avatar::*;
}
