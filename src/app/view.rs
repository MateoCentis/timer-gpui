//! Dibujo de la ventana: barra de título, pestañas, lo que está sonando y el
//! contenido. También las piezas que comparten las pestañas: superficies de
//! vidrio, el control segmentado y el reloj grande editable.

use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};

use chrono::{NaiveTime, Timelike as _};
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, ClickEvent, Context, Div, ElementId, Entity,
    Focusable as _, FontFeatures, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, ScrollDelta, ScrollWheelEvent, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, Window, div, img, prelude::FluentBuilder as _,
    pulsating_between, px, relative,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::{Selectable as _, TimeField as BaseTimeField, TimeFieldState, TimeSegment};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, StyledExt as _, TitleBar,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    v_flex,
};

use crate::model::{self, ItemRef};

use super::{CONTEXT, LOGO, StopAll, Tab, Temporizador, TogglePin};

/// Entradas: rápidas al principio y suaves al final (ease-out fuerte).
pub(super) fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(4)
}

/// Aparece con un fundido y subiendo unos píxeles; sin movimiento si Windows
/// tiene las animaciones apagadas.
pub(super) fn fade_in<E: gpui::Styled + IntoElement + 'static>(
    el: E,
    id: impl Into<ElementId>,
    reduced_motion: bool,
) -> impl IntoElement {
    el.relative().with_animation(
        id,
        Animation::new(Duration::from_millis(220)).with_easing(ease_out),
        move |el, t| {
            let el = el.opacity(t);
            if reduced_motion {
                el
            } else {
                el.top(px((1.0 - t) * 6.0))
            }
        },
    )
}

/// Números de ancho fijo: un reloj que cambia no baila.
pub(super) fn tabular() -> FontFeatures {
    FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

// Espacios, en toda la app: 16 px de margen de página y entre secciones
// (`px_4`, `pt_4`, `gap_4`), 8 px entre cosas de una misma sección (`gap_2`) y
// 12 px de margen interior en las superficies.

/// Alto de las flechas del reloj grande; caben en el espacio que el reloj deja
/// arriba y abajo (`GROUP_GAP`).
const ARROW_H: Pixels = px(24.);

/// Entre el reloj grande y lo que tiene debajo: cabe una flecha y algo de aire.
pub(super) const GROUP_GAP: Pixels = px(32.);

/// Tamaño de los números del reloj grande, igual en temporizadores y alarmas.
pub(super) const DIAL_SIZE: Pixels = px(88.);

/// Campo de nombre bajo el reloj grande.
pub(super) fn name_field(state: &Entity<InputState>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    h_flex()
        .w(px(280.))
        .max_w_full()
        .h(px(36.))
        .px_4()
        .rounded_full()
        .border_1()
        .border_color(theme.input)
        .bg(theme.secondary)
        .child(Input::new(state).appearance(false).small())
}

/// Lista chica en la esquina superior derecha: lo que corre o lo que va a sonar.
pub(super) fn corner_list(
    id: &'static str,
    title: &'static str,
    rows: Vec<AnyElement>,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let count = rows.len();
    // 4 px de la superficie + 8 px de cada fila = 12 px de margen interior.
    surface(cx)
        .w(px(272.))
        .max_w_full()
        .p_1()
        .child(
            h_flex()
                .justify_between()
                .px_2()
                .pt_1p5()
                .pb_1()
                .child(section_title(title, cx))
                .child(div().text_xs().text_color(muted).child(count.to_string())),
        )
        .child(
            v_flex()
                .id(id)
                .max_h(px(212.))
                .overflow_y_scroll()
                .children(rows),
        )
}

/// Superficie de vidrio: un velo apenas más claro que el fondo, con un filo fino.
pub(super) fn surface(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted)
}

