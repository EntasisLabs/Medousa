//! Medousa's color themes on RNDR. The catalog is generated from medousa-home's
//! theme contract (`npm run themes:native`), so both shells share one source of
//! truth. This module only decides how Medousa's roles land on RNDR tokens.
use rndr_core::Color;
use rndr_ui::{Appearance, ColorPalette, RadiusScale, Token};
use serde_json::Value;

const CATALOG: &str = include_str!("../themes/medousa-themes.json");

/// One theme's resolved roles for one appearance.
struct Variant<'a>(&'a Value);

impl Variant<'_> {
    fn rgb(&self, group: &str, key: &str) -> Color {
        let channel = |index: usize| {
            self.0[group][key][index]
                .as_u64()
                .unwrap_or_else(|| panic!("theme {group}.{key} is missing")) as f32
                / 255.
        };
        Color::rgb(channel(0), channel(1), channel(2))
    }
    fn role(&self, key: &str) -> Color {
        self.rgb("roles", key)
    }
    fn surface(&self, step: &str) -> Color {
        self.rgb("surface", step)
    }
    fn number(&self, key: &str) -> f32 {
        self.0[key].as_f64().unwrap_or(1.) as f32
    }
    fn on_primary(&self) -> Color {
        let v = &self.0["onPrimary"];
        let channel = |index: usize| v[index].as_u64().unwrap_or(255) as f32 / 255.;
        Color::rgb(channel(0), channel(1), channel(2))
    }
}

/// `color` at `alpha` composited over an opaque `base`. RNDR tokens are
/// opaque by contract, so medousa-home's translucent washes are flattened onto
/// the surface they sit on there.
fn over(color: Color, base: Color, alpha: f32) -> Color {
    let mix = |top: f32, bottom: f32| top * alpha + bottom * (1. - alpha);
    Color::rgb(
        mix(color.r, base.r),
        mix(color.g, base.g),
        mix(color.b, base.b),
    )
}

fn translucent(color: Color, alpha: f32) -> Color {
    Color::rgba(color.r, color.g, color.b, alpha)
}

/// Medousa's shell grammar, as in medousa-home's workshop shell: reading
/// content sits on the canvas, navigation and rails share the chrome plane,
/// and fills that are washes there are washes here.
fn apply(palette: &mut ColorPalette, appearance: Appearance, v: &Variant) {
    use Token::*;
    let canvas = v.role("canvas");
    let chrome = over(v.role("chrome"), canvas, v.number("chromeAlpha"));
    let border = v.role("border");
    let mut set = |token, color| palette.set(appearance, token, color);
    set(Canvas, canvas);
    set(Content, canvas);
    set(Navigation, chrome);
    // User bubbles, the composer and code blocks: `surface-800 / 0.5`.
    set(Raised, over(v.surface("800"), canvas, 0.5));
    // Menus and popovers: `surface-900 / 0.98`.
    set(Overlay, over(v.surface("900"), canvas, 0.98));
    set(
        Glass,
        translucent(v.role("chrome"), v.number("chromeAlpha")),
    );
    set(Material, translucent(v.role("pane"), v.number("paneAlpha")));
    set(Panel, translucent(v.role("header"), v.number("paneAlpha")));
    set(CodeFill, over(v.role("selection"), canvas, 0.11));
    set(Highlight, over(v.role("warning"), canvas, 0.2));
    set(Text, v.role("text"));
    set(Secondary, v.role("text-secondary"));
    set(Tertiary, v.role("text-tertiary"));
    set(Quiet, v.role("text-quiet"));
    set(Faint, v.role("text-faint"));
    set(Link, v.role("link"));
    set(Disabled, v.role("text-disabled"));
    set(OnAccent, v.on_primary());
    set(SubtleStroke, over(border, canvas, 0.3));
    set(StrongStroke, border);
    set(Focus, v.role("focus"));
    set(Accent, v.role("action"));
    set(Hover, v.role("action-hover"));
    set(Pressed, v.rgb("primary", "600"));
    // Row and icon hovers: `surface-700 / 0.6`, pressed a step denser.
    set(ControlHover, over(v.surface("700"), chrome, 0.6));
    set(ControlPressed, over(v.surface("700"), chrome, 0.8));
    // Active rail and list items. medousa-home's rail uses `selection / 0.24`
    // under an icon; rows carry text, so a lighter wash keeps Secondary and
    // Quiet ink readable on it in every catalog theme.
    set(Selected, over(v.role("selection"), chrome, 0.14));
    for (ink, fill, stroke, role) in [
        (InfoText, InfoFill, InfoStroke, "decorative"),
        (SuccessText, SuccessFill, SuccessStroke, "success"),
        (WarningText, WarningFill, WarningStroke, "warning"),
        (DangerText, DangerFill, DangerStroke, "error"),
    ] {
        let color = v.role(role);
        set(ink, color);
        set(fill, over(color, canvas, 0.12));
        set(stroke, color);
    }
}

