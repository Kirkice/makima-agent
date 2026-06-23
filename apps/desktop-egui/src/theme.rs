use eframe::egui::{self, Color32, Visuals};

/// Makima brand colors and semantic UI tokens.
pub mod colors {
    use eframe::egui::Color32;

    pub const GRAPHITE_BG: Color32 = Color32::from_rgb(17, 18, 24);
    pub const GRAPHITE_SURFACE: Color32 = Color32::from_rgb(23, 24, 31);
    pub const GRAPHITE_ELEVATED: Color32 = Color32::from_rgb(30, 31, 40);
    pub const GRAPHITE_BORDER: Color32 = Color32::from_rgb(42, 43, 54);
    pub const GRAPHITE_BORDER_SOFT: Color32 = Color32::from_rgb(55, 57, 68);

    pub const RED_PRIMARY: Color32 = Color32::from_rgb(183, 64, 72);
    pub const RED_ACCENT: Color32 = Color32::from_rgb(219, 86, 95);
    pub const RED_DIM: Color32 = Color32::from_rgb(82, 34, 40);
    pub const RED_GLOW: Color32 = Color32::from_rgb(255, 112, 118);

    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(232, 233, 239);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(162, 165, 178);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(109, 113, 127);
    pub const TEXT_ACCENT: Color32 = Color32::from_rgb(234, 122, 129);

    pub const SUCCESS: Color32 = Color32::from_rgb(34, 197, 94);
    pub const WARNING: Color32 = Color32::from_rgb(245, 158, 11);
    pub const ERROR: Color32 = Color32::from_rgb(239, 68, 68);
    pub const INFO: Color32 = Color32::from_rgb(59, 130, 246);

    pub const BUBBLE_USER_BG: Color32 = Color32::from_rgb(49, 31, 38);
    pub const BUBBLE_ASSISTANT_BG: Color32 = Color32::from_rgb(26, 27, 35);
    pub const BUBBLE_TOOL_BG: Color32 = Color32::from_rgb(21, 28, 40);

    pub const BG: Color32 = GRAPHITE_BG;
    pub const SURFACE: Color32 = GRAPHITE_SURFACE;
    pub const ELEVATED: Color32 = GRAPHITE_ELEVATED;
    pub const BORDER_WEAK: Color32 = GRAPHITE_BORDER_SOFT;
    pub const ICON_DEFAULT: Color32 = TEXT_MUTED;
    pub const ICON_ACTIVE: Color32 = RED_ACCENT;
    pub const TRANSPARENT: Color32 = Color32::TRANSPARENT;
    pub const SELECTION_SOFT: Color32 = Color32::from_rgb(66, 39, 46);
}

fn load_cjk_font(ctx: &egui::Context) {
    use std::fs;
    use std::sync::Arc;

    let mut fonts = egui::FontDefinitions::default();

    let cjk_paths = [
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\simsun.ttc",
    ];

    for path in &cjk_paths {
        if let Ok(font_data) = fs::read(path) {
            fonts.font_data.insert(
                "CJK".to_owned(),
                Arc::new(egui::FontData::from_owned(font_data)),
            );

            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.push("CJK".to_owned());
            }
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.push("CJK".to_owned());
            }
            break;
        }
    }

    let emoji_paths = [r"C:\Windows\Fonts\seguiemj.ttf"];

    for path in &emoji_paths {
        if let Ok(font_data) = fs::read(path) {
            fonts.font_data.insert(
                "Emoji".to_owned(),
                Arc::new(egui::FontData::from_owned(font_data)),
            );

            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.push("Emoji".to_owned());
            }
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.push("Emoji".to_owned());
            }
            break;
        }
    }

    ctx.set_fonts(fonts);
}

/// Apply Makima dark theme to egui.
pub fn apply_theme(ctx: &egui::Context) {
    load_cjk_font(ctx);

    let mut style = (*ctx.style()).clone();

    style.visuals = Visuals {
        dark_mode: true,
        override_text_color: Some(colors::TEXT_PRIMARY),
        window_shadow: egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::BLACK.gamma_multiply(0.16),
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
            bg_fill: colors::SELECTION_SOFT,
            stroke: egui::Stroke::new(1.0, colors::RED_ACCENT),
        },
        widgets: egui::style::Widgets {
            noninteractive: egui::style::WidgetVisuals {
                bg_fill: colors::GRAPHITE_SURFACE,
                weak_bg_fill: colors::GRAPHITE_ELEVATED,
                bg_stroke: egui::Stroke::new(1.0, colors::BORDER_WEAK),
                corner_radius: 10.0.into(),
                fg_stroke: egui::Stroke::new(1.0, colors::TEXT_SECONDARY),
                expansion: 0.0,
            },
            inactive: egui::style::WidgetVisuals {
                bg_fill: colors::GRAPHITE_ELEVATED,
                weak_bg_fill: colors::GRAPHITE_SURFACE,
                bg_stroke: egui::Stroke::new(1.0, colors::BORDER_WEAK),
                corner_radius: 10.0.into(),
                fg_stroke: egui::Stroke::new(1.0, colors::TEXT_PRIMARY),
                expansion: 0.0,
            },
            hovered: egui::style::WidgetVisuals {
                bg_fill: colors::GRAPHITE_ELEVATED,
                weak_bg_fill: colors::GRAPHITE_SURFACE,
                bg_stroke: egui::Stroke::new(1.0, colors::GRAPHITE_BORDER),
                corner_radius: 10.0.into(),
                fg_stroke: egui::Stroke::new(1.0, colors::TEXT_PRIMARY),
                expansion: 0.0,
            },
            active: egui::style::WidgetVisuals {
                bg_fill: colors::SELECTION_SOFT,
                weak_bg_fill: colors::RED_DIM,
                bg_stroke: egui::Stroke::new(1.0, colors::RED_PRIMARY),
                corner_radius: 10.0.into(),
                fg_stroke: egui::Stroke::new(1.0, colors::TEXT_PRIMARY),
                expansion: 0.0,
            },
            open: egui::style::WidgetVisuals {
                bg_fill: colors::GRAPHITE_ELEVATED,
                weak_bg_fill: colors::GRAPHITE_SURFACE,
                bg_stroke: egui::Stroke::new(1.0, colors::RED_ACCENT),
                corner_radius: 10.0.into(),
                fg_stroke: egui::Stroke::new(1.0, colors::TEXT_PRIMARY),
                expansion: 0.0,
            },
        },
        ..Default::default()
    };

    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.indent = 12.0;
    style.spacing.menu_margin = 8.0.into();

    ctx.set_style(style);
}
