//! Archivos de la app en `%APPDATA%\Temporizador`:
//! - `settings.json`: preferencias.
//! - `state.json`: temporizadores, alarmas y predefinidos.

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{model::State, sound::Sound};

pub fn data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Temporizador")
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SavedBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dark: bool,
    pub always_on_top: bool,
    /// Cerrar la ventana la esconde en la bandeja; los avisos siguen funcionando.
    pub close_to_tray: bool,
    /// Traer la ventana al frente cuando algo empieza a sonar.
    pub show_on_ring: bool,
    pub sound: Sound,
    /// Segundos que suena antes de callarse solo (el aviso queda en pantalla).
    pub ring_seconds: u64,
    /// Minutos de "Posponer" en alarmas (en temporizadores es "+1 min").
    pub snooze_minutes: u64,
    /// Opacidad del fondo de la ventana, en %. Por debajo de 100 es acrílico.
    pub opacity: u8,
    pub window: Option<SavedBounds>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dark: true,
            always_on_top: false,
            close_to_tray: true,
            show_on_ring: true,
            sound: Sound::default(),
            ring_seconds: 60,
            snooze_minutes: 5,
            opacity: 80,
            window: None,
        }
    }
}

fn read<T: DeserializeOwned>(name: &str) -> Option<T> {
    let src = fs::read_to_string(data_dir().join(name)).ok()?;
    serde_json::from_str(&src).ok()
}

/// Escribe primero a un archivo temporal: un corte a mitad de escritura no deja
/// el archivo a medias.
fn write<T: Serialize>(name: &str, value: &T) {
    let dir = data_dir();
    let _ = fs::create_dir_all(&dir);
    let Ok(json) = serde_json::to_string_pretty(value) else {
        return;
    };
    let tmp = dir.join(format!("{name}.tmp"));
    if fs::write(&tmp, json).is_ok() {
        let _ = fs::rename(&tmp, dir.join(name));
    }
}

impl Settings {
    pub fn opacity(&self) -> f32 {
        f32::from(self.opacity) / 100.0
    }

    pub fn load() -> Self {
        let mut s: Self = read("settings.json").unwrap_or_default();
        s.ring_seconds = s.ring_seconds.clamp(5, 30 * 60);
        s.snooze_minutes = s.snooze_minutes.clamp(1, 120);
        s.opacity = s.opacity.clamp(50, 100);
        s
    }

    pub fn save(&self) {
        write("settings.json", self);
    }
}

pub fn load_state() -> State {
    read("state.json").unwrap_or_default()
}

pub fn save_state(state: &State) {
    write("state.json", state);
}