/// A Medousa theme resolved for RNDR.
#[derive(Clone, Debug, PartialEq)]
pub struct MedousaTheme {
    pub id: String,
    pub palette: ColorPalette,
    pub radii: RadiusScale,
}

fn catalog() -> Value {
    serde_json::from_str(CATALOG).expect("embedded theme catalog is valid JSON")
}

/// The catalog's default theme id (medousa-home's `DEFAULT_COLOR_THEME`).
pub fn default_id() -> String {
    catalog()["defaultTheme"]
        .as_str()
        .unwrap_or("medousa")
        .to_string()
}

/// Resolve `id`, falling back to the default theme for an unknown id.
pub fn resolve(id: &str) -> MedousaTheme {
    let catalog = catalog();
    let themes = catalog["themes"].as_array().expect("theme list");
    let default = catalog["defaultTheme"].as_str().unwrap_or("medousa");
    let theme = themes
        .iter()
        .find(|t| t["id"] == id)
        .or_else(|| themes.iter().find(|t| t["id"] == default))
        .expect("default theme is in the catalog");
    let mut palette = ColorPalette::default();
    apply(&mut palette, Appearance::Dark, &Variant(&theme["dark"]));
    apply(&mut palette, Appearance::Light, &Variant(&theme["light"]));
    // Both appearances share shape; read it from the dark variant.
    let shape = Variant(&theme["dark"]);
    let control = shape.number("controlRadius");
    let card = shape.number("containerRadius");
    let radii = RadiusScale {
        control,
        input: control,
        card,
        ..RadiusScale::default()
    };
    MedousaTheme {
        id: theme["id"].as_str().unwrap_or(default).to_string(),
        palette,
        radii,
    }
}

/// One catalog theme as a picker shows it: id, name, and canvas, chrome and
/// action colors for each appearance.
#[derive(Clone, Debug, PartialEq)]
pub struct Swatch {
    pub id: String,
    pub label: String,
    pub dark: [Color; 3],
    pub light: [Color; 3],
}

/// Every catalog theme, in medousa-home's order.
pub fn swatches() -> Vec<Swatch> {
    let catalog = catalog();
    let colors = |v: &Value| {
        let v = Variant(v);
        [v.role("canvas"), v.role("chrome"), v.role("action")]
    };
    catalog["themes"]
        .as_array()
        .expect("theme list")
        .iter()
        .map(|t| Swatch {
            id: t["id"].as_str().unwrap_or_default().to_string(),
            label: t["label"].as_str().unwrap_or_default().to_string(),
            dark: colors(&t["dark"]),
            light: colors(&t["light"]),
        })
        .collect()
}

