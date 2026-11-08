//! Lo que GPUI no expone: mostrar y esconder la ventana, "siempre encima",
//! notificaciones de Windows, ícono en la bandeja e inicio con Windows.
//!
//! Las notificaciones y la bandeja avisan a la app con un [`Command`] por un canal.
//! Mostrar la ventana también pasa por la app: GPUI le da su posición la primera vez.

use crate::model::ItemRef;

/// Identificador de la app para Windows: agrupa la barra de tareas y firma las notificaciones.
pub const APP_ID: &str = "Mateo.Temporizador";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Show,
    Quit,
    Stop(ItemRef),
    Snooze(ItemRef),
}

#[cfg(windows)]
mod imp {
    use std::{
        path::Path,
        sync::atomic::{AtomicIsize, Ordering},
    };

    use futures::channel::mpsc::UnboundedSender as Sender;

    use gpui::Window;
    use windows_sys::Win32::{
        Foundation::HWND,
        System::Registry::{
            HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
            RegSetKeyValueW,
        },
        UI::{
            Shell::SetCurrentProcessExplicitAppUserModelID,
            WindowsAndMessaging::{
                HWND_NOTOPMOST, HWND_TOPMOST, IsIconic, SPI_GETCLIENTAREAANIMATION, SW_HIDE,
                SW_RESTORE, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow,
                SetWindowPos, ShowWindow, SystemParametersInfoW,
            },
        },
    };

    use super::{APP_ID, Command};
    use crate::model::{Fired, ItemRef};

    /// La ventana principal, para poder mostrarla desde la bandeja o una notificación.
    static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    fn hwnd() -> Option<HWND> {
        let h = MAIN_HWND.load(Ordering::Relaxed);
        (h != 0).then_some(h as HWND)
    }

