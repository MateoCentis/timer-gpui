//! Pestaña de ajustes: grupos de filas sobre superficies de vidrio.

use gpui::{
    AnyElement, App, ClickEvent, Context, Div, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, prelude::FluentBuilder as _, px,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    switch::Switch,
    v_flex,
};

use crate::{platform, sound, store};

use super::{
    Temporizador, TogglePin, ToggleTheme,
    view::{section_title, segment, segmented, surface},
};

const RING_OPTIONS: &[(u64, &str)] = &[
    (15, "15 s"),
    (30, "30 s"),
    (60, "1 min"),
    (120, "2 min"),
    (300, "5 min"),
];
const SNOOZE_OPTIONS: &[u64] = &[1, 5, 10, 15];
const OPACITY_OPTIONS: &[(u8, &str)] = &[(100, "Sólida"), (90, "90 %"), (80, "80 %"), (70, "70 %")];

/// Un grupo de filas con su título.
fn group(title: &'static str, rows: Vec<AnyElement>, cx: &App) -> impl IntoElement {
    let border = cx.theme().border;
    v_flex()
        .gap_2()
        .child(div().px_1().child(section_title(title, cx)))
        .child(
            surface(cx).children(rows.into_iter().enumerate().map(|(i, row)| {
                div()
                    .when(i > 0, |this| this.border_t_1().border_color(border))
                    .child(row)
            })),
        )
}

/// Título y explicación de una fila.
fn label(title: &'static str, detail: Option<&'static str>, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .flex_1()
        .min_w_0()
        .child(div().text_sm().child(title))
        .when_some(detail, |this, d| {
            this.child(div().text_xs().text_color(muted).child(d))
        })
}

/// Fila con el control a la derecha.
fn row(
    title: &'static str,
    detail: Option<&'static str>,
    control: impl IntoElement,
    cx: &App,
) -> AnyElement {
    h_flex()
        .gap_3()
        .items_center()
        .px_3()
        .py_3()
        .child(label(title, detail, cx))
        .child(control)
        .into_any_element()
}

/// Fila con el control (un control segmentado) debajo del título.
fn stacked(
    title: &'static str,
    detail: Option<&'static str>,
    control: impl IntoElement,
    cx: &App,
) -> AnyElement {
    v_flex()
        .gap_2()
        .px_3()
        .py_3()
        .child(label(title, detail, cx))
        .child(control)
        .into_any_element()
}

impl Temporizador {
    fn cycle_sound(&mut self, step: isize, cx: &mut Context<Self>) {
        if self.sounds.is_empty() {
            return;
        }
        let n = self.sounds.len() as isize;
        let current = self
            .sounds
            .iter()
            .position(|s| *s == self.settings.sound)
            .map_or(-1, |i| i as isize);
        let next = (current + step).rem_euclid(n) as usize;
        self.settings.sound = self.sounds[next].clone();
        self.settings.save();
        self.preview_sound();
        cx.notify();
    }

    fn preview_sound(&mut self) {
        // No tapar una alarma que está sonando.
        if self.sound_until.is_none() {
            self.settings.sound.preview();
        }
    }

