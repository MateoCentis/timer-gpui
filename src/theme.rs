//! Tema de colores (variantes clara y oscura en `assets/theme.json`).

use std::rc::Rc;

use gpui::{App, SharedString, Window, WindowBackgroundAppearance};
use gpui_kit::component::{Theme, ThemeMode, ThemeSet};

/// Familia que GPUI resuelve a la fuente de interfaz del sistema (Segoe UI).
const SYSTEM_UI_FONT: &str = ".SystemUIFont";

/// Aplica el tema. `opacity` (de 0 a 1) es la del fondo de la ventana: por debajo
/// de 1 se ve, desenfocado, lo que hay detrás (ver `glass_appearance`).
pub fn apply(dark: bool, opacity: f32, window: Option<&mut Window>, cx: &mut App) {
    let set: ThemeSet =
        serde_json::from_str(include_str!("../assets/theme.json")).expect("tema inválido");
    let pick = |want_dark: bool| {
        set.themes
            .iter()
            .find(|t| t.mode.is_dark() == want_dark)
            .cloned()
            .map(Rc::new)
            .expect("falta una variante del tema")
    };
    let theme = Theme::global_mut(cx);
    theme.light_theme = pick(false);
    theme.dark_theme = pick(true);
    let mode = if dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    Theme::change(mode, window, cx);
    Theme::update(cx, |theme| {
        theme.font_family = SharedString::from(SYSTEM_UI_FONT);
        theme.colors.background = theme.colors.background.opacity(opacity.clamp(0.0, 1.0));
    });
}

/// Fondo de la ventana: acrílico (desenfoca lo de atrás) si no es opaca.
pub fn glass_appearance(opacity: f32) -> WindowBackgroundAppearance {
    if opacity < 1.0 {
        WindowBackgroundAppearance::Blurred
    } else {
        WindowBackgroundAppearance::Opaque
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn el_tema_es_valido() {
        let set: gpui_kit::component::ThemeSet =
            serde_json::from_str(include_str!("../assets/theme.json")).unwrap();
        assert!(set.themes.iter().any(|t| t.mode.is_dark()));
        assert!(set.themes.iter().any(|t| !t.mode.is_dark()));
    }
}