/// Device-local native appearance. Concurrent sources cannot select the
/// client's theme through a legacy active-workshop setting.
pub fn save_active_id(id: &str) -> Result<(), String> {
    let dir = medousa_desktop_host::paths::medousa_config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("native-appearance.json");
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(
        &temporary,
        serde_json::to_vec(&serde_json::json!({"theme":id})).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(temporary, path).map_err(|e| e.to_string())
}

/// Read only native device appearance; never consult a daemon selection.
pub fn active_id() -> String {
    std::fs::read(medousa_desktop_host::paths::medousa_config_dir().join("native-appearance.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
        .and_then(|v| v["theme"].as_str().map(str::to_owned))
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(default_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rndr_ui::{Contrast, UiTheme};

    fn ratio(a: Color, b: Color) -> f32 {
        let lum = |c: Color| {
            let linear = |v: f32| {
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            linear(c.r) * 0.2126 + linear(c.g) * 0.7152 + linear(c.b) * 0.0722
        };
        let (a, b) = (lum(a), lum(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn ids() -> Vec<String> {
        catalog()["themes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn every_catalog_theme_resolves_and_unknown_ids_fall_back() {
        assert_eq!(default_id(), "medousa");
        assert_eq!(ids().len(), 25);
        for id in ids() {
            assert_eq!(resolve(&id).id, id);
        }
        assert_eq!(resolve("no-such-theme"), resolve("medousa"));
    }

    /// The shell's reading pairs survive the mapping in every theme, and the
    /// text tiers keep their order, which is what lets grey carry hierarchy.
    #[test]
    fn mapped_themes_keep_shell_text_readable_and_tiers_ordered() {
        use Token::*;
        let mut failures = vec![];
        for id in ids() {
            let medousa = resolve(&id);
            for appearance in [Appearance::Dark, Appearance::Light] {
                let theme = UiTheme {
                    appearance,
                    palette: medousa.palette.clone(),
                    ..UiTheme::default()
                };
                let c = |token| theme.color(token);
                for surface in [Canvas, Navigation, Raised, ControlHover, Selected] {
                    for (ink, min) in [(Text, 4.5), (Secondary, 4.5), (Quiet, 3.0)] {
                        let r = ratio(c(ink), c(surface));
                        if r < min {
                            failures
                                .push(format!("{id} {appearance:?} {ink:?}/{surface:?} {r:.2}"));
                        }
                    }
                }
                let tiers =
                    [Text, Secondary, Tertiary, Quiet, Faint].map(|t| ratio(c(t), c(Canvas)));
                if !tiers.windows(2).all(|p| p[0] >= p[1]) {
                    failures.push(format!("{id} {appearance:?} tiers {tiers:?}"));
                }
                let r = ratio(c(Link), c(Canvas));
                if r < 4.5 {
                    failures.push(format!("{id} {appearance:?} Link/Canvas {r:.2}"));
                }
            }
            // High contrast stays RNDR's accessibility palette.
            let high = UiTheme {
                contrast: Contrast::High,
                palette: medousa.palette.clone(),
                ..UiTheme::default()
            };
            let standard = UiTheme {
                contrast: Contrast::High,
                ..UiTheme::default()
            };
            assert_eq!(high.color(Canvas), standard.color(Canvas));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn obsidian_reads_canvas_content_and_chrome_navigation() {
        let medousa = resolve("medousa");
        let theme = UiTheme {
            appearance: Appearance::Dark,
            palette: medousa.palette,
            ..UiTheme::default()
        };
        let rgb = |c: Color| [c.r, c.g, c.b].map(|v| (v * 255.).round() as u8);
        assert_eq!(rgb(theme.color(Token::Content)), [16, 16, 24]);
        assert_eq!(rgb(theme.color(Token::Canvas)), [16, 16, 24]);
        // chrome 32 32 44 at 0.96 over the canvas.
        assert_eq!(rgb(theme.color(Token::Navigation)), [31, 31, 43]);
        assert_eq!(rgb(theme.color(Token::Link)), [178, 148, 255]);
        assert!((medousa.radii.control - 8.8).abs() < 0.01);
    }
}
