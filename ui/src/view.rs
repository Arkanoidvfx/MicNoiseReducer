use super::*;
use iced::advanced::widget::{Id, Operation, operation};
use iced::widget::{
    self, Space, button, column, container, mouse_area, pick_list, row, scrollable, slider, text,
    text_input,
};
use iced::{Border, Color, Length};

// Keep an off-screen keyboard target mounted without mounting every row on the way to it.
fn sound_rows(count: usize, scroll: f32, viewport: f32, pitch: f32, focus: Option<usize>) -> Vec<usize> {
    // The stored offset can outlive a shorter section/filter until Iced clamps its scrollable.
    let scroll = scroll.min((count as f32 * pitch - viewport).max(0.0));
    let first = (((scroll - 96.0) / pitch).floor().max(0.0) as usize).min(count);
    let last = (((scroll + viewport + 96.0) / pitch).ceil() as usize).min(count).max(first);
    let mut rows: Vec<_> = (first..last).collect();
    if let Some(at) = focus.filter(|&at| at < count)
        && let Err(pos) = rows.binary_search(&at)
    {
        rows.insert(pos, at);
    }
    rows
}

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
fn slider_style_with_opacity(
    status: slider::Status,
    opacity: f32,
    handle_opacity: f32,
) -> slider::Style {
    let fade = |color: Color| Color { a: color.a * opacity, ..color };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (fade(ORANGE).into(), fade(LINE).into()),
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
            background: Color { a: handle_opacity, ..INK }.into(),
            border_width: 0.0,
            border_color: fade(ORANGE),
        },
    }
}
fn slider_style(_: &Theme, status: slider::Status) -> slider::Style {
    slider_style_with_opacity(status, 1.0, 1.0)
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
                row![logo, bold("Mic Noize", 18, INK)]
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
        let content = if self.soundpad_page {
            self.soundpad_view()
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
        {
            let tab = |title, page, selected| {
                action(
                    label(title, 13, if selected { BG } else { INK }),
                    Msg::Page(page),
                    self.focus == focus::tab(page),
                    selected,
                )
            };
            body = body.push(
                row![
                    tab(
                        "Микрофон",
                        0,
                        !self.details && !self.rvc_page && !self.headphone_page && !self.soundpad_page
                    ),
                    tab("Наушники", 3, self.headphone_page),
                    tab("Voice Changer", 1, self.rvc_page && !self.details),
                    tab("Саундпад", 4, self.soundpad_page),
                    Space::new().width(Length::Fill),
                    tab("Настройки", 2, self.details),
                ]
                .spacing(6)
                .align_y(iced::Center),
            );
        }
        // A ready update used to be visible only inside Настройки. It now sits above every
        // page, with the action in the same place as the status.
        if self.update_ready {
            body = body.push(
                panel(
                    row![
                        column![
                            bold("Доступно обновление", 13, ORANGE),
                            label(&self.update_status, 12, DIM)
                        ]
                        .spacing(2),
                        Space::new().width(Length::Fill),
                        action(
                            label("Обновить и перезапустить", 13, BG),
                            Msg::ApplyUpdate,
                            self.focus == focus::UPDATE_BANNER,
                            true
                        )
                    ]
                    .spacing(10)
                    .align_y(iced::Center),
                )
                .width(Length::Fill),
            );
        }
        if !self.message.is_empty() {
            body = body.push(container(label(&self.message, 13, RED)).padding([4, 0]));
        }
        // The soundpad owns its own scrollable list (and the "body" id) so its toolbar and
        // sidebar stay put while hundreds of clips scroll.
        if self.soundpad_page && self.sound_folder.is_some() {
            body = body.push(container(content).width(Length::Fill).height(Length::Fill));
        } else {
            body = body.push(
                scrollable(
                    mouse_area(container(content).padding(iced::Padding {
                        right: 10.0,
                        ..Default::default()
                    }))
                    .on_scroll(|d| Msg::Wheel("body", smooth::wheel_pixels(d))),
                )
                .id("body")
                .width(Length::Fill)
                .height(Length::Fill),
            );
        }
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
                self.bind_button(10, self.keys[10], self.focus == focus::effects::MONITOR_BIND, false, 150.0),
            ]
            .spacing(8)
            .align_y(iced::Center),
            label(
                if self.monitor_all && matches!(self.monitor, 1 | 2) {
                    "Сейчас слышен весь голос; режим эффектов сохранён."
                } else if self.effect_monitoring() && self.monitor == 1 {
                    "Подключение наушников…"
                } else if self.effect_monitoring() && self.monitor == 3 {
                    "Ошибка прослушивания — см. сообщение сверху."
                } else {
                    ""
                },
                11,
                DIM
            ),
            frame(widget::checkbox(self.effects_monitor)
                .label("Слышать результат эффектов").text_size(13).size(16)
                .on_toggle(Msg::EffectsMonitor), self.focus == focus::effects::EFFECTS_MONITOR),
            frame(widget::checkbox(self.boost_monitor)
                .label("Слышать результат эффекта усиления").text_size(13).size(16)
                .on_toggle(Msg::BoostMonitor), self.focus == focus::effects::BOOST_MONITOR),
        ]
        .spacing(5)
        .width(Length::Fill);
        let output_controls = column![
            row![
                label("Повтор последнего", 13, INK),
                self.bind_button(11, self.keys[11], self.focus == focus::effects::REPLAY_BIND, false, 150.0)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                label("Громкость Discord", 13, DIM),
                Space::new().width(Length::Fill),
                label(
                    format!(
                        "{:.0}%",
                        discord_volume_percent(self.controls.discord_volume)
                    ),
                    13,
                    INK
                )
            ],
            frame(
                slider(
                    0.0..=DISCORD_VOLUME_MAX_PERCENT,
                    discord_volume_percent(self.controls.discord_volume),
                    Msg::DiscordVolume
                )
                .step(1.0_f32)
                .style(slider_style),
                self.focus == focus::effects::DISCORD_VOLUME
            ),
        ]
        .spacing(5)
        .width(Length::Fill)
        .push(self.clips_view());
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
                        self.bind_button(12, self.keys[12], self.focus == focus::effects::NOISE_BIND, false, 150.0),
                    ].spacing(8).height(30).align_y(iced::Center),
                    frame(slider(0.0..=200.0, self.controls.alternate_intensity * 100.0, Msg::AlternateIntensity).step(1.0_f32).style(slider_style), self.focus == focus::effects::ALT_INTENSITY),
                ].spacing(5).width(Length::Fill),
            ].spacing(20),
            label("101–200% — запрос вне диапазона NVIDIA. SDK может отклонить его или не усилить эффект.",12,DIM),
            self.effects_table(),
            line(),
            row![monitoring, output_controls].spacing(20),
        ]
        .spacing(12)
        .into()
    }
    /// One recording: play/stop on the left, its save menu on the right.
    fn clip_cell(&self, i: usize, playing: u32) -> Element<'_, Msg> {
        use focus::effects::*;
        let clip = &self.clips[i];
        let lit = playing == Self::clip_id(i);
        let failed = matches!(clip.state, SoundState::Failed(_));
        let chosen = self.clip_menu == Some(i);
        let focused = self.focus == CLIP_BASE + 2 * i;
        let outlined = chosen || focused;
        let play = button(focus_target(
            row![
                // U+25B8 / U+25A0 stay text glyphs; U+25B6 falls back to the colour emoji font.
                label(if lit { "■" } else { "▸" }, 13, if failed { RED } else if lit { ORANGE } else { DIM }).width(11),
                label(super::clip_label(&clip.name), 12, if lit { ORANGE } else { INK }),
            ]
            .spacing(4)
            .align_y(iced::Center),
            focused,
        ))
        .width(Length::Fill)
        .height(CLIP_CELL)
        .padding([0, 6])
        .on_press(Msg::ClipPlay(i))
        .style(move |_, status| button::Style {
            background: Some(
                (if lit {
                    Color::from_rgb8(46, 39, 33)
                } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    Color::from_rgb8(49, 50, 54)
                } else {
                    PANEL
                })
                .into(),
            ),
            text_color: INK,
            border: Border {
                color: if outlined { ORANGE } else { Color::TRANSPARENT },
                width: if outlined { 2.0 } else { 1.0 },
                radius: 6.0.into(),
            },
            ..Default::default()
        });
        let save_focused = self.focus == CLIP_BASE + 2 * i + 1;
        let save = button(focus_target(
            container(label("↓", 13, if chosen { ORANGE } else { DIM })).center_x(Length::Fill),
            save_focused,
        ))
        .width(22)
        .height(CLIP_CELL)
        .padding(0)
        .on_press(Msg::ClipMenu(Some(i)))
        .style(move |_, status| button::Style {
            background: Some(
                (if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    Color::from_rgb8(49, 50, 54)
                } else {
                    PANEL
                })
                .into(),
            ),
            text_color: INK,
            border: Border {
                color: if save_focused { ORANGE } else { Color::TRANSPARENT },
                width: if save_focused { 2.0 } else { 1.0 },
                radius: 6.0.into(),
            },
            ..Default::default()
        });
        row![play, save].spacing(2).width(Length::Fill).into()
    }
    /// The last recordings of the hold effects, three per line so all six fit under the
    /// Discord volume without scrolling. The header line carries the block's title, or the
    /// result of the last save, or the two save targets while a menu is open.
    fn clips_view(&self) -> Element<'_, Msg> {
        use focus::effects::*;
        let head: Element<'_, Msg> = match self.clip_menu.filter(|i| *i < self.clips.len()) {
            Some(i) => row![
                label("Сохранить:", 12, DIM),
                action(
                    label("В папку саундпада", 12, INK),
                    Msg::ClipSave(i, true),
                    self.focus == CLIP_TO_SOUNDPAD,
                    false,
                )
                .padding([2, 8]),
                action(
                    label("В другую папку", 12, INK),
                    Msg::ClipSave(i, false),
                    self.focus == CLIP_TO_FOLDER,
                    false,
                )
                .padding([2, 8]),
            ]
            .spacing(6)
            .align_y(iced::Center)
            .into(),
            None if !self.clip_note.is_empty() => {
                let saved = self.clip_note.starts_with("Сохранено");
                let hint = self.clip_note.starts_with("Выберите");
                label(&self.clip_note, 11, if saved { GREEN } else if hint { DIM } else { RED }).into()
            }
            None => label("Последние записи", 12, DIM).into(),
        };
        let mut list = column![container(head).height(CLIP_CELL).center_y(CLIP_CELL)]
            .spacing(4)
            .width(Length::Fill);
        if self.clips.is_empty() {
            return list
                .push(label(
                    "Появятся после эффектов удержания: высоты, замедления, ускорения или реверса.",
                    11,
                    DIM,
                ))
                .into();
        }
        let playing = self.sound_playing.0;
        for line in (0..self.clips.len()).collect::<Vec<_>>().chunks(CLIPS_PER_LINE) {
            let mut cells = row![].spacing(6);
            for &i in line {
                cells = cells.push(self.clip_cell(i, playing));
            }
            // Keep a short last line in the same column widths as a full one.
            for _ in line.len()..CLIPS_PER_LINE {
                cells = cells.push(Space::new().width(Length::Fill));
            }
            list = list.push(cells);
        }
        list.into()
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
                let target = i + if discord { 5 } else { 0 };
                self.bind_button(
                    target,
                    self.keys[target],
                    self.focus == if discord { DISCORD_BIND_BASE + i } else { bind_focus },
                    lit,
                    150.0,
                )
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
            label("Выход приложения в микшере Windows → Mic Noize Headphones. После остановки верните физические наушники.", 11, DIM),
            label("Режим NVIDIA меняется после остановки. Музыка и атмосфера тоже могут подавляться.", 11, DIM),
        ].spacing(12);
        if !self.headphone_message.is_empty() {
            body = body.push(label(&self.headphone_message, 13, RED));
        }
        body.into()
    }
    fn soundpad_view(&self) -> Element<'_, Msg> {
        use focus::soundpad::*;
        let folder_name = self
            .sound_folder
            .as_ref()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Папка не выбрана".into());
        let toolbar = row![
            label(folder_name, 12, if self.sound_folder.is_some() { INK } else { DIM }).width(Length::Fill),
            action(label("Выбрать папку", 13, INK), Msg::SoundpadFolder, self.focus == FOLDER, false)
                .on_press_maybe((!self.sound_dialog).then_some(Msg::SoundpadFolder)),
            action(label("Добавить звуки", 13, BG), Msg::SoundpadAdd, self.focus == ADD, true)
                .on_press_maybe((!self.sound_dialog && self.sound_folder.is_some()).then_some(Msg::SoundpadAdd)),
            action(label("Обновить", 13, INK), Msg::SoundpadRefresh, self.focus == REFRESH, false)
                .on_press_maybe(self.sound_folder.is_some().then_some(Msg::SoundpadRefresh)),
        ]
        .spacing(8)
        .align_y(iced::Center);
        let (playing_id, position, length) = self.sound_playing;
        let playing = playing_id != 0;
        let controls = row![
            column![
                row![
                    label("Громкость звуков", 13, INK),
                    Space::new().width(Length::Fill),
                    label(format!("{:.0}%", self.sound_volume * 100.0), 13, INK),
                ],
                frame(
                    slider(0.0..=200.0, self.sound_volume * 100.0, Msg::SoundpadVolume)
                        .step(1.0_f32)
                        .style(slider_style),
                    self.focus == VOLUME,
                ),
            ]
            .spacing(5)
            .width(Length::Fill),
            column![
                frame(
                    widget::checkbox(self.sound_monitor)
                        .label("Слышать звуки в наушниках")
                        .text_size(13)
                        .size(16)
                        .on_toggle(Msg::SoundpadHear),
                    self.focus == HEAR,
                ),
                label(self.monitor_hint(), 11, if self.sound_monitor && self.monitor == 3 { RED } else { DIM }),
                row![
                    label("Остановить всё", 13, INK),
                    self.bind_button(super::SOUND_STOP_BIND, self.sound_stop_key, self.focus == STOP_BIND, false, 150.0),
                    action(label("Стоп", 12, if playing { BG } else { DIM }), Msg::SoundpadStop, false, playing)
                        .on_press_maybe(playing.then_some(Msg::SoundpadStop)),
                ]
                .spacing(8)
                .align_y(iced::Center),
            ]
            .spacing(6)
            .width(Length::Fill),
        ]
        .spacing(20)
        .align_y(iced::alignment::Vertical::Top);
        let mut body = column![toolbar, controls].spacing(10);
        if !self.sound_note.is_empty() {
            body = body.push(label(&self.sound_note, 12, if self.sound_note.starts_with("Добавлено") || self.sound_note.starts_with('«') { GREEN } else { DIM }));
        }
        body = body.push(line());
        if self.sound_folder.is_none() {
            return body
                .push(
                    column![
                        bold("Звуки поверх голоса в виртуальный микрофон", 15, INK),
                        label("Выберите папку с mp3, wav, ogg или m4a. Каждому звуку в списке назначается свой хоткей; звуки без хоткея запускаются кнопкой в строке.", 12, DIM),
                        label("Хоткей звука: нажатие играет, повторное нажатие останавливает, быстрое двойное перезапускает с начала.", 12, DIM),
                    ]
                    .spacing(8)
                    .padding([12, 0]),
                )
                .into();
        }
        let visible = self.visible_sounds();
        body = body.push(
            row![
                container(frame(
                    text_input("Поиск по названию", &self.sound_filter)
                        .id("sound-filter")
                        .size(13)
                        .padding([5, 8])
                        .on_input(Msg::SoundpadFilter),
                    self.focus == FILTER,
                ))
                .width(Length::Fill),
                label(
                    if self.sound_filter.trim().is_empty() && self.section == super::Selection::All {
                        format!("{} зв.", self.sounds.len())
                    } else {
                        format!("{} из {}", visible.len(), self.sounds.len())
                    },
                    12,
                    DIM
                ),
                frame(
                    pick_list(SoundSort::ALL, Some(self.sound_sort), Msg::SoundpadSort)
                        .text_size(12)
                        .padding([4, 8])
                        .width(170)
                        .style(device_style),
                    self.focus == SORT,
                ),
            ]
            .spacing(10)
            .align_y(iced::Center),
        );
        let custom = self.custom_section();
        body = body.push(
            row![self.section_sidebar(custom), self.sound_list(&visible, custom, playing_id, position, length)]
                .spacing(12)
                .height(Length::Fill),
        );
        body.height(Length::Fill).into()
    }
    /// Left column: "all", automatic prefix groups, custom sections, and the editor of the
    /// selected custom section. Custom entries are drop targets while a clip is dragged.
    fn section_sidebar(&self, custom: Option<usize>) -> Element<'_, Msg> {
        use focus::soundpad::*;
        let items = self.section_items();
        let mut list = column![].spacing(2);
        for (i, item) in items.iter().enumerate() {
            let selected = item.selection == self.section;
            let target = match item.selection {
                super::Selection::Custom(section) => Some(section),
                _ => None,
            };
            let hot = self.dragging.is_some() && target.is_some() && self.drag_over == target;
            let focused = self.focus == SECTION_BASE + i;
            let entry = button(focus_target(
                row![
                    label(&item.label, 12, if selected { BG } else { INK }).width(Length::Fill),
                    label(item.count.to_string(), 11, if selected { BG } else { DIM }),
                ]
                .spacing(6)
                .align_y(iced::Center),
                focused,
            ))
            .width(Length::Fill)
            .padding([5, 8])
            .on_press(Msg::SectionSelect(i))
            .style(move |_, status| button::Style {
                background: Some(
                    (if selected {
                        ORANGE
                    } else if hot || matches!(status, button::Status::Hovered | button::Status::Pressed) {
                        Color::from_rgb8(49, 50, 54)
                    } else {
                        Color::TRANSPARENT
                    })
                    .into(),
                ),
                text_color: if selected { BG } else { INK },
                border: Border {
                    color: if hot || focused { ORANGE } else { Color::TRANSPARENT },
                    width: if hot { 2.0 } else { 1.0 },
                    radius: 6.0.into(),
                },
                ..Default::default()
            });
            // Drop targets report the cursor; the global mouse release finishes the drop. Every
            // entry is wrapped so the tree (and the sidebar scroll offset) stays stable.
            list = list.push(
                mouse_area(entry)
                    .on_enter(Msg::DragOver(target))
                    .on_exit(Msg::DragOver(None)),
            );
        }
        let mut sidebar = column![
            label(
                if self.dragging.is_some() { "Отпустите на разделе" } else { "Разделы" },
                12,
                if self.dragging.is_some() { ORANGE } else { DIM }
            ),
            scrollable(
                mouse_area(container(list).padding(iced::Padding { right: 10.0, ..Default::default() }))
                    .on_scroll(|d| Msg::Wheel("sections", smooth::wheel_pixels(d))),
            )
                .id("sections")
                .height(Length::Fill)
                .width(Length::Fill),
            action(label("+ Раздел", 12, INK), Msg::SectionAdd, self.focus == SECTION_ADD, false).width(Length::Fill),
        ]
        .spacing(6)
        .width(170)
        .height(Length::Fill);
        if custom.is_some() {
            sidebar = sidebar.push(
                column![
                    frame(
                        text_input(
                            custom.and_then(|i| self.sections.get(i)).map(|s| s.name.as_str()).unwrap_or("Название раздела"),
                            &self.section_name,
                        )
                            .id("section-name")
                            .size(12)
                            .padding([4, 8])
                            .on_input(Msg::SectionName)
                            .on_submit(Msg::SectionRename),
                        self.focus == SECTION_NAME,
                    ),
                    action(label("Удалить раздел", 12, RED), Msg::SectionDelete, self.focus == SECTION_DELETE, false).width(Length::Fill),
                    label("Перетащите звук за ≡ из списка; × в строке убирает его из раздела.", 11, DIM),
                ]
                .spacing(6),
            );
        }
        sidebar.into()
    }
    /// Header plus the virtualised clip list in its own scrollable (id "body" so keyboard
    /// focus reveal keeps working here).
    fn sound_list(&self, visible: &[usize], custom: Option<usize>, playing_id: u32, position: f32, length: f32) -> Element<'_, Msg> {
        use focus::soundpad::*;
        let header = row![
            Space::new().width(18),
            label("Звук", 12, DIM).width(Length::Fill),
            label("Громкость", 12, DIM).width(130),
            label("Хоткей", 12, DIM).width(if custom.is_some() { 150 } else { 120 }),
        ]
        .spacing(10);
        if self.sounds.is_empty() {
            return column![header, label("В папке пока нет mp3, wav, ogg или m4a.", 13, DIM)].spacing(8).into();
        }
        if visible.is_empty() {
            return column![
                header,
                label(
                    if custom.is_some() { "Раздел пуст: перетащите сюда звуки из «Все звуки» за ≡." } else { "Ничего не найдено." },
                    13,
                    DIM
                )
            ]
            .spacing(8)
            .into();
        }
        // Build only the viewport and one keyboard target. Fixed row pitch lets spacers
        // preserve all skipped distances, including the gap to an off-screen focus target.
        // Keys keep row state (shaped text) attached to the same clip as the window slides.
        const PITCH: f32 = ROW_HEIGHT + ROW_SPACING;
        let (scroll, viewport) = self.sound_scroll;
        let focused = self.focus.checked_sub(ROW_BASE)
            .and_then(|f| visible.iter().position(|&i| i == f / 3));
        let mounted = sound_rows(visible.len(), scroll, viewport, PITCH, focused);
        let mut rows: widget::keyed::Column<'_, usize, Msg> = widget::keyed::Column::new().spacing(ROW_SPACING);
        let mut next = 0;
        for at in mounted {
            if at > next {
                rows = rows.push(usize::MAX - next, Space::new().height((at - next) as f32 * PITCH - ROW_SPACING));
            }
            next = at + 1;
            let i = visible[at];
            let sound = &self.sounds[i];
            let lit = playing_id == super::App::sound_id(i);
            let dragged = self.dragging == Some(i);
            let stem = sound.name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&sound.name);
            let status = match &sound.state {
                _ if lit => format!("{} / {}", clock(position), clock(length)),
                SoundState::Loaded(seconds) => clock(*seconds),
                SoundState::Loading => "загрузка…".into(),
                SoundState::Failed(e) => e.clone(),
                SoundState::Unloaded => String::new(),
            };
            let failed = matches!(sound.state, SoundState::Failed(_));
            let play_focused = self.focus == ROW_BASE + 3 * i;
            let volume_active = self.sound_hover == Some(i);
            let grip = mouse_area(
                container(label("≡", 14, if dragged { ORANGE } else { DIM }))
                    .width(18)
                    .height(ROW_HEIGHT)
                    .center_y(ROW_HEIGHT),
            )
            .on_press(Msg::DragStart(i))
            .interaction(iced::mouse::Interaction::Grab);
            let play = button(focus_target(
                row![
                    // U+25B8 / U+25A0 stay text glyphs; U+25B6 falls back to the colour emoji font.
                    label(if lit { "■" } else { "▸" }, 15, if lit { ORANGE } else { DIM }).width(14),
                    label(stem, 13, if lit { ORANGE } else { INK }).width(Length::Fill),
                    label(status, 11, if failed { RED } else if lit { ORANGE } else { DIM }),
                ]
                .spacing(8)
                .align_y(iced::Center),
                play_focused,
            ))
            .width(Length::Fill)
            .height(ROW_HEIGHT)
            .padding([0, 8])
            .on_press(Msg::SoundPlay(i))
            .style(move |_, status| button::Style {
                background: Some(
                    (if lit {
                        Color::from_rgb8(46, 39, 33)
                    } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                        Color::from_rgb8(49, 50, 54)
                    } else {
                        Color::TRANSPARENT
                    })
                    .into(),
                ),
                text_color: INK,
                border: Border {
                    color: if play_focused { ORANGE } else { Color::TRANSPARENT },
                    width: if play_focused { 2.0 } else { 1.0 },
                    radius: 6.0.into(),
                },
                ..Default::default()
            });
            let volume = row![
                frame(
                    slider(0.0..=200.0, sound.volume as f32, move |v| Msg::SoundVolume(i, v))
                        .step(5.0_f32)
                        .width(76)
                        .style(move |_, status| {
                            let active = volume_active
                                || matches!(status, slider::Status::Hovered | slider::Status::Dragged);
                            slider_style_with_opacity(
                                status,
                                if active { 1.0 } else { 0.15 },
                                if active { 1.0 } else { 0.0 },
                            )
                        }),
                    self.focus == ROW_BASE + 3 * i + 1,
                ),
                label(
                    format!("{}%", sound.volume),
                    12,
                    Color {
                        a: if volume_active { 1.0 } else { 0.15 },
                        ..if sound.volume == 100 { DIM } else { INK }
                    },
                )
                .width(38),
            ]
            .spacing(4)
            .align_y(iced::Center)
            .width(130);
            let bind = self.bind_button(
                super::SOUND_BIND_BASE + i,
                sound.key,
                self.focus == ROW_BASE + 3 * i + 2,
                false,
                120.0,
            );
            let mut line = row![grip, play, volume, bind].spacing(10).height(ROW_HEIGHT).align_y(iced::Center);
            if custom.is_some() {
                line = line.push(
                    button(label("×", 14, DIM))
                        .width(24)
                        .padding(0)
                        .on_press(Msg::SoundUnassign(i))
                        .style(|_, status| button::Style {
                            background: matches!(status, button::Status::Hovered | button::Status::Pressed).then(|| PANEL.into()),
                            text_color: RED,
                            border: Border { radius: 6.0.into(), ..Default::default() },
                            ..Default::default()
                        }),
                );
            }
            // A row-sized clip layer lets tiny-skia invalidate the moving row as a whole.
            // Without it, scattered text/slider damage fragments repaint the same list repeatedly.
            rows = rows.push(i, widget::stack![
                Space::new().width(Length::Fill).height(ROW_HEIGHT),
                mouse_area(line)
                    .on_enter(Msg::SoundHover(i, true))
                    .on_exit(Msg::SoundHover(i, false)),
            ].clip(true));
        }
        if next < visible.len() {
            rows = rows.push(usize::MAX - next, Space::new().height((visible.len() - next) as f32 * PITCH - ROW_SPACING));
        }
        column![
            header,
            scrollable(
                mouse_area(container(rows).padding(iced::Padding { right: 10.0, ..Default::default() }))
                    .on_scroll(|d| Msg::Wheel("body", smooth::wheel_pixels(d))),
            )
                .id("body")
                .on_scroll(|v| Msg::SoundpadScroll(v.absolute_offset().y, v.bounds().height))
                .width(Length::Fill)
                .height(Length::Fill),
        ]
        .spacing(6)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
    /// What the headphone monitor is doing for the soundpad, with the level it really renders.
    fn monitor_hint(&self) -> String {
        if !self.sound_monitor {
            return String::new();
        }
        match self.monitor {
            2 if self.monitor_all => "Сейчас слышен весь голос, звуки в нём.".into(),
            2 => {
                let level = if self.monitor_peak > 0.0005 {
                    format!("{:.0} dBFS", 20.0 * self.monitor_peak.log10())
                } else {
                    "тишина".into()
                };
                format!("Наушники: {} · {level}", self.monitor_message)
            }
            1 => "Подключение наушников…".into(),
            3 => "Ошибка прослушивания — см. сообщение сверху.".into(),
            _ if self.running() => "Прослушивание не запущено.".into(),
            _ => "Включится вместе с обработкой микрофона.".into(),
        }
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
        // Only while the virtual microphone is missing: a button that can do nothing is noise.
        let driver_row: Element<'_, Msg> = if self.driver_ready {
            Space::new().height(0).into()
        } else {
            column![
                label(
                    "Виртуальный микрофон не установлен: Windows запросит права администратора.",
                    12,
                    DIM
                ),
                action(
                    label(
                        if self.driver_installing { "Устанавливаем…" } else { "Установить виртуальный микрофон" },
                        13,
                        INK
                    ),
                    Msg::InstallDriver,
                    self.focus == focus::settings::DRIVER,
                    false
                )
                .on_press_maybe((!self.driver_installing).then_some(Msg::InstallDriver))
            ]
            .spacing(6)
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
            driver_row,
            widget::rule::horizontal(1),
            label(format!("Mic Noize {}", env!("CARGO_PKG_VERSION")), 13, INK),
            label(&self.update_status, 12, if self.update_ready { GREEN } else { DIM }),
            row![
                action(label(if self.update_checking { "Проверка…" } else { "Проверить обновления" },13,INK),Msg::UpdateCheck,self.focus==focus::settings::UPDATE,false)
                    .on_press_maybe((!self.update_checking).then_some(Msg::UpdateCheck)),
                action(label("Обновить сейчас",13,BG),Msg::ApplyUpdate,self.focus==focus::settings::APPLY_UPDATE,true)
                    .on_press_maybe(self.update_ready.then_some(Msg::ApplyUpdate)),
                Space::new().width(Length::Fill),
                action(label(if self.report_sending { "Отправляем…" } else { "Отправить логи разработчику" },13,INK),Msg::SendReport,self.focus==focus::settings::REPORT,false)
                    .on_press_maybe((!self.report_sending).then_some(Msg::SendReport)),
            ].spacing(8),
            row![action(label("Обновить устройства",13,INK),Msg::Refresh,self.focus==focus::settings::REFRESH,false),Space::new().width(Length::Fill),action(label("Выход",13,INK),Msg::Quit,self.focus==focus::settings::QUIT,false),action(label("Готово",13,BG),Msg::Settings,self.focus==focus::settings::DONE,true)].spacing(8)
        ].spacing(12).into()
    }
    /// A hotkey button that captures its key in place: click, press the key, done. A clash
    /// stays on the button in red and keeps waiting; a second click or Esc cancels; the small
    /// cross clears an existing binding while capturing.
    fn bind_button(&self, target: usize, key: u32, focused: bool, lit: bool, width: f32) -> Element<'_, Msg> {
        if self.binding != Some(target) {
            // An unassigned slot is a placeholder, not a value: 30 % ink so bound keys stand out.
            let color = if lit {
                BG
            } else if key == 0 {
                Color { a: 0.3, ..INK }
            } else {
                INK
            };
            return action(label(key_name(key), 12, color), Msg::Bind(target), focused, lit)
                .width(width)
                .into();
        }
        let (text, color) = match self.bind_conflict {
            Some(taken) => (format!("Занято: {}", key_name(taken)), RED),
            None => ("Нажмите…".into(), ORANGE),
        };
        let capture = button(focus_target(label(text, 12, color), focused))
            .padding([6, 10])
            .width(Length::Fill)
            .on_press(Msg::Bind(target))
            .style(|_, _| button::Style {
                background: Some(BG.into()),
                text_color: ORANGE,
                border: Border {
                    color: ORANGE,
                    width: 2.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            });
        if key == 0 {
            return container(capture).width(width).into();
        }
        row![
            capture,
            button(label("×", 14, RED))
                .width(24)
                .padding([5, 0])
                .on_press(Msg::ClearBind)
                .style(|_, status| button::Style {
                    background: matches!(status, button::Status::Hovered | button::Status::Pressed).then(|| PANEL.into()),
                    text_color: RED,
                    border: Border { radius: 6.0.into(), ..Default::default() },
                    ..Default::default()
                }),
        ]
        .spacing(4)
        .align_y(iced::Center)
        .width(width)
        .into()
    }
}