    pub(super) fn render_settings_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sound_picker = h_flex()
            .gap_0p5()
            .items_center()
            .child(
                Button::new("sound-prev")
                    .ghost()
                    .small()
                    .icon(IconName::ChevronLeft)
                    .tooltip("Anterior")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cycle_sound(-1, cx))),
            )
            .child(
                div()
                    .min_w(px(76.))
                    .text_center()
                    .text_sm()
                    .child(self.settings.sound.label()),
            )
            .child(
                Button::new("sound-next")
                    .ghost()
                    .small()
                    .icon(IconName::ChevronRight)
                    .tooltip("Siguiente")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cycle_sound(1, cx))),
            )
            .child(
                Button::new("sound-test")
                    .ghost()
                    .small()
                    .icon(IconName::Play)
                    .tooltip("Probar")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, _| this.preview_sound())),
            )
            .child(
                Button::new("sound-stop")
                    .ghost()
                    .small()
                    .icon(IconName::Square)
                    .tooltip("Detener la prueba")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, _| {
                        if this.sound_until.is_none() {
                            sound::stop();
                        }
                    })),
            );

        let ring_choices = segmented(cx).children(RING_OPTIONS.iter().map(|&(secs, text)| {
            segment(
                ("ring", secs as usize),
                text,
                self.settings.ring_seconds == secs,
                cx,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.settings.ring_seconds = secs;
                this.settings.save();
                cx.notify();
            }))
        }));

        let snooze_choices = segmented(cx).children(SNOOZE_OPTIONS.iter().map(|&mins| {
            segment(
                ("snooze", mins as usize),
                format!("{mins} min"),
                self.settings.snooze_minutes == mins,
                cx,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.settings.snooze_minutes = mins;
                this.settings.save();
                cx.notify();
            }))
        }));

        let theme_choices =
            segmented(cx)
                .w(px(180.))
                .children([("Oscuro", true), ("Claro", false)].map(|(text, dark)| {
                    segment(
                        ("theme", dark as usize),
                        text,
                        self.settings.dark == dark,
                        cx,
                    )
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, window, cx| {
                            if this.settings.dark != dark {
                                this.toggle_theme(&ToggleTheme, window, cx);
                            }
                        },
                    ))
                }));

        let opacity_choices = segmented(cx).children(OPACITY_OPTIONS.iter().map(|&(pct, text)| {
            segment(
                ("opacity", pct as usize),
                text,
                self.settings.opacity == pct,
                cx,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.set_opacity(pct, window, cx)
            }))
        }));

        let has_tray = self.tray.is_some();
        let muted = cx.theme().muted_foreground;

        v_flex()
            .w_full()
            .max_w(px(560.))
            .mx_auto()
            .p_4()
            // Entre grupos, el triple que entre el título y su grupo (8 px).
            .gap_6()
            .child(group(
                "Sonido",
                vec![
                    row("Sonido de alarma", None, sound_picker, cx),
                    stacked("Suena durante", None, ring_choices, cx),
                    stacked("Posponer alarmas", None, snooze_choices, cx),
                ],
                cx,
            ))
            .child(group(
                "Apariencia",
                vec![
                    row("Tema", None, theme_choices, cx),
                    stacked(
                        "Opacidad de la ventana",
                        Some("Por debajo de «Sólida» se ve, desenfocado, lo que hay detrás"),
                        opacity_choices,
                        cx,
                    ),
                    row(
                        "Siempre encima",
                        Some("Ctrl+Shift+T"),
                        Switch::new("on-top")
                            .checked(self.settings.always_on_top)
                            .on_click(cx.listener(|this, on: &bool, window, cx| {
                                if *on != this.settings.always_on_top {
                                    this.toggle_pin(&TogglePin, window, cx);
                                }
                            })),
                        cx,
                    ),
                ],
                cx,
            ))
            .child(group(
                "Ventana",
                vec![
                    row(
                        "Traer al frente al sonar",
                        Some("Muestra la ventana aunque esté en la bandeja"),
                        Switch::new("show-on-ring")
                            .checked(self.settings.show_on_ring)
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                this.settings.show_on_ring = *on;
                                this.settings.save();
                                cx.notify();
                            })),
                        cx,
                    ),
                    row(
                        "Al cerrar, seguir en la bandeja",
                        Some(if has_tray {
                            "Los temporizadores y alarmas siguen sonando"
                        } else {
                            "No se pudo crear el ícono de la bandeja"
                        }),
                        Switch::new("close-to-tray")
                            .checked(self.settings.close_to_tray && has_tray)
                            .on_click(cx.listener(move |this, on: &bool, _, cx| {
                                this.settings.close_to_tray = *on && has_tray;
                                this.settings.save();
                                cx.notify();
                            })),
                        cx,
                    ),
                    row(
                        "Iniciar con Windows",
                        Some("Arranca escondido en la bandeja"),
                        Switch::new("autostart")
                            .checked(self.autostart)
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                if platform::set_autostart(*on) {
                                    this.autostart = platform::autostart_enabled();
                                } else {
                                    this.flash("No se pudo cambiar el inicio con Windows", cx);
                                }
                                cx.notify();
                            })),
                        cx,
                    ),
                ],
                cx,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .px_3()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Los datos se guardan en %APPDATA%\\Temporizador"),
                    )
                    .child(
                        Button::new("open-data")
                            .ghost()
                            .xsmall()
                            .icon(IconName::FolderOpen)
                            .label("Abrir carpeta")
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                let dir = store::data_dir();
                                let _ = std::fs::create_dir_all(&dir);
                                cx.open_with_system(&dir);
                            })),
                    ),
            )
    }
}
