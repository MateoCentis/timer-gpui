//! La ventana principal: temporizadores, alarmas y ajustes.
//!
//! Un ciclo cada 250 ms revisa qué venció, hace sonar la alarma y manda la
//! notificación. Los comandos de la bandeja y de las notificaciones llegan por un
//! canal que se atiende en cuanto llegan.

mod alarms;
mod settings;
mod timers;
mod view;

use std::{path::PathBuf, time::Duration};

use chrono::{Local, NaiveTime, Timelike};
use futures::{
    StreamExt as _,
    channel::mpsc::{UnboundedReceiver, UnboundedSender},
};
use gpui::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle, KeyBinding, SharedString,
    Subscription, Task, Window, actions,
};
use gpui_kit::component::{
    input::{InputEvent, InputState},
    time_field::{TimeFieldState, TimePrecision},
};

use crate::{
    model::{self, Days, ItemRef, MINUTE, Millis, State},
    platform::{self, Command, Tray},
    sound::{self, Sound},
    store::{self, Settings},
};

pub const LOGO: &str = "app/logo.png";
const CONTEXT: &str = "Temporizador";
const TICK: Duration = Duration::from_millis(250);

actions!(
    temporizador,
    [
        ShowTimers,
        ShowAlarms,
        ShowSettings,
        StopAll,
        ToggleTheme,
        TogglePin,
        HideWindow,
        Quit,
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-1", ShowTimers, Some(CONTEXT)),
        KeyBinding::new("ctrl-2", ShowAlarms, Some(CONTEXT)),
        KeyBinding::new("ctrl-3", ShowSettings, Some(CONTEXT)),
        KeyBinding::new("ctrl-,", ShowSettings, Some(CONTEXT)),
        KeyBinding::new("escape", StopAll, Some(CONTEXT)),
        KeyBinding::new("ctrl-t", ToggleTheme, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-t", TogglePin, Some(CONTEXT)),
        KeyBinding::new("ctrl-w", HideWindow, Some(CONTEXT)),
        KeyBinding::new("ctrl-q", Quit, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Timers,
    Alarms,
    Settings,
}

pub struct Temporizador {
    state: State,
    settings: Settings,
    tab: Tab,
    focus_handle: FocusHandle,

    // Formulario de temporizador nuevo.
    duration: Entity<TimeFieldState>,
    timer_label: Entity<InputState>,
    // Formulario de alarma nueva.
    alarm_time: Entity<TimeFieldState>,
    alarm_label: Entity<InputState>,
    alarm_days: Days,

    window: AnyWindowHandle,
    commands_tx: UnboundedSender<Command>,
    tray: Option<Tray>,
    /// Ícono que muestran las notificaciones.
    icon_path: PathBuf,
    /// Hasta cuándo suena; `None` si está callado.
    sound_until: Option<Millis>,
    sounds: Vec<Sound>,
    autostart: bool,

    /// Mensaje temporal bajo las pestañas.
    status: Option<SharedString>,
    status_timer: Option<Task<()>>,
    /// Sube con cada mensaje, para que cada uno entre con su animación.
    status_seq: usize,
    /// Windows tiene las animaciones apagadas: sin movimiento, solo fundidos.
    reduced_motion: bool,
    /// Segundo (o minuto) mostrado por última vez, para redibujar solo si cambia.
    last_frame: i64,
    last_title: String,
    _ticker: Task<()>,
    _commands: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl Temporizador {
    pub fn new(
        state: State,
        settings: Settings,
        commands: (UnboundedSender<Command>, UnboundedReceiver<Command>),
        icon_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (commands_tx, mut commands) = commands;

        let duration =
            cx.new(|cx| TimeFieldState::new(window, cx).precision(TimePrecision::Second));
        duration.update(cx, |s, cx| {
            s.set_time(NaiveTime::from_hms_opt(0, 5, 0).unwrap(), window, cx)
        });
        let timer_label = cx.new(|cx| InputState::new(window, cx).placeholder("Nombre (opcional)"));

        // Por defecto, la próxima hora en punto.
        let next_hour = (Local::now().hour() + 1) % 24;
        let alarm_time = cx.new(|cx| TimeFieldState::new(window, cx));
        alarm_time.update(cx, |s, cx| {
            s.set_time(
                NaiveTime::from_hms_opt(next_hour, 0, 0).unwrap(),
                window,
                cx,
            )
        });
        let alarm_label = cx.new(|cx| InputState::new(window, cx).placeholder("Nombre (opcional)"));

        let subscriptions = vec![
            cx.subscribe_in(
                &timer_label,
                window,
                |this, _, ev: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = ev {
                        this.start_from_form(window, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &alarm_label,
                window,
                |this, _, ev: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = ev {
                        this.add_alarm_from_form(window, cx);
                    }
                },
            ),
        ];

        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        });

        let commands_task = cx.spawn(async move |this, cx| {
            while let Some(cmd) = commands.next().await {
                if this
                    .update(cx, |this, cx| this.run_command(cmd, model::now_ms(), cx))
                    .is_err()
                {
                    break;
                }
            }
        });

        let tray = Tray::new(commands_tx.clone());

        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| this.should_close(window, cx))
                .unwrap_or(true)
        });

        let mut this = Self {
            state,
            settings,
            tab: Tab::Timers,
            focus_handle: cx.focus_handle(),
            duration,
            timer_label,
            alarm_time,
            alarm_label,
            alarm_days: 0,
            window: window.window_handle(),
            commands_tx,
            tray,
            icon_path,
            sound_until: None,
            sounds: Sound::options(),
            autostart: platform::autostart_enabled(),
            status: None,
            status_timer: None,
            status_seq: 0,
            reduced_motion: platform::reduced_motion(),
            last_frame: 0,
            last_title: String::new(),
            _ticker: ticker,
            _commands: commands_task,
            _subscriptions: subscriptions,
        };
        if this.tray.is_none() {
            // Sin bandeja, cerrar tiene que cerrar de verdad.
            this.settings.close_to_tray = false;
        }
        this.focus_handle.focus(window, cx);
        this.tick(cx);
        this
    }

    fn save(&self) {
        store::save_state(&self.state);
    }

    /// Muestra un mensaje bajo las pestañas durante unos segundos.
    fn flash(&mut self, msg: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.status = Some(msg.into());
        self.status_seq += 1;
        self.status_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(4)).await;
            let _ = this.update(cx, |this, cx| {
                this.status = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let now = model::now_ms();

        let fired = self.state.tick(now);
        if !fired.is_empty() {
            self.save();
            self.sound_until = Some(now + self.settings.ring_seconds as Millis * 1000);
            self.settings.sound.play_loop();
            for f in &fired {
                let snooze = match f.item {
                    ItemRef::Timer(_) => "+1 min".to_string(),
                    ItemRef::Alarm(_) => format!("Posponer {} min", self.settings.snooze_minutes),
                };
                platform::notify(f, &snooze, &self.icon_path, &self.commands_tx);
            }
            if self.settings.show_on_ring {
                self.show_window(cx);
            }
            self.tab = match fired[0].item {
                ItemRef::Timer(_) if self.tab != Tab::Settings => Tab::Timers,
                ItemRef::Alarm(_) if self.tab != Tab::Settings => Tab::Alarms,
                _ => self.tab,
            };
            cx.notify();
        }

        // Callar al vencer el tiempo de sonido o si ya no suena nada.
        if let Some(until) = self.sound_until
            && (now >= until || self.state.ringing().is_empty())
        {
            self.silence();
            cx.notify();
        }

        if let Some(tray) = self.tray.as_mut() {
            tray.set_tooltip(&tray_tooltip(&self.state, now));
        }

        // Redibujar cuando cambia lo que se muestra: cada segundo si algo corre o
        // suena, si no cada minuto (los "en 5 min" de las alarmas).
        let busy =
            self.state.timers.iter().any(|t| t.is_running()) || !self.state.ringing().is_empty();
        let frame = if busy { now / 1000 } else { now / MINUTE };
        if frame != self.last_frame {
            self.last_frame = frame;
            cx.notify();
        }
    }

    fn run_command(&mut self, cmd: Command, now: Millis, cx: &mut Context<Self>) {
        match cmd {
            Command::Show => self.show_window(cx),
            Command::Quit => self.quit(&Quit, cx),
            Command::Stop(item) => self.stop(item, now, cx),
            Command::Snooze(item) => self.snooze(item, now, cx),
        }
    }

    /// Muestra la ventana y la trae al frente. Pasa por GPUI, que la primera vez
    /// le da la posición y el tamaño guardados (si arrancó escondida no los tiene).
    fn show_window(&mut self, cx: &mut Context<Self>) {
        let handle = self.window;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            platform::show_window();
        });
    }

    fn silence(&mut self) {
        if self.sound_until.take().is_some() {
            sound::stop();
        }
    }

    fn stop(&mut self, item: ItemRef, now: Millis, cx: &mut Context<Self>) {
        self.state.stop(item, now);
        self.after_ring_change(cx);
    }

    fn snooze(&mut self, item: ItemRef, now: Millis, cx: &mut Context<Self>) {
        let ms = match item {
            ItemRef::Timer(_) => MINUTE,
            ItemRef::Alarm(_) => self.settings.snooze_minutes as Millis * MINUTE,
        };
        self.state.snooze(item, ms, now);
        self.after_ring_change(cx);
    }

    fn stop_all(&mut self, _: &StopAll, _: &mut Window, cx: &mut Context<Self>) {
        let now = model::now_ms();
        let ringing = self.state.ringing();
        if ringing.is_empty() {
            return;
        }
        for (item, _) in ringing {
            self.state.stop(item, now);
        }
        self.after_ring_change(cx);
    }

    fn after_ring_change(&mut self, cx: &mut Context<Self>) {
        if self.state.ringing().is_empty() {
            self.silence();
        }
        self.save();
        cx.notify();
    }

    fn set_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = tab;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn show_timers(&mut self, _: &ShowTimers, window: &mut Window, cx: &mut Context<Self>) {
        self.set_tab(Tab::Timers, window, cx);
    }

    fn show_alarms(&mut self, _: &ShowAlarms, window: &mut Window, cx: &mut Context<Self>) {
        self.set_tab(Tab::Alarms, window, cx);
    }

    fn show_settings(&mut self, _: &ShowSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.set_tab(Tab::Settings, window, cx);
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.dark = !self.settings.dark;
        self.settings.save();
        crate::theme::apply(
            self.settings.dark,
            self.settings.opacity(),
            Some(window),
            cx,
        );
        cx.notify();
    }

    fn set_opacity(&mut self, pct: u8, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.opacity = pct;
        self.settings.save();
        let opacity = self.settings.opacity();
        window.set_background_appearance(crate::theme::glass_appearance(opacity));
        crate::theme::apply(self.settings.dark, opacity, Some(window), cx);
        cx.notify();
    }

    fn toggle_pin(&mut self, _: &TogglePin, _: &mut Window, cx: &mut Context<Self>) {
        self.settings.always_on_top = !self.settings.always_on_top;
        platform::set_always_on_top(self.settings.always_on_top);
        self.settings.save();
        self.flash(
            if self.settings.always_on_top {
                "Siempre encima: activado"
            } else {
                "Siempre encima: desactivado"
            },
            cx,
        );
    }

    fn hide_window(&mut self, _: &HideWindow, window: &mut Window, cx: &mut Context<Self>) {
        if self.should_close(window, cx) {
            window.remove_window();
        }
    }

    /// Guarda todo y cierra la app (desde el menú de la bandeja o `Ctrl+Q`).
    fn quit(&mut self, _: &Quit, cx: &mut Context<Self>) {
        self.silence();
        self.save();
        self.settings.save();
        // Soltar el ícono de la bandeja antes de salir, para que no quede un fantasma.
        self.tray = None;
        cx.quit();
    }

    fn quit_action(&mut self, _: &Quit, _: &mut Window, cx: &mut Context<Self>) {
        self.quit(&Quit, cx);
    }

    /// Al cerrar la ventana: con la bandeja activa solo se esconde; si no, se
    /// guarda la posición y se sale.
    fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.remember_bounds(window);
        self.settings.save();
        self.save();
        if self.settings.close_to_tray && self.tray.is_some() {
            platform::hide_window();
            return false;
        }
        self.silence();
        self.tray = None;
        let _ = cx;
        true
    }

    fn remember_bounds(&mut self, window: &Window) {
        use gpui::WindowBounds;
        let b = match window.window_bounds() {
            WindowBounds::Windowed(b)
            | WindowBounds::Maximized(b)
            | WindowBounds::Fullscreen(b) => b,
        };
        self.settings.window = Some(store::SavedBounds {
            x: f32::from(b.origin.x),
            y: f32::from(b.origin.y),
            width: f32::from(b.size.width),
            height: f32::from(b.size.height),
        });
    }
}

/// Texto del ícono de la bandeja: lo que suena, o lo próximo que va a sonar.
fn tray_tooltip(state: &State, now: Millis) -> String {
    let mut lines = vec!["Temporizador".to_string()];
    let ringing = state.ringing();
    if !ringing.is_empty() {
        lines.push(format!("Sonando: {}", ringing.len()));
    }
    let running: Vec<_> = state.timers.iter().filter(|t| t.is_running()).collect();
    if let Some(t) = state.soonest_timer() {
        let more = if running.len() > 1 {
            format!(" (+{})", running.len() - 1)
        } else {
            String::new()
        };
        lines.push(format!(
            "{} · {}{more}",
            model::clock(t.remaining(now)),
            t.title()
        ));
    }
    if let Some(a) = state
        .alarms
        .iter()
        .filter(|a| a.next_at.is_some())
        .min_by_key(|a| a.next_at)
    {
        lines.push(format!(
            "Alarma {}",
            model::when(a.next_at.unwrap_or_default(), now)
        ));
    }
    // El tooltip de la bandeja admite como mucho 127 caracteres.
    let mut text = lines.join("\n");
    if text.chars().count() > 120 {
        text = text.chars().take(120).collect();
    }
    text
}
