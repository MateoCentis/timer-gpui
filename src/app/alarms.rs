//! Pestaña de alarmas: la hora grande, los días y el nombre de una alarma nueva
//! al centro; las alarmas, en una lista chica arriba a la derecha.

use chrono::Timelike;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::Selectable as _;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    switch::Switch,
    v_flex,
};

use crate::model::{self, ALL_DAYS, Alarm, DAY_LETTERS, Millis, WEEKDAYS};

use super::{
    Temporizador,
    view::{DIAL_SIZE, GROUP_GAP, big_time, corner_list, fade_in, name_field, tabular},
};

impl Temporizador {
    pub(super) fn add_alarm_from_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let time = self.alarm_time.read(cx).time();
        let label = self.alarm_label.read(cx).value().trim().to_string();
        let now = model::now_ms();
        let id = self.state.take_id();
        let alarm = Alarm::new(id, label, time.hour(), time.minute(), self.alarm_days, now);
        let msg = match alarm.next_at {
            Some(at) => format!(
                "Sonará {} ({})",
                model::when(at, now),
                model::until(at - now)
            ),
            None => "Alarma agregada".into(),
        };
        self.state.alarms.push(alarm);
        self.alarm_label
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.save();
        self.flash(msg, cx);
    }

    fn toggle_day(&mut self, day: usize, cx: &mut Context<Self>) {
        self.alarm_days ^= 1 << day;
        cx.notify();
    }

    fn set_alarm_enabled(&mut self, id: u64, enabled: bool, cx: &mut Context<Self>) {
        let now = model::now_ms();
        let mut msg = None;
        if let Some(a) = self.state.alarm_mut(id) {
            a.set_enabled(enabled, now);
            if let Some(at) = a.next_at {
                msg = Some(format!(
                    "Sonará {} ({})",
                    model::when(at, now),
                    model::until(at - now)
                ));
            }
        }
        self.after_ring_change(cx);
        if let Some(msg) = msg {
            self.flash(msg, cx);
        }
    }

    fn delete_alarm(&mut self, id: u64, cx: &mut Context<Self>) {
        self.state.alarms.retain(|a| a.id != id);
        self.after_ring_change(cx);
    }

    pub(super) fn render_alarms_tab(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let now = model::now_ms();
        // Encendidas primero, por próxima vez que suenan; después las apagadas, por hora.
        let mut alarms: Vec<&Alarm> = self.state.alarms.iter().collect();
        alarms.sort_by_key(|a| (a.next_at.is_none(), a.next_at, a.hour, a.minute));
        let rows: Vec<AnyElement> = alarms
            .into_iter()
            .map(|a| self.render_alarm(a, now, cx).into_any_element())
            .collect();

        // El mismo esquema que los temporizadores: la lista arriba a la derecha y
        // el reloj al centro del espacio que queda.
        v_flex()
            .min_h_full()
            .p_4()
            .gap_4()
            .when(!rows.is_empty(), |this| {
                this.child(
                    h_flex()
                        .justify_end()
                        .child(corner_list("alarms", "Alarmas", rows, cx)),
                )
            })
            .child(self.render_alarm_form(window, cx))
    }

    fn render_alarm_form(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let days = self.alarm_days;

        let day_chip = |i: usize| {
            let on = days & (1 << i) != 0;
            div()
                .id(("day", i))
                .size(px(36.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .text_sm()
                .cursor_pointer()
                .border_1()
                .when(on, |this| {
                    this.bg(theme.secondary_active)
                        .border_color(theme.foreground.opacity(0.45))
                        .text_color(theme.foreground)
                        .font_semibold()
                })
                .when(!on, |this| {
                    this.border_color(theme.border)
                        .text_color(theme.muted_foreground)
                        .hover(|s| s.bg(theme.secondary).text_color(theme.foreground))
                })
                .child(DAY_LETTERS[i])
        };

        let quick = |id: &'static str, label: &'static str, tooltip: &'static str, mask| {
            Button::new(id)
                .outline()
                .xsmall()
                .label(label)
                .tooltip(tooltip)
                .selected(days == mask)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.alarm_days = if this.alarm_days == mask { 0 } else { mask };
                    cx.notify();
                }))
        };

        v_flex()
            .flex_1()
            .justify_center()
            .items_center()
            .gap(GROUP_GAP)
            .child(big_time(
                "alarm-time",
                &self.alarm_time,
                DIAL_SIZE,
                window,
                cx,
            ))
            .child(
                v_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        h_flex()
                            .flex_wrap()
                            .justify_center()
                            .gap_1p5()
                            .children((0..7).map(|i| {
                                day_chip(i).on_click(cx.listener(
                                    move |this, _: &ClickEvent, _, cx| this.toggle_day(i, cx),
                                ))
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                if days == 0 {
                                    "Suena una vez".to_string()
                                } else {
                                    format!("Se repite: {}", model::days_summary(days))
                                },
                            ))
                            .child(quick("days-weekdays", "L–V", "Lunes a viernes", WEEKDAYS))
                            .child(quick("days-all", "Todos", "Todos los días", ALL_DAYS)),
                    ),
            )
            .child(
                v_flex()
                    .items_center()
                    .gap_3()
                    .child(name_field(&self.alarm_label, cx))
                    .child(
                        Button::new("add-alarm")
                            .primary()
                            .large()
                            .rounded(px(999.))
                            .px_6()
                            .icon(IconName::AlarmClockPlus)
                            .label("Agregar alarma")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.add_alarm_from_form(window, cx)
                            })),
                    ),
            )
    }

    /// Una alarma en la lista chica: hora, nombre o días, cuándo suena y el
    /// interruptor.
    fn render_alarm(&self, a: &Alarm, now: Millis, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let id = a.id;
        let ringing = a.ringing_since.is_some();
        let group = format!("alarm-{id}");

        let status = if ringing {
            "Sonando".to_string()
        } else {
            match a.next_at {
                Some(at) if a.snoozed => format!(
                    "Pospuesta hasta las {}",
                    model::to_local(at).format("%H:%M")
                ),
                Some(at) => format!("{} · {}", model::when(at, now), model::until(at - now)),
                None => "Apagada".into(),
            }
        };
        let active = a.enabled || ringing;
        let title = if a.label.trim().is_empty() {
            model::days_summary(a.days)
        } else {
            a.label.trim().to_string()
        };
        let text = if active {
            theme.foreground
        } else {
            theme.muted_foreground
        };

        let row = h_flex()
            .id(("alarm-row", id as usize))
            .group(group.clone())
            .gap_2()
            .items_center()
            .px_2()
            .py_1p5()
            .rounded(theme.radius)
            .hover(|s| s.bg(theme.secondary))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(52.))
                    .text_base()
                    .font_medium()
                    .font_features(tabular())
                    .text_color(if ringing { theme.danger } else { text })
                    .child(a.time_text()),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().truncate().text_sm().text_color(text).child(title))
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(if ringing {
                                theme.danger
                            } else {
                                theme.muted_foreground
                            })
                            .child(status),
                    ),
            )
            .child(
                div().invisible().group_hover(group, |s| s.visible()).child(
                    Button::new(("alarm-delete", id as usize))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Trash)
                        .tooltip("Borrar")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.delete_alarm(id, cx)
                        })),
                ),
            )
            .child(
                Switch::new(("alarm-on", id as usize))
                    .small()
                    .checked(a.enabled)
                    .tooltip(if a.enabled { "Apagar" } else { "Encender" })
                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                        this.set_alarm_enabled(id, *on, cx)
                    })),
            );
        fade_in(row, ("alarm-in", id as usize), self.reduced_motion)
    }
}
