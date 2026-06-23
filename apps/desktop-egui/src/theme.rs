use eframe::egui::{self, Color32, Visuals};

/// Makima brand colors
pub mod colors {
    use eframe::egui::Color32;

    /// Deep graphite base
    pub const GRAPHITE_BG: Color32 = Color32::from_rgb(18, 18, 22);
    pub const GRAPHITE_SURFACE: Color32 = Color32::from_rgb(28, 28, 34);
    pub const GRAPHITE_ELEVATED: Color32 = Color32::from_rgb(38, 38, 44);
    pub const GRAPHITE_BORDER: Color32 = Color32::from_rgb(48, 48, 54);

    /// Red accents for Makima identity — restrained, not overwhelming
    pub const RED_PRIMARY: Color32 = Color32::from_rgb(200, 40, 40);
    pub const RED_ACCENT: Color32 = Color32::from_rgb(220, 60, 60);
    pub const RED_DIM: Color32 = Color32::from_rgb(160, 30, 30);
    pub const RED_GLOW: Color32 = Color32::from_rgb(255, 60, 60);

    /// Text colors
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(220, 220, 228);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(160, 160, 170);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(110, 110, 120);
    pub const TEXT_ACCENT: Color32 = Color32::from_rgb(240, 60, 60);

    /// Semantic colors
    pub const SUCCESS: Color32 = Color32::from_rgb(34, 197, 94);
    pub const WARNING: Color32 = Color32::from_rgb(245, 158, 11);
    pub const ERROR: Color32 = Color32::from_rgb(239, 68, 68);
    pub const INFO: Color32 = Color32::from_rgb(59, 130, 246);

    /// Chat bubble colors
    pub const BUBBLE_USER_BG: Color32 = Color32::from_rgb(30, 30, 40);
    pub const BUBBLE_ASSISTANT_BG: Color32 = Color32::from_rgb(24, 24, 32);
    pub const BUBBLE_TOOL_BG: Color32 = Color32::from_rgb(20, 20, 28);

    // ── Layout semantic aliases ──────────────────────────────────────
    /// Main workspace / content background
    pub const BG: Color32 = GRAPHITE_BG;
    /// Sidebar / top bar / panel surfaces
    pub const SURFACE: Color32 = GRAPHITE_SURFACE;
    /// Elevated cards / inputs
    pub const ELEVATED: Color32 = GRAPHITE_ELEVATED;
    /// Weak separator / border
    pub const BORDER_WEAK: Color32 = GRAPHITE_BORDER;
    /// Activity bar icon default
    pub const ICON_DEFAULT: Color32 = TEXT_MUTED;
    /// Activity bar icon active
    pub const ICON_ACTIVE: Color32 = RED_ACCENT;
    /// Transparent background
    pub const TRANSPARENT: Color32 = Color32::TRANSPARENT;
}

/// Load CJK + Emoji fonts as fallbacks for proper Chinese and icon rendering.
///
/// The default egui fonts handle ASCII characters. CJK and Emoji fonts are appended
/// as fallbacks, so characters not found in the default fonts will fall back to these.
fn load_cjk_font(ctx: &egui::Context) {
    use std::sync::Arc;
    use std::fs;

    let mut fonts = egui::FontDefinitions::default();

    // ── 1. Load CJK font (for Chinese characters) ──────────────────────
    let cjk_paths = [
        r"C:\Windows\Fonts\simhei.ttf",    // SimHei (黑体) - single font file
        r"C:\Windows\Fonts\msyh.ttc",      // Microsoft YaHei (微软雅黑) - collection
        r"C:\Windows\Fonts\simsun.ttc",    // SimSun (宋体) - collection
    ];

    let mut cjk_loaded = false;
    for path in &cjk_paths {
        if let Ok(font_data) = fs::read(path) {
            fonts.font_data.insert("CJK".to_owned(), Arc::new(egui::FontData::from_owned(font_data)));

            // Append AFTER default fonts as fallback (not at position 0)
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.push("CJK".to_owned());
            }
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.push("CJK".to_owned());
            }

            tracing::info!("Loaded CJK font: {}", path);
            cjk_loaded = true;
            break;
        }
    }

    if !cjk_loaded {
        tracing::warn!("No CJK font found, Chinese text may display as boxes");
    }

    // ── 2. Load Emoji font (for emoji icons like 📞 🎙️ ✅ ❌) ──────────
    let emoji_paths = [
        r"C:\Windows\Fonts\seguiemj.ttf",  // Segoe UI Emoji (Win10/11)
    ];

    let mut emoji_loaded = false;
    for path in &emoji_paths {
        if let Ok(font_data) = fs::read(path) {
            fonts.font_data.insert(
                "Emoji".to_owned(),
                Arc::new(egui::FontData::from_owned(font_data)),
            );

            // Append AFTER default fonts (and after CJK) as fallback
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.push("Emoji".to_owned());
            }
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.push("Emoji".to_owned());
            }

            tracing::info!("Loaded Emoji font: {}", path);
            emoji_loaded = true;
            break;
        }
    }

    if !emoji_loaded {
        tracing::warn!("No Emoji font found, emoji may display as boxes");
    }

    ctx.set_fonts(fonts);
}

