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
}

/// Apply Makima dark theme to egui
pub fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    style.visuals = Visuals {
        dark_mode: true,
        override_text_color: Some(colors::TEXT_PRIMARY),
        window_rounding: egui::Rounding::same(8.0),
        window_shadow: egui::epaint::Shadow {
            offset: egui::vec2(0.0, 4.0),
            blur: 16.0,
            spread: 0.0,
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
                rounding: egui::Rounding::same(6.0),
                fg_stroke: egui::Stroke::new(1.0, colors::TEXT_SECONDARY),
                expansion: 0.0,
            },
            inactive: egui::style::WidgetVisuals {
                bg_fill: colors::GRAPHITE_ELEVATED,
                weak_bg_fill: colors::GRAPHITE_SURFACE,
                bg_stroke: egui::Stroke::new(1.0, colors::GRAPHITE_BORDER),
                rounding: egui::Rounding::same(6.0),
                fg_stroke: egui::Stroke::new(1.5, colors::TEXT_PRIMARY),
                expansion: 0.0,
            },
            hovered: egui::style::WidgetVisuals {
                bg_fill: colors::GRAPHITE_ELEVATED,
                weak_bg_fill: colors::GRAPHITE_SURFACE,
                bg_stroke: egui::Stroke::new(1.0, colors::RED_DIM),
                rounding: egui::Rounding::same(6.0),
                fg_stroke: egui::Stroke::new(1.5, colors::TEXT_ACCENT),
                expansion: 0.0,
            },
            active: egui::style::WidgetVisuals {
                bg_fill: colors::RED_DIM,
                weak_bg_fill: colors::RED_DIM,
                bg_stroke: egui::Stroke::new(1.0, colors::RED_PRIMARY),
                rounding: egui::Rounding::same(6.0),
                fg_stroke: egui::Stroke::new(2.0, colors::TEXT_PRIMARY),
                expansion: 0.0,
            },
            open: egui::style::WidgetVisuals {
                bg_fill: colors::GRAPHITE_ELEVATED,
                weak_bg_fill: colors::GRAPHITE_SURFACE,
                bg_stroke: egui::Stroke::new(1.0, colors::RED_ACCENT),
                rounding: egui::Rounding::same(6.0),
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