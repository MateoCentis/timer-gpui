// Sin consola de fondo en las builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Las macros de GPUI generan rutas `gpui::...`; gpui-kit recomienda este alias.
extern crate gpui_kit as gpui;

mod app;
mod model;
mod platform;
mod sound;
mod store;
mod theme;

use std::{borrow::Cow, path::PathBuf};

use gpui::{
    AppContext as _, AssetSource, Bounds, SharedString, WindowBounds, WindowOptions, point, px,
    size,
};
use gpui_kit::component::TitleBar;

use crate::{app::Temporizador, store::Settings};

// Íconos que no vienen en el paquete por defecto de los componentes.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Timer,
        AlarmClock,
        AlarmClockPlus,
        BellRing,
        BellOff,
        BookmarkPlus,
        RotateCcw,
        Trash,
        Pin,
        PinOff,
        X,
    ]
);

const LOGO_PNG: &[u8] = include_bytes!("../assets/logo.png");
const LOGO_256_PNG: &[u8] = include_bytes!("../assets/logo-256.png");

/// Íconos por defecto de los componentes, los propios y el logo.
struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if path == app::LOGO {
            return Ok(Some(Cow::Borrowed(LOGO_PNG)));
        }
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut all = gpui_kit::assets::Assets.list(path)?;
        all.extend(ExtraIcons.list(path)?);
        Ok(all)
    }
}

/// Deja el logo en la carpeta de datos: las notificaciones necesitan un archivo.
fn write_notification_icon() -> PathBuf {
    let path = store::data_dir().join("logo.png");
    let same = std::fs::read(&path).is_ok_and(|b| b == LOGO_256_PNG);
    if !same {
        let _ = std::fs::create_dir_all(store::data_dir());
        let _ = std::fs::write(&path, LOGO_256_PNG);
    }
    path
}

fn main() {
    // `--tray`: arrancar escondido en la bandeja (lo usa "Iniciar con Windows").
    let start_hidden = std::env::args().any(|a| a == "--tray");
    // Si ya está abierta, se le pide que se muestre y esta copia termina.
    let Some(instance) = platform::claim_single_instance() else {
        return;
    };
    let settings = Settings::load();
    let state = store::load_state();
    let icon_path = write_notification_icon();
    platform::register_app_id(&icon_path);
    platform::refresh_autostart();

    gpui_kit::application()
        .with_assets(AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            theme::apply(settings.dark, settings.opacity(), None, cx);
            app::bind_keys(cx);

            // La posición guardada vale solo en el monitor que la contiene: GPUI la
            // valida contra el monitor que se le indique (por defecto, el principal).
            let saved = settings
                .window
                .filter(|b| b.width >= 300.0 && b.height >= 300.0);
            let restored = saved.and_then(|b| {
                let bounds = Bounds {
                    origin: point(px(b.x), px(b.y)),
                    size: size(px(b.width), px(b.height)),
                };
                let center = bounds.center();
                let display = cx
                    .displays()
                    .into_iter()
                    .find(|d| d.bounds().contains(&center))?;
                Some((bounds, display.id()))
            });
            let (window_bounds, display_id) = match restored {
                Some((bounds, id)) => (bounds, Some(id)),
                None => (Bounds::centered(None, size(px(460.), px(720.)), cx), None),
            };

            // Sin la barra de título de Windows: la app dibuja la suya con los
            // botones de minimizar, maximizar y cerrar.
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                window_min_size: Some(size(px(380.), px(420.))),
                display_id,
                window_background: theme::glass_appearance(settings.opacity()),
                show: !start_hidden,
                focus: !start_hidden,
                ..TitleBar::window_options()
            };

            let view_on_top = settings.always_on_top;
            let commands = futures::channel::mpsc::unbounded();
            platform::listen_for_other_instances(instance, commands.0.clone());
            let (window, _) = gpui_kit::open_window(options, cx, |window, cx| {
                platform::register_window(window);
                cx.new(|cx| Temporizador::new(state, settings, commands, icon_path, window, cx))
            })
            .expect("no se pudo abrir la ventana");

            // Cerrar la ventana (sin la bandeja) termina la app.
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            // "Siempre encima" necesita la ventana nativa ya creada.
            let on_top = view_on_top;
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, _| {
                    platform::register_window(window);
                    if on_top {
                        platform::set_always_on_top(true);
                    }
                });
            });
        });
}