pub(super) fn section_title(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .text_xs()
        .font_medium()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

/// Riel de un control segmentado; cada opción es un [`segment`].
pub(super) fn segmented(cx: &App) -> Div {
    h_flex()
        .p(px(3.))
        .gap(px(2.))
        .rounded(cx.theme().radius + px(3.))
        .bg(cx.theme().secondary)
}

/// Una opción de un control segmentado. La elegida se levanta sobre el riel.
pub(super) fn segment(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    h_flex()
        .id(id)
        .flex_1()
        .justify_center()
        .items_center()
        .gap_1p5()
        .h(px(28.))
        .px_2()
        .rounded(theme.radius)
        .text_sm()
        .cursor_pointer()
        .when(selected, |this| {
            // Levantada sobre el riel: más clara en el tema claro, menos oscura
            // en el oscuro.
            this.bg(if theme.mode.is_dark() {
                theme.secondary_active
            } else {
                theme.popover
            })
            .text_color(theme.foreground)
            .font_medium()
            .shadow_xs()
        })
        .when(!selected, |this| {
            this.text_color(theme.muted_foreground)
                .hover(|s| s.text_color(theme.foreground))
        })
        .child(label.into())
}

/// Suma `delta` al segmento sin pasar al de al lado (59 → 00, no 1:00).
fn step_segment(time: NaiveTime, segment: TimeSegment, delta: i32) -> NaiveTime {
    let (h, m, s) = (
        time.hour() as i32,
        time.minute() as i32,
        time.second() as i32,
    );
    let (h, m, s) = match segment {
        TimeSegment::Hour => ((h + delta).rem_euclid(24), m, s),
        TimeSegment::Minute => (h, (m + delta).rem_euclid(60), s),
        TimeSegment::Second => (h, m, (s + delta).rem_euclid(60)),
        _ => (h, m, s),
    };
    NaiveTime::from_hms_opt(h as u32, m as u32, s as u32).unwrap_or(time)
}

fn step(
    state: &Entity<TimeFieldState>,
    segment: TimeSegment,
    delta: i32,
    window: &mut Window,
    cx: &mut App,
) {
    state.update(cx, |state, cx| {
        let time = step_segment(state.time(), segment, delta);
        state.set_time(time, window, cx);
    });
}

/// Unidad bajo un número del reloj grande. Lleva su propio interlineado: si
/// heredara el de los números quedaría flotando lejos. La copia oculta de arriba
/// compensa su alto, para que el número quede a la altura de los ":".
fn unit_label(unit: &'static str, color: Hsla, hidden: bool) -> impl IntoElement {
    div()
        .text_sm()
        .line_height(px(20.))
        .font_weight(FontWeight::NORMAL)
        .text_color(color)
        .when(hidden, |this| this.invisible())
        .child(unit)
}

/// El reloj grande y editable: se escribe encima, se mueve con las flechas o la
/// rueda del mouse, y cada número tiene sus flechas al pasar el mouse.
pub(super) fn big_time(
    id: &'static str,
    state: &Entity<TimeFieldState>,
    size: Pixels,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    let (fg, muted, selection) = (theme.foreground, theme.muted_foreground, theme.selection);
    let radius = theme.radius;
    let group: SharedString = format!("{id}-group").into();
    let group_name = group.clone();

    let field_state = state.clone();
    let field = BaseTimeField::new(id, state)
        .flex()
        .items_center()
        .text_size(size)
        .line_height(size * 1.02)
        .font_weight(FontWeight::LIGHT)
        .font_features(tabular())
        // Los ":" toman este color; los números, el suyo.
        .text_color(muted)
        .render_segment(move |item, seg_state, _, cx| {
            let segment = seg_state.segment();
            let unit = match segment {
                TimeSegment::Hour => "h",
                TimeSegment::Minute => "min",
                _ => "s",
            };
            // Rueda del mouse: un paso por muesca; en el touchpad, cada 24 px.
            let pending = Rc::new(Cell::new(0.0f32));
            let wheel_state = field_state.clone();
            // Las flechas quedan fuera del flujo, justo arriba y abajo de la
            // columna, en el espacio que separa el reloj de lo demás.
            let arrow = |name: &'static str, icon: IconName, delta: i32| {
                let state = field_state.clone();
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .when(delta > 0, |this| this.bottom(relative(1.)))
                    .when(delta < 0, |this| this.top(relative(1.)))
                    .invisible()
                    .when(focused, |this| this.visible())
                    .group_hover(group.clone(), |s| s.visible())
                    .child(
                        // Tan anchas como el número: fáciles de acertar.
                        Button::new((name, segment as usize))
                            .ghost()
                            .w(size * 1.25)
                            .h(ARROW_H)
                            .child(Icon::new(icon).size(px(22.)))
                            .tab_stop(false)
                            .on_click(move |_: &ClickEvent, window, cx| {
                                step(&state, segment, delta, window, cx)
                            }),
                    )
            };
            let _ = cx;
            // Flechas por fuera; la unidad justo debajo del número y su hueco
            // también arriba, para que el número quede a la altura de los ":".
            v_flex()
                .relative()
                .items_center()
                .child(unit_label(unit, muted, true))
                .child(
                    // La flecha de arriba va pegada al número (la de abajo, bajo
                    // la unidad), así las dos quedan a la misma distancia.
                    item.relative()
                        .child(arrow("up", IconName::ChevronUp, 1))
                        .px(size * 0.06)
                        .line_height(size * 1.02)
                        .rounded(radius)
                        .text_color(fg)
                        .cursor_text()
                        .when(seg_state.is_selected(), |this| this.bg(selection))
                        .on_scroll_wheel(move |ev: &ScrollWheelEvent, window, cx| {
                            let lines = match ev.delta {
                                ScrollDelta::Lines(p) => p.y,
                                ScrollDelta::Pixels(p) => {
                                    let acc = pending.get() + f32::from(p.y);
                                    let steps = (acc / 24.0).trunc();
                                    pending.set(acc - steps * 24.0);
                                    steps
                                }
                            };
                            if lines != 0.0 {
                                let delta = if lines > 0.0 { 1 } else { -1 };
                                step(&wheel_state, segment, delta, window, cx);
                                cx.stop_propagation();
                            }
                        }),
                )
                .child(unit_label(unit, muted, false))
                .child(arrow("down", IconName::ChevronDown, -1))
                .into_any_element()
        });

    // El área de "pasar el mouse" incluye la franja de las flechas, pero sin
    // ocupar lugar en el diseño.
    div()
        .id(SharedString::from(format!("{id}-wrap")))
        .group(group_name)
        .py(ARROW_H)
        .my(-ARROW_H)
        .child(field)
        .into_any_element()
}