    pub fn register_window(window: &Window) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        // `Window` tiene su propio `window_handle()`; aquí se quiere el de raw-window-handle.
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        if let RawWindowHandle::Win32(h) = handle.as_raw() {
            MAIN_HWND.store(h.hwnd.get(), Ordering::Relaxed);
        }
    }

    pub fn show_window() {
        let Some(hwnd) = hwnd() else { return };
        // SAFETY: `hwnd` es la ventana de la app, que vive mientras la app corre.
        unsafe {
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            } else {
                ShowWindow(hwnd, SW_SHOW);
            }
            SetForegroundWindow(hwnd);
        }
    }

    pub fn hide_window() {
        if let Some(hwnd) = hwnd() {
            // SAFETY: ver `show_window`.
            unsafe { ShowWindow(hwnd, SW_HIDE) };
        }
    }

    /// "Mostrar animaciones en Windows" apagado (Accesibilidad > Efectos visuales).
    pub fn reduced_motion() -> bool {
        let mut on: i32 = 1;
        // SAFETY: SPI_GETCLIENTAREAANIMATION escribe un BOOL en `on`.
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut on as *mut i32).cast(),
                0,
            )
        };
        ok != 0 && on == 0
    }

    pub fn set_always_on_top(on_top: bool) {
        let Some(hwnd) = hwnd() else { return };
        let after = if on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
        // SAFETY: SetWindowPos solo cambia el orden Z de la ventana de la app.
        unsafe {
            SetWindowPos(
                hwnd,
                after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    // ── Registro ─────────────────────────────────────────────────────────────

    fn set_reg_string(key: &str, name: &str, value: &str) -> bool {
        let (key, name, value) = (wide(key), wide(name), wide(value));
        // SAFETY: cadenas UTF-16 terminadas en cero que viven durante la llamada.
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            ) == 0
        }
    }

    fn get_reg_string(key: &str, name: &str) -> Option<String> {
        let (key, name) = (wide(key), wide(name));
        let mut buf = vec![0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: `buf` tiene `len` bytes y RegGetValueW no escribe más allá.
        let ok = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            ) == 0
        };
        if !ok {
            return None;
        }
        let n = (len as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..n]))
    }

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN_NAME: &str = "Temporizador";

    fn autostart_command() -> Option<String> {
        let exe = std::env::current_exe().ok()?;
        Some(format!("\"{}\" --tray", exe.display()))
    }

    pub fn autostart_enabled() -> bool {
        get_reg_string(RUN_KEY, RUN_NAME).is_some()
    }

    pub fn set_autostart(on: bool) -> bool {
        if on {
            autostart_command().is_some_and(|cmd| set_reg_string(RUN_KEY, RUN_NAME, &cmd))
        } else {
            let (key, name) = (wide(RUN_KEY), wide(RUN_NAME));
            // SAFETY: cadenas UTF-16 terminadas en cero.
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
            true
        }
    }

    /// Si "Iniciar con Windows" apunta a otro ejecutable (se movió o se reinstaló),
    /// lo actualiza a este.
    pub fn refresh_autostart() {
        if let (Some(current), Some(cmd)) = (get_reg_string(RUN_KEY, RUN_NAME), autostart_command())
            && current != cmd
        {
            set_reg_string(RUN_KEY, RUN_NAME, &cmd);
        }
    }

    // ── Instancia única ──────────────────────────────────────────────────────

    /// Evento con nombre que comparten todas las instancias de la sesión.
    const SHOW_EVENT: &str = r"Local\Mateo.Temporizador.Mostrar";

    /// `Some(evento)` si esta es la primera instancia. Si ya había otra, le pide que
    /// se muestre y devuelve `None`: esta instancia debe terminar.
    pub fn claim_single_instance() -> Option<isize> {
        use windows_sys::Win32::{
            Foundation::{ERROR_ALREADY_EXISTS, GetLastError},
            System::Threading::{CreateEventW, SetEvent},
        };
        let name = wide(SHOW_EVENT);
        // SAFETY: nombre UTF-16 terminado en cero; el evento vive lo que la app.
        unsafe {
            let event = CreateEventW(std::ptr::null(), 0, 0, name.as_ptr());
            if event.is_null() {
                // Sin evento no se puede coordinar; mejor arrancar igual.
                return Some(0);
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                SetEvent(event);
                return None;
            }
            Some(event as isize)
        }
    }

    /// Cada vez que otra instancia intenta arrancar, manda `Command::Show`.
    pub fn listen_for_other_instances(event: isize, tx: Sender<Command>) {
        use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};
        if event == 0 {
            return;
        }
        std::thread::spawn(move || {
            loop {
                // SAFETY: el evento no se cierra nunca (vive lo que el proceso).
                let r = unsafe { WaitForSingleObject(event as _, INFINITE) };
                if r != 0 || tx.unbounded_send(Command::Show).is_err() {
                    break;
                }
            }
        });
    }

    // ── Notificaciones ───────────────────────────────────────────────────────

    /// Registra el nombre y el ícono de la app para que las notificaciones digan
    /// "Temporizador" y no "Windows PowerShell".
    pub fn register_app_id(icon: &Path) {
        let key = format!(r"Software\Classes\AppUserModelId\{APP_ID}");
        set_reg_string(&key, "DisplayName", "Temporizador");
        set_reg_string(&key, "IconUri", &icon.display().to_string());
        let id = wide(APP_ID);
        // SAFETY: cadena UTF-16 terminada en cero.
        unsafe { SetCurrentProcessExplicitAppUserModelID(id.as_ptr()) };
    }

    pub fn notify(fired: &Fired, snooze_label: &str, icon: &Path, tx: &Sender<Command>) {
        use tauri_winrt_notification::{Duration, IconCrop, Toast};

        let item = fired.item;
        let tx = tx.clone();
        let toast = Toast::new(APP_ID)
            .title(&fired.title)
            .text1(&fired.detail)
            // El sonido lo pone la app, que lo repite hasta que se detenga.
            .sound(None)
            .duration(Duration::Long)
            .add_button("Detener", &format!("stop:{}", item.encode()))
            .add_button(snooze_label, &format!("snooze:{}", item.encode()))
            .on_activated(move |action| {
                let cmd = match action.as_deref().and_then(|a| a.split_once(':')) {
                    Some(("stop", rest)) => ItemRef::decode(rest).map(Command::Stop),
                    Some(("snooze", rest)) => ItemRef::decode(rest).map(Command::Snooze),
                    _ => None,
                };
                // Clic en el cuerpo de la notificación: mostrar la app.
                let _ = tx.unbounded_send(cmd.unwrap_or(Command::Show));
                Ok(())
            });
        let toast = if icon.exists() {
            toast.icon(icon, IconCrop::Square, "")
        } else {
            toast
        };
        if let Err(e) = toast.show() {
            eprintln!("no se pudo mostrar la notificación: {e}");
        }
    }

    // ── Bandeja ──────────────────────────────────────────────────────────────

    pub struct Tray {
        icon: tray_icon::TrayIcon,
        tooltip: String,
    }

    impl Tray {
        pub fn new(tx: Sender<Command>) -> Option<Self> {
            use tray_icon::{
                Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
                menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
            };

            let menu = Menu::new();
            let show = MenuItem::with_id("show", "Abrir Temporizador", true, None);
            let quit = MenuItem::with_id("quit", "Salir", true, None);
            menu.append_items(&[&show, &PredefinedMenuItem::separator(), &quit])
                .ok()?;

            // El ícono #1 del ejecutable es el que incrusta `build.rs`.
            let icon = Icon::from_resource(1, Some((32, 32))).ok()?;
            let tray = TrayIconBuilder::new()
                .with_icon(icon)
                .with_tooltip("Temporizador")
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .build()
                .ok()?;

            let tray_tx = tx.clone();
            TrayIconEvent::set_event_handler(Some(move |event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    let _ = tray_tx.unbounded_send(Command::Show);
                }
            }));
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id.0.as_str() {
                "show" => {
                    let _ = tx.unbounded_send(Command::Show);
                }
                "quit" => {
                    let _ = tx.unbounded_send(Command::Quit);
                }
                _ => {}
            }));

            Some(Self {
                icon: tray,
                tooltip: String::new(),
            })
        }

        pub fn set_tooltip(&mut self, text: &str) {
            if self.tooltip != text {
                self.tooltip = text.to_string();
                let _ = self.icon.set_tooltip(Some(text));
            }
        }
    }
}

#[cfg(windows)]
pub use imp::*;

#[cfg(not(windows))]
mod imp {
    use std::path::Path;

    use futures::channel::mpsc::UnboundedSender as Sender;

    use gpui::Window;

    use super::Command;
    use crate::model::Fired;

    pub fn register_window(_: &Window) {}
    pub fn show_window() {}
    pub fn hide_window() {}
    pub fn set_always_on_top(_: bool) {}
    pub fn reduced_motion() -> bool {
        false
    }
    pub fn autostart_enabled() -> bool {
        false
    }
    pub fn set_autostart(_: bool) -> bool {
        false
    }
    pub fn refresh_autostart() {}
    pub fn register_app_id(_: &Path) {}
    pub fn claim_single_instance() -> Option<isize> {
        Some(0)
    }
    pub fn listen_for_other_instances(_: isize, _: Sender<Command>) {}
    pub fn notify(_: &Fired, _: &str, _: &Path, _: &Sender<Command>) {}
    pub struct Tray;
    impl Tray {
        pub fn new(_: Sender<Command>) -> Option<Self> {
            None
        }
        pub fn set_tooltip(&mut self, _: &str) {}
    }
}

#[cfg(not(windows))]
pub use imp::*;