const ROW_HEIGHT: f32 = 30.0;
/// Recordings block: three cells per line, two lines, so all six fit under the Discord volume.
const CLIP_CELL: f32 = 22.0;
const CLIPS_PER_LINE: usize = 3;
const ROW_SPACING: f32 = 2.0;
/// m:ss for clip lengths and playback position.
fn clock(seconds: f32) -> String {
    let whole = seconds.max(0.0).round() as u32;
    format!("{}:{:02}", whole / 60, whole % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn soundpad_scroll_timing() {
        use iced::advanced::{Renderer as _, Layout, graphics::{damage, Viewport}};
        let folder = std::env::var("MNR_SCROLL_BENCH_FOLDER").ok();
        let settings = folder.as_ref().map(|f| format!("[soundpad]\nfolder={f}")).unwrap_or_default();
        let (mut app, _) = App::from_settings(Settings::for_test(&settings))
            .unwrap().unwrap();
        if folder.is_none() {
            app.sound_folder = Some(PathBuf::from("test sounds"));
            app.sounds = (0..100).map(|i| Sound {
                name: format!("Section {} - Sound {i:03}.wav", i / 5), path: PathBuf::new(),
                key: 0, volume: 100, played: 0, modified: 0, state: SoundState::Unloaded,
            }).collect();
        }
        let scale: f32 = std::env::var("MNR_SCROLL_BENCH_SCALE").ok()
            .map(|s| s.parse().unwrap()).unwrap_or(1.0);
        let size = Size::new((1100.0 * scale) as u32, (900.0 * scale) as u32);
        app.soundpad_page = true;
        let mut renderer = iced::Renderer::new(Font::with_name("Segoe UI"), iced::Pixels(14.0));
        let mut tree = iced::advanced::widget::Tree::empty();
        let limits = iced::advanced::layout::Limits::new(Size::ZERO, Size::new(1100.0, 900.0));
        for id in ["sections", "body"] {
            let mut samples = Vec::new();
            let mut raster = Vec::new();
            let mut regions = Vec::new();
            let mut previous = Vec::new();
            let mut pixels = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
            let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
            let viewport = Viewport::with_physical_size(size, scale);
            for frame in 0..if folder.is_some() { 90 } else { 6 } {
                let offset = frame as f32 * 7.3;
                app.sound_scroll = (if id == "body" { offset } else { 0.0 }, 600.0);
                let start = Instant::now();
                let mut element = app.view(window::Id::unique());
                tree.diff(element.as_widget());
                let layout = element.as_widget_mut().layout(&mut tree, &renderer, &limits);
                let mut scroll = iced::advanced::widget::operation::scrollable::scroll_to::<()>(
                    Id::new(id), iced::widget::scrollable::AbsoluteOffset { x: None, y: Some(offset) });
                element.as_widget_mut().operate(&mut tree, Layout::new(&layout), &renderer, &mut scroll);
                if frame > 0 { samples.push(start.elapsed().as_secs_f64() * 1000.0); }
                let start = Instant::now();
                renderer.reset(iced::Rectangle::with_size(Size::new(1100.0, 900.0)));
                element.as_widget().draw(&tree, &mut renderer, &Theme::Dark,
                    &iced::advanced::renderer::Style { text_color: INK }, Layout::new(&layout),
                    iced::mouse::Cursor::Unavailable, &iced::Rectangle::with_size(Size::new(1100.0, 900.0)));
                let changes = damage::group(damage::diff(&previous, renderer.layers(),
                    |layer| vec![layer.bounds], iced_tiny_skia::Layer::damage),
                    iced::Rectangle::with_size(Size::new(1100.0, 900.0)));
                previous = renderer.layers().to_vec();
                renderer.draw(&mut pixels.as_mut(), &mut mask, &viewport, &changes, BG);
                if frame > 0 && !changes.is_empty() {
                    regions.push(changes.len());
                    raster.push(start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            samples.sort_by(f64::total_cmp);
            raster.sort_by(f64::total_cmp);
            eprintln!("{id}, {} clips: view/diff/layout median {:.2} ms, p95 {:.2} ms",
                app.sounds.len(), samples[samples.len()/2], samples[samples.len()*95/100]);
            regions.sort();
            if !regions.is_empty() {
                eprintln!("{id}: draw/damage raster median {:.2} ms, p95 {:.2} ms; damage regions median {}",
                    raster[raster.len()/2], raster[raster.len()*95/100], regions[regions.len()/2]);
            }
            if id == "body" {
                assert!(!regions.is_empty());
                assert!(regions[regions.len()/2] <= 4, "fragmented row damage: {regions:?}");
                if let Ok(path) = std::env::var("MNR_SCROLL_BENCH_IMAGE") {
                    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), size.width, size.height);
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder.write_header().unwrap().write_image_data(pixels.data()).unwrap();
                }
            }
        }
        // Same damage grouping and rasterizer as the window compositor; excludes OS presentation.
    }
    #[test]
    fn sound_rows_stay_bounded_with_distant_focus() {
        // A selected first clip must not mount the 400 intervening rows while scrolling.
        let rows = sound_rows(1000, 12_800.0, 640.0, 32.0, Some(0));
        assert_eq!(rows, std::iter::once(0).chain(397..423).collect::<Vec<_>>());
        // Keyboard navigation still has a mounted target on either side of the viewport.
        assert_eq!(sound_rows(1000, 0.0, 640.0, 32.0, Some(999)),
            (0..23).chain(std::iter::once(999)).collect::<Vec<_>>());
        assert_eq!(sound_rows(1000, 0.0, 640.0, 32.0, Some(2)), (0..23).collect::<Vec<_>>());
        // Filtering or selecting a short section must mount its clips immediately.
        assert_eq!(sound_rows(3, 12_800.0, 640.0, 32.0, None), vec![0, 1, 2]);
        assert_eq!(sound_rows(3, 12_800.0, 640.0, 32.0, Some(1)), vec![0, 1, 2]);
        assert!(sound_rows(0, 0.0, 640.0, 32.0, Some(0)).is_empty());
    }
    #[test]
    fn clock_formats_minutes() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(7.4), "0:07");
        assert_eq!(clock(125.6), "2:06");
    }
    #[test]
    fn focus_scroll_moves_only_when_outside_viewport() {
        assert_eq!(focus_scroll_delta(120.0, 30.0, 100.0, 200.0), 0.0);
        assert_eq!(focus_scroll_delta(90.0, 30.0, 100.0, 200.0), -18.0);
        assert_eq!(focus_scroll_delta(280.0, 30.0, 100.0, 200.0), 18.0);
    }
}