impl Temporizador {
    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        TitleBar::new()
            .bg(theme.title_bar)
            .border_color(theme.title_bar_border)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(img(LOGO).size(px(16.)).flex_shrink_0())
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Temporizador"),
                    ),
            )
            .child(
                h_flex()
                    .id("title-actions")
                    // Tapa la zona de arrastre de la barra: sin esto Windows trata el
                    // clic como "mover ventana" y los botones no reciben el clic.
                    .occlude()
                    .flex_shrink_0()
                    .pr_1()
                    .child(
                        Button::new("pin")
                            .ghost()
                            .xsmall()
                            .icon(if self.settings.always_on_top {
                                IconName::PinOff
                            } else {
                                IconName::Pin
                            })
                            .tooltip("Siempre encima (Ctrl+Shift+T)")
                            .selected(self.settings.always_on_top)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.toggle_pin(&TogglePin, window, cx)
                            })),
                    ),
            )
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let running = self.state.timers.len();
        let armed = self.state.alarms.iter().filter(|a| a.enabled).count();
        let muted = theme.muted_foreground;

        let tab = |id: &'static str, tab: Tab, label: &'static str, count: usize| {
            let active = self.tab == tab;
            segment(id, label, active, cx)
                .when(count > 0, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .font_normal()
                            .text_color(muted)
                            .child(count.to_string()),
                    )
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.set_tab(tab, window, cx)
                }))
        };

        div().px_4().pt_1().child(
            segmented(cx)
                .child(tab("tab-timers", Tab::Timers, "Temporizador", running))
                .child(tab("tab-alarms", Tab::Alarms, "Alarmas", armed))
                .child(tab("tab-settings", Tab::Settings, "Ajustes", 0)),
        )
    }

    /// Lo que está sonando, arriba de todo, con sus botones.
    fn render_ringing(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let ringing = self.state.ringing();
        if ringing.is_empty() {
            return None;
        }
        let theme = cx.theme();
        let alert = theme.danger;
        let silenced = self.sound_until.is_none();
        let many = ringing.len() > 1;

        let rows = ringing.into_iter().map(|(item, since)| {
            let title = self.state.describe(item).unwrap_or_default();
            let (key, snooze_label) = match item {
                ItemRef::Timer(id) => (id as usize * 2, "+1 min".to_string()),
                ItemRef::Alarm(id) => (
                    id as usize * 2 + 1,
                    format!("+{} min", self.settings.snooze_minutes),
                ),
            };
            let kind = match item {
                ItemRef::Timer(_) => "Terminó",
                ItemRef::Alarm(_) => "Sonó",
            };
            let detail = format!(
                "{kind} a las {}{}",
                model::to_local(since).format("%H:%M"),
                if silenced { " · en silencio" } else { "" }
            );
            let row = h_flex()
                .gap_3()
                .items_center()
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(alert)
                        .child(Icon::new(IconName::BellRing).size(px(20.)))
                        .with_animation(
                            ("bell", key),
                            Animation::new(Duration::from_millis(1000))
                                .repeat()
                                .with_easing(pulsating_between(0.4, 1.0)),
                            |el, t| el.opacity(t),
                        ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().truncate().text_sm().font_medium().child(title))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(detail),
                        ),
                )
                .child(
                    Button::new(("ring-snooze", key))
                        .small()
                        .label(snooze_label)
                        .tooltip(match item {
                            ItemRef::Timer(_) => "Sumar 1 minuto",
                            ItemRef::Alarm(_) => "Posponer",
                        })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.snooze(item, model::now_ms(), cx)
                        })),
                )
                .child(
                    Button::new(("ring-stop", key))
                        .primary()
                        .small()
                        .label("Detener")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.stop(item, model::now_ms(), cx)
                        })),
                );
            fade_in(row, ("ring-row", key), self.reduced_motion)
        });

        Some(
            div().px_4().pt_4().child(
                v_flex()
                    .gap_3()
                    .p_3()
                    .rounded(theme.radius_lg)
                    .border_1()
                    .border_color(alert.opacity(0.45))
                    .bg(alert.opacity(0.10))
                    .children(rows)
                    .when(many || !silenced, |this| {
                        this.child(
                            h_flex()
                                .justify_end()
                                .gap_1()
                                .when(!silenced, |this| {
                                    this.child(
                                        Button::new("silence")
                                            .ghost()
                                            .xsmall()
                                            .icon(IconName::BellOff)
                                            .label("Silenciar")
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _, cx| {
                                                    this.silence();
                                                    cx.notify();
                                                },
                                            )),
                                    )
                                })
                                .when(many, |this| {
                                    this.child(
                                        Button::new("stop-all")
                                            .ghost()
                                            .xsmall()
                                            .label("Detener todo (Esc)")
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, window, cx| {
                                                    this.stop_all(&StopAll, window, cx)
                                                },
                                            )),
                                    )
                                }),
                        )
                    }),
            ),
        )
    }

    fn render_status(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let status = self.status.clone()?;
        let theme = cx.theme();
        let pill = div()
            .px_3()
            .py_1p5()
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .text_sm()
            .shadow_lg()
            .child(status);
        Some(
            h_flex()
                .absolute()
                .bottom_4()
                .left_0()
                .right_0()
                .justify_center()
                .px_4()
                .child(fade_in(
                    pill,
                    ("status", self.status_seq),
                    self.reduced_motion,
                )),
        )
    }

    fn window_title(&self) -> String {
        let now = model::now_ms();
        if !self.state.ringing().is_empty() {
            return "¡Sonando! — Temporizador".into();
        }
        match self.state.soonest_timer() {
            Some(t) => format!(
                "{} · {} — Temporizador",
                model::clock(t.remaining(now)),
                t.title()
            ),
            None => "Temporizador".into(),
        }
    }
}

impl Render for Temporizador {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.window_title();
        if title != self.last_title {
            window.set_window_title(&title);
            self.last_title = title;
        }

        let content = match self.tab {
            Tab::Timers => self.render_timers_tab(window, cx).into_any_element(),
            Tab::Alarms => self.render_alarms_tab(window, cx).into_any_element(),
            Tab::Settings => self.render_settings_tab(cx).into_any_element(),
        };

        v_flex()
            .relative()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::show_timers))
            .on_action(cx.listener(Self::show_alarms))
            .on_action(cx.listener(Self::show_settings))
            .on_action(cx.listener(Self::stop_all))
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(Self::toggle_pin))
            .on_action(cx.listener(Self::hide_window))
            .on_action(cx.listener(Self::quit_action))
            .size_full()
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(cx))
            .child(self.render_tabs(cx))
            .children(self.render_ringing(cx))
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(content),
            )
            .children(self.render_status(cx))
    }
}