/// Apply Makima dark theme to egui
pub fn apply_theme(ctx: &egui::Context) {
    // Load CJK font first
    load_cjk_font(ctx);

    let mut style = (*ctx.style()).clone();

    style.visuals = Visuals {
        dark_mode: true,
        override_text_color: Some(colors::TEXT_PRIMARY),
        window_shadow: egui::epaint::Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: Color32::BLACK.gamma_multiply(0.2),
        },
        window_fill: colors::GRAPHITE_BG,
        panel_fill: colors::GRAPHITE_SURFACE,
        faint_bg_color: colors::GRAPHITE_SURFACE,
        extreme_bg_color: colors::GRAPHITE_BG,
        code_bg_color: colors::GRAPHITE_ELEVATED,
        warn_fg_color: colors::WARNING,
        error_fg_color: colors::ERROR,
        hyperlink_color: colors::RED_ACCENT,
        selection: egui::style::Selection {
            bg_fill: colors::RED_DIM,
            stroke: egui::Stroke::new(1.0, colors::RED_ACCENT),
        },
        widgets: egui::style::Widgets {
            noninteractive: egui::style::WidgetVisuals {
                bg_fill: colors::GRAPHITE_SURFACE,
                weak_bg_fill: colors::GRAPHITE_ELEVATED,
                bg_stroke: egui::Stroke::new(1.0, colors::GRAPHITE_BORDER),
                corner_radius: 6.0.into(),
                fg_stroke: egui::Stroke::new(1.0, colors::TEXT_SECONDARY),
                expansion: 0.0,
            },
            inactive: egui::style::WidgetVisuals {
                bg_fill: colors::GRAPHITE_ELEVATED,
                weak_bg_fill: colors::GRAPHITE_SURFACE,
                bg_stroke: egui::Stroke::new(1.0, colors::GRAPHITE_BORDER),
                corner_radius: 6.0.into(),
                fg_stroke: egui::Stroke::new(1.5, colors::TEXT_PRIMARY),
                expansion: 0.0,
            },
            hovered: egui::style::WidgetVisuals {
                bg_fill: colors::GRAPHITE_ELEVATED,
                weak_bg_fill: colors::GRAPHITE_SURFACE,
                bg_stroke: egui::Stroke::new(1.0, colors::RED_DIM),
                corner_radius: 6.0.into(),
                fg_stroke: egui::Stroke::new(1.5, colors::TEXT_ACCENT),
                expansion: 0.0,
            },
            active: egui::style::WidgetVisuals {
                bg_fill: colors::RED_DIM,
                weak_bg_fill: colors::RED_DIM,
                bg_stroke: egui::Stroke::new(1.0, colors::RED_PRIMARY),
                corner_radius: 6.0.into(),
                fg_stroke: egui::Stroke::new(2.0, colors::TEXT_PRIMARY),
                expansion: 0.0,
            },
            open: egui::style::WidgetVisuals {
                bg_fill: colors::GRAPHITE_ELEVATED,
                weak_bg_fill: colors::GRAPHITE_SURFACE,
                bg_stroke: egui::Stroke::new(1.0, colors::RED_ACCENT),
                corner_radius: 6.0.into(),
                fg_stroke: egui::Stroke::new(1.5, colors::TEXT_PRIMARY),
                expansion: 0.0,
            },
        },
        ..Default::default()
    };

    // Custom spacing for high information density
    style.spacing.item_spacing = egui::vec2(8.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.indent = 12.0;

    ctx.set_style(style);
}