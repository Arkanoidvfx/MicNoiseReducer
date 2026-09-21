use super::*;
use iced::advanced::widget::{Id, Operation, operation};
use iced::widget::{
    self, Space, button, column, container, mouse_area, pick_list, row, scrollable, slider, text,
    text_input,
};
use iced::{Border, Color, Length};

// Query actual layout so keyboard focus remains visible at every window size/DPI.
pub fn reveal_focus() -> Task<Msg> {
    #[derive(Default)]
    struct FocusBounds {
        target: Option<iced::Rectangle>,
        viewport: Option<(iced::Rectangle, iced::Vector)>,
    }
    impl Operation<f32> for FocusBounds {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<f32>)) {
            operate(self);
        }
        fn container(&mut self, id: Option<&Id>, bounds: iced::Rectangle) {
            if id == Some(&Id::new("focused-control")) && self.viewport.is_some() {
                self.target = Some(bounds);
            }
        }
        fn scrollable(
            &mut self,
            id: Option<&Id>,
            bounds: iced::Rectangle,
            _: iced::Rectangle,
            translation: iced::Vector,
            _: &mut dyn operation::Scrollable,
        ) {
            if id == Some(&Id::new("body")) {
                self.viewport = Some((bounds, translation));
            }
        }
        fn finish(&self) -> operation::Outcome<f32> {
            match (self.target, self.viewport) {
                (Some(target), Some((viewport, translation))) => {
                    operation::Outcome::Some(focus_scroll_delta(
                        target.y - translation.y,
                        target.height,
                        viewport.y,
                        viewport.height,
                    ))
                }
                _ => operation::Outcome::None,
            }
        }
    }
    iced::advanced::widget::operate(FocusBounds::default()).then(|y| {
        iced::widget::operation::scroll_by(
            "body",
            iced::widget::operation::AbsoluteOffset { x: 0.0, y },
        )
    })
}
fn focus_scroll_delta(top: f32, height: f32, viewport_top: f32, viewport_height: f32) -> f32 {
    if top < viewport_top + 8.0 {
        top - viewport_top - 8.0
    } else {
        (top + height + 8.0 - viewport_top - viewport_height).max(0.0)
    }
}
pub const BG: Color = Color::from_rgb8(23, 24, 26);
const PANEL: Color = Color::from_rgb8(34, 35, 38);
pub const INK: Color = Color::from_rgb8(242, 237, 227);
const DIM: Color = Color::from_rgb8(159, 159, 164);
const LINE: Color = Color::from_rgb8(60, 61, 65);
pub const ORANGE: Color = Color::from_rgb8(255, 159, 86);
pub const GREEN: Color = Color::from_rgb8(111, 225, 139);
pub const RED: Color = Color::from_rgb8(255, 119, 118);
fn label<'a>(s: impl Into<String>, size: u32, color: Color) -> widget::Text<'a> {
    text(s.into()).size(size).color(color)
}
fn bold<'a>(s: impl Into<String>, size: u32, color: Color) -> widget::Text<'a> {
    label(s, size, color).font(Font {
        weight: iced::font::Weight::Semibold,
        ..Font::with_name("Segoe UI")
    })
}
fn line<'a>() -> Element<'a, Msg> {
    container(Space::new().height(1))
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(LINE.into()),
            ..Default::default()
        })
        .into()
}
fn outline(focused: bool) -> Border {
    Border {
        color: if focused { ORANGE } else { LINE },
        width: if focused { 2.0 } else { 1.0 },
        radius: 8.0.into(),
    }
}
fn action<'a>(
    content: impl Into<Element<'a, Msg>>,
    message: Msg,
    focused: bool,
    accent: bool,
) -> widget::Button<'a, Msg> {
    button(focus_target(content, focused))
        .padding([6, 10])
        .on_press(message)
        .style(move |_, status| {
            let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(
                    (if accent {
                        ORANGE
                    } else if hover {
                        Color::from_rgb8(49, 50, 54)
                    } else {
                        PANEL
                    })
                    .into(),
                ),
                text_color: if accent { BG } else { INK },
                border: outline(focused),
                ..Default::default()
            }
        })
}
fn window_action<'a>(s: &'a str, message: Msg) -> widget::Button<'a, Msg> {
    button(label(s, 16, DIM))
        .width(32)
        .height(32)
        .padding(0)
        .on_press(message)
        .style(|_, status| button::Style {
            background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                .then(|| PANEL.into()),
            text_color: INK,
            border: Border {
                color: LINE,
                width: 1.0,
                radius: 7.0.into(),
            },
            ..Default::default()
        })
}
fn focus_target<'a>(
    content: impl Into<Element<'a, Msg>>,
    focused: bool,
) -> widget::Container<'a, Msg> {
    let content = container(content);
    if focused {
        content.id("focused-control")
    } else {
        content
    }
}
fn frame<'a>(content: impl Into<Element<'a, Msg>>, focused: bool) -> Element<'a, Msg> {
    focus_target(content, focused)
        .padding(4)
        .style(move |_| container::Style {
            border: Border {
                color: if focused { ORANGE } else { Color::TRANSPARENT },
                width: 1.0,
                radius: 5.0.into(),
            },
            ..Default::default()
        })
        .into()
}
fn panel<'a>(content: impl Into<Element<'a, Msg>>) -> widget::Container<'a, Msg> {
    container(content).padding(10).style(|_| container::Style {
        background: Some(PANEL.into()),
        border: Border {
            color: LINE,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    })
}
fn device_style(_: &Theme, status: pick_list::Status) -> pick_list::Style {
    pick_list::Style {
        text_color: INK,
        placeholder_color: DIM,
        handle_color: ORANGE,
        background: if matches!(
            status,
            pick_list::Status::Hovered | pick_list::Status::Opened { .. }
        ) {
            PANEL.into()
        } else {
            BG.into()
        },
        border: Border {
            radius: 5.0.into(),
            ..Border::default()
        },
    }
}
fn slider_style(_: &Theme, status: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (ORANGE.into(), LINE.into()),
            width: 4.0,
            border: Border::default(),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle {
                radius: if matches!(status, slider::Status::Hovered | slider::Status::Dragged) {
                    7.0
                } else {
                    6.0
                },
            },
            background: INK.into(),
            border_width: 0.0,
            border_color: ORANGE,
        },
    }
}
impl App {
    pub fn view(&self, _: window::Id) -> Element<'_, Msg> {
        let logo = widget::Row::with_children([10, 22, 14].into_iter().map(|h| {
            container(Space::new().width(3).height(h))
                .style(|_| container::Style {
                    background: Some(ORANGE.into()),
                    border: Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        }))
        .spacing(3)
        .align_y(iced::Center);
        let title = mouse_area(
            container(
                row![logo, bold("MicNoiseReducer", 18, INK)]
                    .spacing(12)
                    .align_y(iced::Center),
            )
            .width(Length::Fill)
            .height(36)
            .center_y(36),
        )
        .on_press(Msg::Drag);
        let header = row![
            title,
            window_action("−", Msg::Minimize),
            window_action("×", Msg::Hide)
        ]
        .spacing(4)
        .align_y(iced::Center);
        let content = if self.binding.is_some() {
            self.binding_view()
        } else if self.headphone_page {
            self.headphone_view()
        } else if self.details {
            self.settings_view()
        } else if self.rvc_page {
            self.rvc_view()
        } else {
            self.main_view()
        };
        let mut body = column![header, line()].spacing(8).width(Length::Fill);
        if self.binding.is_none() {
            let tab = |title, page, selected| {
                action(
                    label(title, 13, if selected { BG } else { INK }),
                    Msg::Page(page),
                    self.focus == focus::TAB_BASE + page as usize,
                    selected,
                )
            };
            body = body.push(
                row![
                    tab(
                        "Микрофон",
                        0,
                        !self.details && !self.rvc_page && !self.headphone_page
                    ),
                    tab("Наушники", 3, self.headphone_page),
                    tab("Voice Changer", 1, self.rvc_page && !self.details),
                    Space::new().width(Length::Fill),
                    tab("Настройки", 2, self.details),
                ]
                .spacing(6)
                .align_y(iced::Center),
            );
        }
        if !self.message.is_empty() {
            body = body.push(container(label(&self.message, 13, RED)).padding([4, 0]));
        }
        body = body.push(
            scrollable(container(content).padding(iced::Padding {
                right: 10.0,
                ..Default::default()
            }))
            .id("body")
            .width(Length::Fill)
            .height(Length::Fill),
        );
        container(body.height(Length::Fill))
            .padding([8, 12])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(BG.into()),
                text_color: Some(INK),
                border: Border {
                    color: LINE,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
    fn main_view(&self) -> Element<'_, Msg> {
        let full_monitor = if self.monitor_all { self.monitor } else { 0 };
        let output = self
            .output
            .as_ref()
            .map(|d| d.name.as_str())
            .unwrap_or("Выберите выход");
        let route = row![
            container(frame(
                pick_list(self.inputs.as_slice(), self.input.as_ref(), Msg::Input)
                    .placeholder("Выберите микрофон")
                    .text_size(12)
                    .padding([4, 6])
                    .width(Length::Fill)
                    .style(device_style),
                self.focus == focus::effects::INPUT,
            ))
            .width(Length::Fill),
            label("›", 18, ORANGE),
            label("NVIDIA", 12, INK),
            label("›", 18, ORANGE),
            label(output, 12, DIM).width(Length::Fill)
        ]
        .spacing(12)
        .align_y(iced::Center);
        let db = if self.peak > 0.000001 {
            20.0 * self.peak.log10()
        } else {
            -100.0
        };
        let level = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        let meter = widget::progress_bar(0.0..=1.0, level)
            .girth(6)
            .style(move |_| widget::progress_bar::Style {
                background: LINE.into(),
                bar: if level > 0.9 {
                    ORANGE.into()
                } else {
                    GREEN.into()
                },
                border: Border::default(),
            });
        let level_view = column![
            row![
                label("Выходной голос", 12, DIM),
                Space::new().width(Length::Fill),
                label(
                    if db <= -99.0 {
                        "−∞ dBFS".into()
                    } else {
                        format!("{db:.0} dBFS")
                    },
                    13,
                    INK
                )
            ],
            meter,
        ]
        .spacing(5);
        let monitoring = column![
            row![
                action(
                    label(
                        match full_monitor {
                            1 => "Подключение…",
                            2 => "Слышу весь голос",
                            _ => "Слышать весь голос",
                        },
                        13,
                        if full_monitor == 2 { BG } else { INK }
                    ),
                    Msg::Monitor,
                    self.focus == focus::effects::MONITOR,
                    full_monitor == 2
                )
                .on_press_maybe(
                    (!self.busy && !self.quitting && matches!(self.snapshot.state, 2 | 3))
                        .then_some(Msg::Monitor)
                ),
                action(
                    label(key_name(self.keys[10]), 12, INK),
                    Msg::Bind(10),
                    self.focus == focus::effects::MONITOR_BIND,
                    false
                ),
            ]
            .spacing(8)
            .align_y(iced::Center),
            label(
                if self.monitor_all && matches!(self.monitor, 1 | 2) {
                    "Сейчас слышен весь голос; режим эффектов сохранён."
                } else if (self.effects_monitor || self.boost_monitor) && self.monitor == 1 {
                    "Подключение наушников…"
                } else if (self.effects_monitor || self.boost_monitor) && self.monitor == 3 {
                    "Ошибка прослушивания — см. сообщение сверху."
                } else {
                    ""
                },
                11,
                DIM
            ),
        ]
        .spacing(5)
        .width(Length::Fill);
        let output_controls = column![
            row![
                label("Повтор последнего", 13, INK),
                action(
                    label(key_name(self.keys[11]), 12, INK),
                    Msg::Bind(11),
                    self.focus == focus::effects::REPLAY_BIND,
                    false
                )
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                label("Громкость Discord", 13, DIM),
                Space::new().width(Length::Fill),
                label(
                    format!("{:.0}%", self.controls.discord_volume * 100.0),
                    13,
                    INK
                )
            ],
            frame(
                slider(
                    0.0..=100.0,
                    self.controls.discord_volume * 100.0,
                    Msg::DiscordVolume
                )
                .step(1.0_f32)
                .style(slider_style),
                self.focus == focus::effects::DISCORD_VOLUME
            ),
        ]
        .spacing(5)
        .width(Length::Fill);
        column![
            route,
            level_view,
            row![
                column![
                    container(label(format!("Шумоподавление · {:.0}%", self.controls.intensity * 100.0), 14, INK)).height(30).center_y(30),
                    frame(slider(0.0..=200.0, self.controls.intensity * 100.0, Msg::Intensity).step(1.0_f32).style(slider_style), self.focus == focus::effects::INTENSITY),
                ].spacing(5).width(Length::Fill),
                column![
                    row![
                        label(format!("При удержании · {:.0}%", self.controls.alternate_intensity * 100.0), 14, INK),
                        Space::new().width(Length::Fill),
                        action(label(key_name(self.keys[12]), 12, INK), Msg::Bind(12), self.focus == focus::effects::NOISE_BIND, false),
                    ].spacing(8).height(30).align_y(iced::Center),
                    frame(slider(0.0..=200.0, self.controls.alternate_intensity * 100.0, Msg::AlternateIntensity).step(1.0_f32).style(slider_style), self.focus == focus::effects::ALT_INTENSITY),
                ].spacing(5).width(Length::Fill),
            ].spacing(20),
            label("101–200% — запрос вне диапазона NVIDIA. SDK может отклонить его или не усилить эффект.",12,DIM),
            self.effects_table(),
            line(),
            row![monitoring, output_controls].spacing(20),
            row![
                frame(widget::checkbox(self.effects_monitor)
                    .label("Слышать результат эффектов").text_size(13).size(16)
                    .on_toggle(Msg::EffectsMonitor), self.focus == focus::effects::EFFECTS_MONITOR),
                frame(widget::checkbox(self.boost_monitor)
                    .label("Слышать результат эффекта усиления").text_size(13).size(16)
                    .on_toggle(Msg::BoostMonitor), self.focus == focus::effects::BOOST_MONITOR),
            ].spacing(20),
        ]
        .spacing(12)
        .into()
    }
    fn effects_table(&self) -> Element<'_, Msg> {
        let mut rows = column![
            row![
                label("Эффект / значение", 12, DIM).width(Length::Fill),
                label("Хоткей · микрофон", 12, DIM).width(150),
                label("Хоткей · Discord", 12, DIM).width(150)
            ]
            .spacing(12),
            line(),
        ]
        .spacing(8);
        for i in 0..5 {
            use focus::effects::*;
            let (title, value, focus, bind_focus, active) = match i {
                0 => (
                    "Усиление",
                    format!("{:.0}%", self.controls.boost * 100.0),
                    BOOST,
                    BOOST_BIND,
                    self.snapshot.boost_active != 0,
                ),
                1 => (
                    "Высота",
                    format!("{:+} полутонов", self.controls.pitch),
                    PITCH,
                    PITCH_BIND,
                    self.snapshot.pitch_active != 0,
                ),
                2 => (
                    "Замедление",
                    format!("×{:.2}", self.controls.slow),
                    SLOW,
                    SLOW_BIND,
                    (1..=8).contains(&self.phrase_state) && self.phrase_state % 2 == 1,
                ),
                3 => (
                    "Ускорение",
                    format!("×{:.2}", self.controls.fast),
                    FAST,
                    FAST_BIND,
                    (1..=8).contains(&self.phrase_state) && self.phrase_state % 2 == 0,
                ),
                _ => (
                    "Реверс",
                    "После отпускания".into(),
                    focus::NONE,
                    REVERSE_BIND,
                    self.phrase_state >= 9,
                ),
            };
            let control: Element<'_, Msg> = match i {
                0 => frame(
                    slider(100.0..=2000.0, self.controls.boost * 100.0, Msg::Boost)
                        .step(10.0_f32)
                        .style(slider_style),
                    self.focus == focus,
                ),
                1 => frame(
                    slider(-12.0..=12.0, self.controls.pitch as f32, Msg::Pitch)
                        .step(1.0_f32)
                        .style(slider_style),
                    self.focus == focus,
                ),
                2 => frame(
                    slider(50.0..=95.0, self.controls.slow * 100.0, Msg::Slow)
                        .step(5.0_f32)
                        .style(slider_style),
                    self.focus == focus,
                ),
                3 => frame(
                    slider(105.0..=200.0, self.controls.fast * 100.0, Msg::Fast)
                        .step(5.0_f32)
                        .style(slider_style),
                    self.focus == focus,
                ),
                _ => Space::new().into(),
            };
            let mut heading = row![
                bold(title, 14, if active { ORANGE } else { INK }),
                Space::new().width(Length::Fill),
                label(value, 13, INK)
            ]
            .spacing(8)
            .align_y(iced::Center);
            if i == 0 {
                heading = heading.push(frame(
                    widget::checkbox(self.controls.overload)
                        .label("Перегрузка")
                        .size(16)
                        .text_size(12)
                        .on_toggle(Msg::Overload),
                    self.focus == OVERLOAD,
                ));
            }
            let parameter = column![heading, control,].spacing(4).width(Length::Fill);
            let binding = |discord: bool| {
                let lit = active && self.discord_source == discord;
                action(
                    label(
                        key_name(self.keys[i + if discord { 5 } else { 0 }]),
                        12,
                        if lit { BG } else { INK },
                    ),
                    Msg::Bind(i + if discord { 5 } else { 0 }),
                    self.focus
                        == if discord {
                            DISCORD_BIND_BASE + i
                        } else {
                            bind_focus
                        },
                    lit,
                )
                .width(150)
            };
            rows = rows.push(
                row![parameter, binding(false), binding(true)]
                    .spacing(12)
                    .align_y(iced::Center),
            );
            rows = rows.push(line());
        }
        // Values mirror mic::PhraseEffect::State in src/effects.hpp: 1/2 record slow/fast,
        // 3/4 play slow/fast, 5/6 tail capture, 7/8 limit reached, 9 record reverse,
        // 10 reverse pause, 11 reverse limit, 12 play reverse.
        let status = match self.phrase_state {
            1 | 2 | 9 => format!(
                "Запись {:.1} / 10 с · отпустите хоткей",
                self.phrase_seconds
            ),
            10 => "Пауза 0,15 с перед реверсом…".into(),
            3 | 4 | 12 => format!("Воспроизведение · осталось {:.1} с", self.phrase_seconds),
            5 | 6 => "Завершаем последний слог…".into(),
            7 | 8 | 11 => "Записано 10 с · отпустите хоткей".into(),
            _ => String::new(),
        };
        if self.phrase_state != 0 {
            rows = rows.push(
                container(
                    row![
                        label(
                            status,
                            12,
                            if self.phrase_state != 0 { ORANGE } else { DIM }
                        )
                        .width(Length::Fill),
                        action(
                            label("Отмена", 12, INK),
                            Msg::CancelPhrase,
                            self.focus == focus::effects::CANCEL_PHRASE,
                            false
                        )
                        .on_press_maybe((self.phrase_state != 0).then_some(Msg::CancelPhrase)),
                    ]
                    .spacing(8)
                    .align_y(iced::Center),
                )
                .height(44)
                .center_y(44),
            );
        }
        if self.discord_state == 3 {
            rows = rows.push(label(&self.discord_message, 12, RED));
        }
        rows.into()
    }
    fn rvc_view(&self) -> Element<'_, Msg> {
        let rvc_status = match self.snapshot.rvc_state {
            1 => "Загрузка / буферизация модели…".into(),
            2 => format!(
                "Активно · задержка ~{} мс · модель {:.0} мс",
                self.controls.rvc_options.chunk + RVC_SLACK_MS,
                self.snapshot.rvc_latency_ms
            ),
            3 => "Модель не успевает · пауза".into(),
            _ if self.controls.rvc => "Включится вместе с обработкой".into(),
            _ => "Выключено".into(),
        };
        let runtime_action: Element<'_, Msg> = if self.rvc_runtime_installed {
            Space::new().into()
        } else {
            action(
                label(
                    if self.rvc_runtime_installing {
                        "Установка RVC…"
                    } else {
                        "Установить RVC runtime"
                    },
                    13,
                    if self.rvc_runtime_installing {
                        DIM
                    } else {
                        INK
                    },
                ),
                Msg::RvcInstall,
                self.focus == focus::rvc::INSTALL,
                false,
            )
            .on_press_maybe((!self.rvc_runtime_installing).then_some(Msg::RvcInstall))
            .into()
        };
        let mut content = column![
            row![
                bold(
                    "Голос по модели · RVC",
                    15,
                    if self.snapshot.rvc_state == 2 {
                        GREEN
                    } else {
                        INK
                    }
                ),
                Space::new().width(Length::Fill),
                frame(
                    widget::checkbox(self.controls.rvc)
                        .label("Включён")
                        .text_size(13)
                        .size(16)
                        .on_toggle(Msg::Rvc),
                    self.focus == focus::rvc::ENABLE
                )
            ]
            .align_y(iced::Center),
            label(
                rvc_status,
                12,
                if self.snapshot.rvc_state == 3 {
                    RED
                } else {
                    DIM
                }
            ),
            runtime_action,
            row![
                container(frame(
                    pick_list(
                        self.rvc_models.as_slice(),
                        self.rvc_models
                            .iter()
                            .find(|m| m.slot == self.controls.rvc_options.slot)
                            .cloned(),
                        Msg::RvcModel
                    )
                    .placeholder("Выберите модель")
                    .width(Length::Fill),
                    self.focus == focus::rvc::MODEL,
                ))
                .width(Length::Fill),
                action(
                    label(
                        if self.rvc_importing {
                            "Импорт…"
                        } else {
                            "Импорт модели"
                        },
                        13,
                        if self.rvc_importing { DIM } else { INK }
                    ),
                    Msg::RvcImport,
                    self.focus == focus::rvc::IMPORT,
                    false,
                )
                .on_press_maybe(
                    (self.rvc_runtime_installed && !self.rvc_importing).then_some(Msg::RvcImport),
                )
            ]
            .spacing(10)
            .align_y(iced::Center),
            row![
                container(frame(
                    text_input("Название модели", &self.rvc_name)
                        .id("rvc-name")
                        .on_input_maybe(self.rvc_can_manage().then_some(Msg::RvcName))
                        .on_submit_maybe(self.rvc_can_manage().then_some(Msg::RvcRename)),
                    self.focus == focus::rvc::NAME,
                ))
                .width(Length::Fill),
                action(
                    label(
                        "Переименовать",
                        12,
                        if self.rvc_can_manage() { INK } else { DIM },
                    ),
                    Msg::RvcRename,
                    self.focus == focus::rvc::RENAME,
                    false,
                )
                .on_press_maybe(self.rvc_can_manage().then_some(Msg::RvcRename)),
                action(
                    label(
                        if self.rvc_delete_confirm {
                            "Удалить ещё раз"
                        } else {
                            "Удалить"
                        },
                        12,
                        if self.rvc_can_manage() { RED } else { DIM },
                    ),
                    Msg::RvcDelete,
                    self.focus == focus::rvc::DELETE,
                    false,
                )
                .on_press_maybe(self.rvc_can_manage().then_some(Msg::RvcDelete)),
            ]
            .spacing(10)
            .align_y(iced::Center),
            row![
                label(
                    format!(
                        "Тон модели  {:+} полутонов",
                        self.controls.rvc_options.pitch
                    ),
                    13,
                    INK
                )
                .width(250),
                frame(
                    slider(
                        -24.0..=24.0,
                        self.controls.rvc_options.pitch as f32,
                        Msg::RvcPitch
                    )
                    .step(1.0_f32)
                    .style(slider_style),
                    self.focus == focus::rvc::PITCH
                ),
            ]
            .spacing(12)
            .align_y(iced::Center),
            action(
                label(
                    if self.rvc_advanced {
                        "Скрыть параметры"
                    } else {
                        "Дополнительные параметры"
                    },
                    13,
                    INK
                ),
                Msg::RvcAdvanced,
                self.focus == focus::rvc::ADVANCED,
                false
            ),
        ]
        .spacing(10);
        if !self.rvc_import_note.is_empty() {
            content = content.push(label(&self.rvc_import_note, 12, DIM));
        }
        if self.rvc_advanced {
            content = content.push(column![
                    row![
                        label(format!("Влияние индекса  {}%", self.controls.rvc_options.index), 13, INK).width(250),
                        if self.rvc_has_index() {
                            frame(slider(0.0..=100.0, self.controls.rvc_options.index as f32, Msg::RvcIndex).step(1.0_f32).style(slider_style), self.focus == focus::rvc::INDEX)
                        } else { label("Недоступно без .index", 12, DIM).into() },
                    ].spacing(12).align_y(iced::Center),
                    label(if self.rvc_has_index() {
                        "Индекс усиливает сходство с обучающими примерами модели"
                    } else {"У этой модели нет .index — регулятор индекса не влияет на звук"}, 11, DIM),
                    row![
                        label(format!("Вход модели  {}%", self.controls.rvc_options.gain), 13, INK).width(250),
                        frame(slider(50.0..=300.0, self.controls.rvc_options.gain as f32, Msg::RvcGain).step(5.0_f32).style(slider_style), self.focus == focus::rvc::GAIN),
                    ].spacing(12).align_y(iced::Center),
                    row![
                        label("Блок аудио, мс", 13, INK),
                        frame(pick_list(rvc::CHUNKS, Some(self.controls.rvc_options.chunk), Msg::RvcChunk), self.focus == focus::rvc::CHUNK),
                        Space::new().width(Length::Fill),
                        action(label("Обновить модели", 12, INK), Msg::RvcRefresh, self.focus == focus::rvc::REFRESH, false),
                    ].spacing(10).align_y(iced::Center),
                    label("Задержка = блок + 200 мс, всегда постоянная. При лаге модели — тишина, не обычный голос. 100–150 мс: меньше задержка, выше нагрузка.", 11, DIM),

                    label("Только микрофон. Выключение завершает сервер и выгружает модель; повторный запуск требует загрузки.", 11, DIM)
            ].spacing(8));
        }
        panel(content).width(Length::Fill).into()
    }
    fn headphone_view(&self) -> Element<'_, Msg> {
        let locked = matches!(self.headphone_state, 1 | 2) || self.headphone_busy;
        let route = row![
            label("Звук приложений", 12, DIM),
            label("›", 18, ORANGE),
            label(
                if self.headphone_denoise {
                    "NVIDIA"
                } else {
                    "Без обработки"
                },
                12,
                INK
            ),
            label("›", 18, ORANGE),
            container(frame(
                pick_list(
                    self.headphone_outputs(),
                    self.headphone_output.clone(),
                    Msg::HeadphoneOutput
                )
                .placeholder("Выберите наушники")
                .text_size(12)
                .padding([4, 6])
                .width(Length::Fill)
                .style(device_style),
                self.focus == focus::headphones::OUTPUT,
            ))
            .width(Length::Fill),
        ]
        .spacing(12)
        .align_y(iced::Center);
        let mut body = column![
            route,
            row![
                action(label(if locked { "Остановить" } else { "Включить" }, 13, BG), Msg::HeadphoneToggle, self.focus == focus::headphones::TOGGLE, true),
                action(label(if self.headphone_muted { "Вернуть звук" } else { "Без звука" }, 13, INK), Msg::HeadphoneMute, self.focus == focus::headphones::MUTE, false),
                Space::new().width(Length::Fill),
                frame(widget::checkbox(self.headphone_denoise).label("Шумодав NVIDIA").size(16).text_size(13).on_toggle(Msg::HeadphoneNoise), self.focus == focus::headphones::NOISE),
            ].spacing(10).align_y(iced::Center),
            label(format!("Сила шумоподавления · {:.0}%{}", self.headphone_intensity * 100.0, if self.headphone_intensity > 1.0 { " · эксперимент" } else { "" }), 14, INK),
            frame(slider(0.0..=200.0, self.headphone_intensity * 100.0, Msg::HeadphoneIntensity).step(1.0_f32).style(slider_style), self.focus == focus::headphones::INTENSITY),
            label("101–200% — запрос вне диапазона NVIDIA. SDK может отклонить его или не усилить эффект.", 12, DIM),
            line(),
            row![bold("Громкость", 14, INK), Space::new().width(Length::Fill), label(format!("{:.0}%", self.headphone_volume * 100.0), 13, INK)],
            frame(slider(0.0..=100.0, self.headphone_volume * 100.0, Msg::HeadphoneVolume).step(1.0_f32).style(slider_style), self.focus == focus::headphones::VOLUME),
            line(),
            row![bold("Высота", 14, INK), Space::new().width(Length::Fill), label(format!("{:+} полутонов", self.headphone_pitch), 13, INK)],
            frame(slider(-12.0..=12.0, self.headphone_pitch as f32, Msg::HeadphonePitch).step(1.0_f32).style(slider_style), self.focus == focus::headphones::PITCH),
            line(),
            label("Выход приложения в микшере Windows → MicNoiseReducer Headphones. После остановки верните физические наушники.", 11, DIM),
            label("Режим NVIDIA меняется после остановки. Музыка и атмосфера тоже могут подавляться.", 11, DIM),
        ].spacing(12);
        if !self.headphone_message.is_empty() {
            body = body.push(label(&self.headphone_message, 13, RED));
        }
        body.into()
    }
    fn settings_view(&self) -> Element<'_, Msg> {
        let locked = self.running() || self.busy;
        let input = pick_list(self.inputs.as_slice(), self.input.as_ref(), Msg::Input)
            .placeholder("Микрофон отключён / не выбран")
            .width(Length::Fill)
            .text_size(14);
        let output = pick_list(self.outputs.as_slice(), self.output.as_ref(), Msg::Output)
            .placeholder("Выберите выход")
            .width(Length::Fill)
            .text_size(14);
        let route = if locked {
            column![
                label("Микрофон и выход", 12, DIM),
                label(
                    self.input
                        .as_ref()
                        .map(|d| d.name.clone())
                        .unwrap_or_default(),
                    14,
                    INK
                ),
                label(
                    self.output
                        .as_ref()
                        .map(|d| d.name.clone())
                        .unwrap_or_default(),
                    14,
                    INK
                ),
                label(
                    "Остановите обработку для смены устройств и модели.",
                    12,
                    DIM
                )
            ]
            .spacing(10)
        } else {
            column![
                label("Микрофон", 12, DIM),
                frame(input, self.focus == focus::settings::INPUT),
                label("Передать голос в", 12, DIM),
                frame(output, self.focus == focus::settings::OUTPUT)
            ]
            .spacing(8)
        };
        let model: Element<'_, Msg> = if locked {
            label(
                format!("Denoiser v{}  ·  буфер {} мс", self.version, self.buffer),
                14,
                INK,
            )
            .into()
        } else {
            row![
                frame(
                    pick_list([1, 2], Some(self.version), Msg::Version).width(90),
                    self.focus == focus::settings::VERSION
                ),
                label("Модель v2 экспериментальная", 12, DIM),
                Space::new().width(Length::Fill),
                label("Буфер, мс", 12, DIM),
                frame(
                    pick_list([10, 20, 30, 40, 60, 80], Some(self.buffer), Msg::Buffer).width(80),
                    self.focus == focus::settings::BUFFER
                )
            ]
            .spacing(10)
            .align_y(iced::Center)
            .into()
        };
        column![bold("Настройки",24,INK),route,model,

            line(),label(format!("NVIDIA {:.2} мс  ·  очередь {:.1} мс  ·  пропуски {} / {}",self.snapshot.process_ms,self.snapshot.queue_ms,self.snapshot.underruns,self.snapshot.drops),12,DIM),
            label(format!("Pitch: задержка {:.1} мс  ·  максимум обработки {:.2} мс",self.snapshot.pitch_delay_ms,self.snapshot.pitch_max_ms),12,DIM),
            label("Буфер — запас от обрывов, не полная задержка. Pitch добавляет задержку только при удержании.",12,DIM),
            frame(widget::checkbox(self.autostart)
                .label("Держать виртуальный микрофон доступным после входа в Windows")
                .text_size(13)
                .size(16)
                .on_toggle(Msg::Autostart)
                .style(|theme, status| {
                    let mut style = widget::checkbox::primary(theme, status);
                    style.text_color = Some(INK);
                    if self.autostart { style.background = ORANGE.into(); style.icon_color = BG; }
                    style
                }), self.focus == focus::settings::AUTOSTART),
            widget::rule::horizontal(1),
            label(format!("MicNoiseReducer {}", env!("CARGO_PKG_VERSION")), 13, INK),
            label(&self.update_status, 12, if self.update_ready { GREEN } else { DIM }),
            row![
                action(label(if self.update_checking { "Проверка…" } else { "Проверить обновления" },13,INK),Msg::UpdateCheck,self.focus==focus::settings::UPDATE,false)
                    .on_press_maybe((!self.update_checking).then_some(Msg::UpdateCheck)),
                action(label("Обновить сейчас",13,BG),Msg::ApplyUpdate,self.focus==focus::settings::APPLY_UPDATE,true)
                    .on_press_maybe(self.update_ready.then_some(Msg::ApplyUpdate)),
            ].spacing(8),
            row![action(label("Обновить устройства",13,INK),Msg::Refresh,self.focus==focus::settings::REFRESH,false),Space::new().width(Length::Fill),action(label("Выход",13,INK),Msg::Quit,self.focus==focus::settings::QUIT,false),action(label("Готово",13,BG),Msg::Settings,self.focus==focus::settings::DONE,true)].spacing(8)
        ].spacing(12).into()
    }
    fn binding_view(&self) -> Element<'_, Msg> {
        let i = self.binding.unwrap();
        let duplicate = self.candidate != 0
            && self
                .keys
                .iter()
                .enumerate()
                .any(|(j, &k)| j != i && k == self.candidate);
        column![
            bold(
                if i == 12 {
                    "Хоткей второго уровня шумоподавления".into()
                } else if i == 11 {
                    "Хоткей «Повтор последнего»".into()
                } else if i == 10 {
                    "Хоткей «Слышать себя»".into()
                } else {
                    format!(
                        "{} · {}",
                        [
                            "Хоткей усиления",
                            "Хоткей высоты голоса",
                            "Хоткей замедления",
                            "Хоткей ускорения",
                            "Хоткей реверса после фразы"
                        ][i % 5],
                        if i >= 5 {
                            "Discord"
                        } else {
                            "Микрофон"
                        }
                    )
                },
                25,
                INK
            ),
            label(
                "Нажмите клавишу или боковую кнопку мыши. Можно добавить Ctrl, Alt, Shift.",
                14,
                DIM
            ),
            Space::new().height(18),
            bold(
                if self.candidate == 0 {
                    "Жду нажатия…".into()
                } else {
                    key_name(self.candidate)
                },
                32,
                ORANGE
            ),
            label(
                if duplicate {
                    "Это сочетание уже назначено другому эффекту."
                } else {
                    "Назначение не блокирует эту клавишу в игре. Esc — отмена."
                },
                13,
                if duplicate { RED } else { DIM }
            ),
            Space::new().height(18),
            row![
                action(
                    label("Сохранить", 14, BG),
                    Msg::AcceptBind,
                    self.focus == focus::bind::ACCEPT,
                    true
                ),
                action(
                    label("Убрать хоткей", 14, INK),
                    Msg::ClearBind,
                    self.focus == focus::bind::CLEAR,
                    false
                ),
                action(
                    label("Отмена", 14, INK),
                    Msg::CancelBind,
                    self.focus == focus::bind::CANCEL,
                    false
                )
            ]
            .spacing(10)
        ]
        .spacing(16)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focus_scroll_moves_only_when_outside_viewport() {
        assert_eq!(focus_scroll_delta(120.0, 30.0, 100.0, 200.0), 0.0);
        assert_eq!(focus_scroll_delta(90.0, 30.0, 100.0, 200.0), -18.0);
        assert_eq!(focus_scroll_delta(280.0, 30.0, 100.0, 200.0), 18.0);
    }
}
