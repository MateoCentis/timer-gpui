//! Pestaña de temporizadores: el reloj grande para elegir la duración, los
//! predefinidos debajo y, arriba a la derecha, la lista chica de los que corren.

use chrono::Timelike;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _, px,
    relative,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    tooltip::Tooltip,
    v_flex,
};

use crate::model::{self, ItemRef, Millis, Preset, SECOND, Timer, TimerState};

use super::{
    Temporizador,
    view::{
        DIAL_SIZE, GROUP_GAP, big_time, corner_list, fade_in, name_field, section_title, surface,
        tabular,
    },
};

impl Temporizador {
    fn form_duration(&self, cx: &Context<Self>) -> Millis {
        let t = self.duration.read(cx).time();
        t.num_seconds_from_midnight() as Millis * SECOND
    }

    fn form_label(&self, cx: &Context<Self>) -> String {
        self.timer_label.read(cx).value().trim().to_string()
    }

    pub(super) fn start_from_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ms = self.form_duration(cx);
        if ms <= 0 {
            self.flash("Elige una duración mayor que cero", cx);
            return;
        }
        let label = self.form_label(cx);
        self.timer_label
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.start_timer(label, ms, cx);
    }

    fn start_timer(&mut self, label: String, ms: Millis, cx: &mut Context<Self>) {
        let id = self.state.take_id();
        let timer = Timer::new(id, label, ms, model::now_ms());
        // Los más nuevos, arriba.
        self.state.timers.insert(0, timer);
        self.save();
        cx.notify();
    }

    fn start_preset(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(p) = self.state.presets.get(ix).cloned() else {
            return;
        };
        // Si el nombre del predefinido es solo su duración ("5 min"), el temporizador
        // se titula "Temporizador de 5 min".
        let ms = p.secs as Millis * SECOND;
        let label = if p.label.trim() == model::short_duration(ms) {
            String::new()
        } else {
            p.label
        };
        self.start_timer(label, ms, cx);
    }

    fn save_preset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ms = self.form_duration(cx);
        if ms <= 0 {
            self.flash("Elige una duración mayor que cero", cx);
            return;
        }
        let secs = (ms / SECOND) as u64;
        let label = self.form_label(cx);
        let preset = Preset::new(
            if label.is_empty() {
                model::short_duration(ms)
            } else {
                label
            },
            secs,
        );
        if self.state.presets.contains(&preset) {
            self.flash("Ese predefinido ya existe", cx);
            return;
        }
        let title = preset.title();
        // Ordenados por duración.
        let at = self
            .state
            .presets
            .iter()
            .position(|p| p.secs > secs)
            .unwrap_or(self.state.presets.len());
        self.state.presets.insert(at, preset);
        self.timer_label
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.save();
        self.flash(format!("Predefinido «{title}» guardado"), cx);
    }

    fn delete_preset(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.state.presets.len() {
            let p = self.state.presets.remove(ix);
            self.save();
            self.flash(format!("Predefinido «{}» borrado", p.title()), cx);
        }
    }

    fn with_timer(&mut self, id: u64, cx: &mut Context<Self>, f: impl FnOnce(&mut Timer, Millis)) {
        let now = model::now_ms();
        if let Some(t) = self.state.timer_mut(id) {
            f(t, now);
        }
        self.after_ring_change(cx);
    }

    fn delete_timer(&mut self, id: u64, cx: &mut Context<Self>) {
        self.state.timers.retain(|t| t.id != id);
        self.after_ring_change(cx);
    }

    pub(super) fn render_timers_tab(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let now = model::now_ms();
        let rows: Vec<AnyElement> = self
            .state
            .timers
            .iter()
            .map(|t| self.render_running_row(t, now, cx).into_any_element())
            .collect();
        v_flex()
            .min_h_full()
            .p_4()
            .gap_4()
            .when(!rows.is_empty(), |this| {
                this.child(
                    h_flex()
                        .justify_end()
                        .child(corner_list("running", "En curso", rows, cx)),
                )
            })
            .child(self.render_dial(window, cx))
            .child(self.render_presets(cx))
    }

    /// El reloj grande, el nombre y el botón de empezar.
    fn render_dial(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .justify_center()
            .items_center()
            .gap(GROUP_GAP)
            .child(big_time("duration", &self.duration, DIAL_SIZE, window, cx))
            .child(
                v_flex()
                    .items_center()
                    .gap_3()
                    .child(name_field(&self.timer_label, cx))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new("start")
                                    .primary()
                                    .large()
                                    .rounded(px(999.))
                                    .px_6()
                                    .icon(IconName::Play)
                                    .label("Iniciar")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.start_from_form(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("save-preset")
                                    .ghost()
                                    .large()
                                    .rounded(px(999.))
                                    .icon(IconName::BookmarkPlus)
                                    .tooltip("Guardar como predefinido")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.save_preset(window, cx)
                                    })),
                            ),
                    ),
            )
    }

    fn render_running_row(
        &self,
        t: &Timer,
        now: Millis,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let id = t.id;
        let ringing = t.is_ringing();
        let paused = matches!(t.state, TimerState::Paused { .. });
        let group = format!("running-{id}");

        let tone = if ringing {
            theme.danger
        } else if paused {
            theme.muted_foreground
        } else {
            theme.foreground
        };

        let status = match t.state {
            TimerState::Running { ends_at } => {
                format!("Termina a las {}", model::to_local(ends_at).format("%H:%M"))
            }
            TimerState::Paused { .. } if t.is_reset() => "Listo para empezar".into(),
            TimerState::Paused { .. } => "En pausa".into(),
            TimerState::Ringing { .. } => "Terminó".into(),
        };

        let toggle = if ringing {
            Button::new(("toggle", id as usize))
                .ghost()
                .small()
                .child(
                    Icon::new(IconName::BellRing)
                        .small()
                        .text_color(theme.danger),
                )
                .tooltip("Detener")
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.stop(ItemRef::Timer(id), model::now_ms(), cx)
                }))
        } else {
            Button::new(("toggle", id as usize))
                .ghost()
                .small()
                .icon(if t.is_running() {
                    IconName::Pause
                } else {
                    IconName::Play
                })
                .tooltip(if t.is_running() {
                    "Pausar"
                } else if t.is_reset() {
                    "Empezar"
                } else {
                    "Seguir"
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.with_timer(id, cx, |t, now| t.toggle(now))
                }))
        };

        // El progreso va bajo el texto, no de borde a borde: así no parece un
        // separador. Recién creado no hay nada que mostrar.
        let progress = (!t.is_reset()).then(|| {
            div()
                .mt_1()
                .h(px(2.))
                .w_full()
                .rounded_full()
                .bg(theme.secondary)
                .child(
                    div()
                        .h_full()
                        .w(relative(t.progress(now)))
                        .rounded_full()
                        .bg(tone.opacity(0.8)),
                )
        });

        let row = h_flex()
            .id(("running-row", id as usize))
            .group(group.clone())
            .gap_2()
            .items_center()
            .px_2()
            .py_1p5()
            .rounded(theme.radius)
            .hover(|s| s.bg(theme.secondary))
            .child(toggle)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_baseline()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .child(t.short_title()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_sm()
                                    .font_medium()
                                    .font_features(tabular())
                                    .text_color(tone)
                                    .child(model::clock(t.remaining(now))),
                            ),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(status),
                    )
                    .children(progress),
            )
            .child(
                div().invisible().group_hover(group, |s| s.visible()).child(
                    Button::new(("delete", id as usize))
                        .ghost()
                        .xsmall()
                        .icon(IconName::X)
                        .tooltip("Quitar")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.delete_timer(id, cx)
                        })),
                ),
            );
        fade_in(row, ("running-in", id as usize), self.reduced_motion)
    }

    fn render_presets(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, bg, hover, active, muted, fg, popover) = (
            theme.border,
            theme.secondary,
            theme.secondary_hover,
            theme.secondary_active,
            theme.muted_foreground,
            theme.foreground,
            theme.popover,
        );
        surface(cx).p_3().child(
            v_flex()
                .gap_2()
                .child(section_title("Predefinidos", cx))
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_2()
                        .when(self.state.presets.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child("Elige una duración y guárdala con el marcador."),
                            )
                        })
                        .children(self.state.presets.iter().enumerate().map(|(i, p)| {
                            let group = format!("preset-{i}");
                            let detail = model::short_duration(p.secs as Millis * SECOND);
                            let show_detail = p.title() != detail;
                            h_flex()
                                .id(("preset", i))
                                .group(group.clone())
                                .relative()
                                .items_center()
                                .gap_1()
                                .px_3()
                                .h(px(30.))
                                .rounded_full()
                                .border_1()
                                .border_color(border)
                                .bg(bg)
                                .hover(|s| s.bg(hover))
                                .active(|s| s.bg(active))
                                .cursor_pointer()
                                .text_sm()
                                .text_color(fg)
                                .child(p.title())
                                .when(show_detail, |this| {
                                    this.child(div().text_xs().text_color(muted).child(detail))
                                })
                                // Para borrar: una ✕ sobre la esquina, solo al pasar el
                                // mouse. Fuera del flujo, así no deja un hueco a la derecha.
                                .child(
                                    div()
                                        .id(("preset-x", i))
                                        .absolute()
                                        .top(px(-6.))
                                        .right(px(-6.))
                                        .size(px(18.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_full()
                                        .border_1()
                                        .border_color(border)
                                        .bg(popover)
                                        .text_color(muted)
                                        .invisible()
                                        .group_hover(group, |s| s.visible())
                                        .hover(|s| s.text_color(fg))
                                        .child(Icon::new(IconName::X).size(px(10.)))
                                        .tooltip(|window, cx| {
                                            Tooltip::new("Borrar predefinido").build(window, cx)
                                        })
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, _, cx| {
                                                cx.stop_propagation();
                                                this.delete_preset(i, cx);
                                            },
                                        )),
                                )
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.start_preset(i, cx)
                                }))
                        })),
                ),
        )
    }
}
