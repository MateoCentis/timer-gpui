//! Sonido de alarma: uno de los de Windows (`C:\Windows\Media\Alarm01.wav`…), un
//! pitido generado por la app o un `.wav` propio. Se reproduce en bucle con
//! `PlaySound` hasta que se detiene.

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};

/// Nombre del sonido: `Alarm01`…`Alarm10` y `Ring01`…`Ring10` (los de Windows),
/// `beep` (el incorporado) o la ruta completa de un `.wav`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sound(pub String);

const BEEP: &str = "beep";

impl Default for Sound {
    fn default() -> Self {
        let alarm = Self("Alarm01".into());
        if alarm.path().is_some_and(|p| p.exists()) {
            alarm
        } else {
            Self(BEEP.into())
        }
    }
}

fn media_dir() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Media")
}

impl Sound {
    /// Sonidos para elegir: el incorporado y los de Windows que estén instalados.
    pub fn options() -> Vec<Self> {
        let mut all = vec![Self(BEEP.into())];
        for kind in ["Alarm", "Ring"] {
            for n in 1..=10 {
                let s = Self(format!("{kind}{n:02}"));
                if s.path().is_some_and(|p| p.exists()) {
                    all.push(s);
                }
            }
        }
        all
    }

    /// Archivo a reproducir; `None` para el pitido incorporado.
    fn path(&self) -> Option<PathBuf> {
        let name = self.0.as_str();
        if name == BEEP {
            return None;
        }
        let p = Path::new(name);
        if p.is_absolute() {
            Some(p.to_path_buf())
        } else {
            Some(media_dir().join(format!("{name}.wav")))
        }
    }

    pub fn label(&self) -> String {
        let name = self.0.as_str();
        if name == BEEP {
            return "Pitido".into();
        }
        for (prefix, label) in [("Alarm", "Alarma"), ("Ring", "Timbre")] {
            if let Some(n) = name
                .strip_prefix(prefix)
                .and_then(|n| n.parse::<u32>().ok())
            {
                return format!("{label} {n}");
            }
        }
        Path::new(name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| name.to_string())
    }

    /// Reproduce en bucle hasta `stop()`. Reemplaza lo que estuviera sonando.
    pub fn play_loop(&self) {
        play(self, true);
    }

    /// Una sola vez, para probarlo.
    pub fn preview(&self) {
        play(self, false);
    }
}

#[cfg(windows)]
fn play(sound: &Sound, looped: bool) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Media::Audio::{
        PlaySoundW, SND_ASYNC, SND_FILENAME, SND_LOOP, SND_MEMORY, SND_NODEFAULT,
    };

    let mut flags = SND_ASYNC | SND_NODEFAULT;
    if looped {
        flags |= SND_LOOP;
    }
    let file = sound.path().filter(|p| p.exists());
    // SAFETY: el nombre de archivo es una cadena UTF-16 terminada en cero que vive
    // hasta el final de la llamada (PlaySound la copia). El pitido es un WAV
    // estático, válido mientras suene.
    unsafe {
        match file {
            Some(path) => {
                let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
                PlaySoundW(wide.as_ptr(), std::ptr::null_mut(), flags | SND_FILENAME);
            }
            None => {
                PlaySoundW(
                    beep_wav().as_ptr().cast(),
                    std::ptr::null_mut(),
                    flags | SND_MEMORY,
                );
            }
        }
    }
}

#[cfg(windows)]
pub fn stop() {
    use windows_sys::Win32::Media::Audio::PlaySoundW;
    // SAFETY: un nombre nulo detiene lo que esté sonando.
    unsafe {
        PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
    }
}

#[cfg(not(windows))]
fn play(_: &Sound, _: bool) {}

#[cfg(not(windows))]
pub fn stop() {}

/// Tres pitidos cortos y una pausa, como un despertador; dura 1,4 s y se repite.
fn beep_wav() -> &'static [u8] {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    WAV.get_or_init(|| {
        const RATE: u32 = 22_050;
        let mut samples: Vec<i16> = Vec::new();
        let tone = |samples: &mut Vec<i16>, secs: f32, freq: f32| {
            let n = (secs * RATE as f32) as usize;
            let fade = (0.006 * RATE as f32) as usize;
            for i in 0..n {
                let t = i as f32 / RATE as f32;
                let env = (i.min(n - i) as f32 / fade as f32).min(1.0);
                let wave = (t * freq * std::f32::consts::TAU).sin() * 0.75
                    + (t * freq * 2.0 * std::f32::consts::TAU).sin() * 0.25;
                samples.push((wave * env * 0.55 * i16::MAX as f32) as i16);
            }
        };
        let silence = |samples: &mut Vec<i16>, secs: f32| {
            samples.extend(std::iter::repeat_n(0, (secs * RATE as f32) as usize));
        };
        for _ in 0..3 {
            tone(&mut samples, 0.13, 1046.5);
            silence(&mut samples, 0.07);
        }
        silence(&mut samples, 0.8);

        let data_len = (samples.len() * 2) as u32;
        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes()); // tamaño del bloque fmt
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&RATE.to_le_bytes());
        wav.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes por segundo
        wav.extend_from_slice(&2u16.to_le_bytes()); // bytes por muestra
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits por muestra
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for s in samples {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        wav
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres() {
        assert_eq!(Sound("Alarm03".into()).label(), "Alarma 3");
        assert_eq!(Sound("Ring10".into()).label(), "Timbre 10");
        assert_eq!(Sound("beep".into()).label(), "Pitido");
        assert_eq!(Sound(r"C:\x\gallo.wav".into()).label(), "gallo");
    }

    #[test]
    fn el_pitido_es_un_wav_valido() {
        let wav = beep_wav();
        assert_eq!(&wav[..4], b"RIFF");
        let riff = u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize;
        assert_eq!(riff + 8, wav.len());
    }
}
