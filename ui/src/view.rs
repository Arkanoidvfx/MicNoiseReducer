use super::*;
use crate::tacho::{self, Clock, tacho};
use iced::advanced::widget::{Id, Operation, operation};
use iced::widget::{
    self, Space, button, column, container, mouse_area, pick_list, row, scrollable, text,
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

// 0.2.8 palette: graphite surfaces, one warm accent, red only for the risky zone.
pub const BG: Color = Color::from_rgb8(0x15, 0x16, 0x19);
const RAIL: Color = Color::from_rgb8(0x11, 0x12, 0x14);
const CARD: Color = Color::from_rgb8(0x1B, 0x1C, 0x1F);
const CARD2: Color = Color::from_rgb8(0x22, 0x23, 0x27);
const HOVER: Color = Color::from_rgb8(0x2A, 0x2B, 0x30);
const LINE: Color = Color::from_rgb8(0x26, 0x27, 0x2B);
const EDGE: Color = Color::from_rgb8(0x3A, 0x3B, 0x41);
pub const INK: Color = Color::from_rgb8(242, 237, 227);
const DIM: Color = Color::from_rgb8(0xA3, 0xA3, 0xA9);
const FAINT: Color = Color::from_rgb8(0x85, 0x86, 0x8D);
pub const ORANGE: Color = Color::from_rgb8(255, 159, 86);
const ORANGE_DARK: Color = Color::from_rgb8(0x1A, 0x12, 0x06);
pub const GREEN: Color = Color::from_rgb8(111, 225, 139);
pub const RED: Color = Color::from_rgb8(255, 119, 118);
const LIVE_BG: Color = Color::from_rgb8(0x1F, 0x1B, 0x18);

/// Segoe MDL2 Assets ships with Windows 10 and 11; its glyphs replace hand-drawn icons.
mod glyph {
    pub const MIC: &str = "\u{E720}";
    pub const HEADPHONES: &str = "\u{E7F6}";
    pub const SETTINGS: &str = "\u{E713}";
    pub const EFFECTS: &str = "\u{E945}";
    pub const SOUNDPAD: &str = "\u{E8A9}";
    pub const VOICE: &str = "\u{E77B}";
    pub const CHIP: &str = "\u{E9F5}";
    pub const OUTPUT: &str = "\u{E8BD}";
    pub const LOCK: &str = "\u{E72E}";
    pub const RIGHT: &str = "\u{E76C}";
    pub const DOWN: &str = "\u{E70D}";
    pub const PLAY: &str = "\u{E768}";
    pub const STOP: &str = "\u{E71A}";
    pub const SAVE: &str = "\u{E896}";
    pub const VOLUME: &str = "\u{E767}";
    pub const BOOST: &str = "\u{E995}";
    pub const NOTE: &str = "\u{E8D6}";
    pub const SLOW: &str = "\u{EC49}";
    pub const FAST: &str = "\u{EC4A}";
    pub const REVERSE: &str = "\u{E7A7}";
    pub const REPEAT: &str = "\u{E8EE}";
    pub const CLOSE: &str = "\u{E8BB}";
    pub const MINIMIZE: &str = "\u{E921}";
    pub const WARNING: &str = "\u{E7BA}";
    pub const FOLDER: &str = "\u{E838}";
    pub const REFRESH: &str = "\u{E72C}";
    pub const CHECK: &str = "\u{E73E}";
    pub const LOGS: &str = "\u{E9D9}";
    pub const BACK: &str = "\u{E72B}";
}
fn icon<'a>(glyph: &'a str, size: u32, color: Color) -> widget::Text<'a> {
    text(glyph).size(size).color(color).font(Font::with_name("Segoe MDL2 Assets"))
}
fn label<'a>(s: impl Into<String>, size: u32, color: Color) -> widget::Text<'a> {
    text(s.into()).size(size).color(color)
}
fn bold<'a>(s: impl Into<String>, size: u32, color: Color) -> widget::Text<'a> {
    label(s, size, color).font(Font {
        weight: iced::font::Weight::Semibold,
        ..Font::with_name("Segoe UI")
    })
}
fn title<'a>(s: &'a str) -> widget::Text<'a> {
    bold(s, 22, INK)
}
fn numbers<'a>(s: impl Into<String>, size: u32, color: Color) -> widget::Text<'a> {
    label(s, size, color).font(tacho::numbers())
}
fn outline(focused: bool) -> Border {
    Border {
        color: if focused { ORANGE } else { EDGE },
        width: if focused { 2.0 } else { 1.0 },
        radius: 8.0.into(),
    }
}
/// A button: accent is the page's single primary action, the rest are quiet.
fn action<'a>(
    content: impl Into<Element<'a, Msg>>,
    message: Msg,
    focused: bool,
    accent: bool,
) -> widget::Button<'a, Msg> {
    button(focus_target(content, focused))
        .padding([7, 12])
        .on_press(message)
        .style(move |_, status| {
            let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let disabled = matches!(status, button::Status::Disabled);
            button::Style {
                background: Some(
                    (if accent && disabled {
                        Color { a: 0.35, ..ORANGE }
                    } else if accent && hover {
                        Color::from_rgb8(0xFF, 0xB0, 0x70)
                    } else if accent {
                        ORANGE
                    } else if disabled {
                        BG
                    } else if hover {
                        HOVER
                    } else {
                        CARD2
                    })
                    .into(),
                ),
                text_color: if accent { ORANGE_DARK } else { INK },
                border: if accent && !focused {
                    Border { radius: 8.0.into(), ..Border::default() }
                } else {
                    outline(focused)
                },
                ..Default::default()
            }
        })
}
fn icon_button<'a>(glyph: &'a str, message: Msg, focused: bool) -> widget::Button<'a, Msg> {
    button(focus_target(container(icon(glyph, 14, DIM)).center(Length::Fill), focused))
        .width(34)
        .height(34)
        .padding(0)
        .on_press(message)
        .style(move |_, status| button::Style {
            background: Some(
                (if matches!(status, button::Status::Hovered | button::Status::Pressed) { HOVER } else { CARD2 }).into(),
            ),
            text_color: INK,
            border: outline(focused),
            ..Default::default()
        })
}
fn caption_button<'a>(glyph: &'a str, message: Msg, danger: bool) -> widget::Button<'a, Msg> {
    button(container(icon(glyph, 10, DIM)).center(Length::Fill))
        .width(46)
        .height(46)
        .padding(0)
        .on_press(message)
        .style(move |_, status| {
            let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: hover.then(|| (if danger { Color::from_rgb8(0xC4, 0x2B, 0x1C) } else { HOVER }).into()),
                text_color: INK,
                ..Default::default()
            }
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
/// Any element painted offscreen at 1/[`tacho::MOSAIC_CELL`] scale, as RGB cells.
pub fn mosaic_of<'a, M: 'a>(mut element: Element<'a, M>, area: Size) -> Option<std::sync::Arc<tacho::Mosaic>> {
    use iced::advanced::{Layout, Renderer as _, graphics::Viewport};
    let (w, h) = ((area.width / tacho::MOSAIC_CELL).ceil() as u32, (area.height / tacho::MOSAIC_CELL).ceil() as u32);
    let mut renderer = iced::Renderer::new(Font::with_name("Segoe UI"), iced::Pixels(14.0));
    let mut tree = iced::advanced::widget::Tree::empty();
    tree.diff(element.as_widget());
    let layout = element.as_widget_mut().layout(&mut tree, &renderer, &iced::advanced::layout::Limits::new(Size::ZERO, area));
    let full = iced::Rectangle::with_size(area);
    renderer.reset(full);
    element.as_widget().draw(&tree, &mut renderer, &Theme::Dark, &iced::advanced::renderer::Style { text_color: INK }, Layout::new(&layout), iced::mouse::Cursor::Unavailable, &full);
    let mut pixels = tiny_skia::Pixmap::new(w, h)?;
    let mut mask = tiny_skia::Mask::new(w, h)?;
    renderer.draw(&mut pixels.as_mut(), &mut mask, &Viewport::with_physical_size(Size::new(w, h), 1.0 / tacho::MOSAIC_CELL), &[full], BG);
    // The renderer writes BGRA.
    let cells = pixels.data().as_chunks::<4>().0.iter().map(|p| [p[2], p[1], p[0]]).collect();
    Some(std::sync::Arc::new(tacho::Mosaic { width: w as usize, height: h as usize, cells }))
}

/// The update window (design variant A): the app's mark, what is happening, the versions in
/// large type and the running bar.
pub const UPDATE_CARD: Size = Size::new(440.0, 176.0);
pub fn update_card<'a, M: 'a>(stage: tacho::BarStage, from: &str, to: &str) -> Element<'a, M> {
    let (title_text, version, color) = match stage {
        tacho::BarStage::Done => ("Готово", to.to_owned(), GREEN),
        tacho::BarStage::Launching => ("Запускаем Mic Noize", to.to_owned(), GREEN),
        _ => ("Обновляем Mic Noize", format!("{from} → {to}"), ORANGE),
    };
    container(column![
        container(row![tacho::logo(18.0, 0.0), bold("Mic Noize", 13, INK)].spacing(10).align_y(iced::Center)).padding([0, 14]).center_y(38),
        container(Space::new().height(1)).width(Length::Fill).style(|_| container::Style { background: Some(LINE.into()), ..Default::default() }),
        column![column![label(title_text, 14, DIM), numbers(version, 28, color)].spacing(2), tacho::run_bar(stage)]
            .spacing(16)
            .padding(iced::Padding { top: 18.0, right: 20.0, bottom: 22.0, left: 20.0 }),
    ])
    .width(UPDATE_CARD.width)
    .height(UPDATE_CARD.height)
    .style(|_| container::Style {
        background: Some(Color::from_rgb8(0x15, 0x16, 0x19).into()),
        border: Border { color: Color::from_rgb8(0x2A, 0x2B, 0x30), width: 1.0, radius: 12.0.into() },
        ..Default::default()
    })
    .into()
}
/// The update card centred on the colour key, so a keyed window shows only the card.
pub fn update_card_on_key<'a, M: 'a>(stage: tacho::BarStage, from: &str, to: &str) -> Element<'a, M> {
    container(update_card(stage, from, to))
        .center(Length::Fill)
        .style(|_| container::Style { background: Some(tacho::KEY.into()), ..Default::default() })
        .into()
}

/// tiny-skia repaints only damaged regions and places vertically centred control text from its
/// anchor down, so a pick_list or text_input whose label changes would keep the top half of
/// the old one (hovering repaints it). An invisible background that changes with the label
/// damages the whole control instead.
fn repaint<'a>(key: impl std::hash::Hash, content: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    let h = BuildHasherDefault::<DefaultHasher>::default().hash_one(key);
    let byte = |shift: u32| ((h >> shift) & 0xFF) as f32 / 255.0;
    let tint = Color { r: byte(0), g: byte(8), b: byte(16), a: 0.0 };
    container(content).style(move |_| container::Style { background: Some(tint.into()), ..Default::default() }).into()
}
fn frame<'a>(content: impl Into<Element<'a, Msg>>, focused: bool) -> Element<'a, Msg> {
    focus_target(content, focused)
        .padding(3)
        .style(move |_| container::Style {
            border: Border {
                color: if focused { ORANGE } else { Color::TRANSPARENT },
                width: if focused { 2.0 } else { 1.0 },
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
}
fn card<'a>(content: impl Into<Element<'a, Msg>>) -> widget::Container<'a, Msg> {
    container(content).padding([14, 16]).width(Length::Fill).style(|_| container::Style {
        background: Some(CARD.into()),
        border: Border { color: LINE, width: 1.0, radius: 10.0.into() },
        ..Default::default()
    })
}
fn tile<'a>(glyph: &'a str, size: f32, accent: bool) -> Element<'a, Msg> {
    container(icon(glyph, (size * 0.45) as u32, if accent { ORANGE } else { FAINT }))
        .width(size)
        .height(size)
        .center(size)
        .style(move |_| container::Style {
            background: Some(
                (if accent { Color { a: 0.13, ..ORANGE } } else { Color::from_rgb8(0x25, 0x26, 0x2A) }).into(),
            ),
            border: Border { radius: 9.0.into(), ..Border::default() },
            ..Default::default()
        })
        .into()
}
fn heading_row<'a>(glyph: &'a str, name: &'a str, right: Element<'a, Msg>) -> Element<'a, Msg> {
    row![icon(glyph, 14, DIM), bold(name, 13, INK), Space::new().width(Length::Fill), right]
        .spacing(8)
        .height(24)
        .align_y(iced::Center)
        .into()
}
fn device_style(_: &Theme, status: pick_list::Status) -> pick_list::Style {
    let open = matches!(status, pick_list::Status::Hovered | pick_list::Status::Opened { .. });
    pick_list::Style {
        text_color: INK,
        placeholder_color: FAINT,
        handle_color: DIM,
        background: BG.into(),
        border: Border {
            color: if open { ORANGE } else { Color::from_rgb8(0x45, 0x46, 0x4D) },
            width: 1.0,
            radius: 7.0.into(),
        },
    }
}
fn input_style(_: &Theme, status: text_input::Status) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    let hover = matches!(status, text_input::Status::Hovered);
    text_input::Style {
        background: BG.into(),
        border: Border {
            color: if focused { ORANGE } else if hover { Color::from_rgb8(0x6B, 0x6C, 0x73) } else { Color::from_rgb8(0x2E, 0x2F, 0x34) },
            width: 1.0,
            radius: 7.0.into(),
        },
        icon: DIM,
        placeholder: FAINT,
        value: INK,
        selection: Color { a: 0.35, ..ORANGE },
    }
}
fn switch<'a>(on: bool, message: impl Fn(bool) -> Msg + 'a, enabled: bool) -> widget::Toggler<'a, Msg> {
    widget::toggler(on)
        .size(18)
        .on_toggle_maybe(enabled.then_some(message))
        .style(|_, status| {
            use widget::toggler::Status::*;
            let (Active { is_toggled } | Hovered { is_toggled } | Disabled { is_toggled }) = status;
            widget::toggler::Style {
                background: (if is_toggled { ORANGE } else { Color::from_rgb8(0x34, 0x35, 0x3A) }).into(),
                background_border_width: 0.0,
                background_border_color: Color::TRANSPARENT,
                foreground: (if is_toggled { ORANGE_DARK } else { DIM }).into(),
                foreground_border_width: 0.0,
                foreground_border_color: Color::TRANSPARENT,
                text_color: Some(DIM),
                border_radius: None,
                padding_ratio: 0.18,
            }
        })
}
/// Segmented level meter, the same vocabulary as the sliders.
fn meter<'a>(level: f32, color: Color) -> Element<'a, Msg> {
    let segments = 40usize;
    let lit = (level.clamp(0.0, 1.0) * segments as f32).round() as usize;
    widget::Row::with_children((0..segments).map(|i| {
        let fill = if i < lit { color } else { Color::from_rgb8(0x26, 0x27, 0x2B) };
        container(Space::new().width(Length::Fill).height(10))
            .width(Length::Fill)
            .style(move |_| container::Style { background: Some(fill.into()), ..Default::default() })
            .into()
    }))
    .spacing(2)
    .width(Length::Fill)
    .into()
}
/// The hero recording's bars, as in the mockup: a stable pseudo-waveform per clip (the real
/// samples are not decoded for the UI), filled with the slider gradient as it plays.
fn waveform<'a>(name: &str, progress: Option<f32>) -> Element<'a, Msg> {
    const BARS: usize = 30;
    let seed = name.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) % 997;
    let seed = seed as f32 / 97.0;
    let done = progress.map_or(0, |p| (p * BARS as f32).round() as usize);
    widget::Row::with_children((0..BARS).map(|i| {
        let t = i as f32;
        let h = 5.0 + 24.0 * ((t * 0.55 + seed).sin() * (t * 0.21 + seed * 1.7).cos()).abs();
        let fill = if i < done {
            tacho::lerp(t / BARS as f32)
        } else if progress.is_some() {
            Color::from_rgb8(0x5A, 0x5B, 0x61)
        } else {
            Color::from_rgb8(0x4A, 0x4B, 0x51)
        };
        container(Space::new().width(Length::Fill).height(h.round()))
            .width(Length::Fill)
            .style(move |_| container::Style { background: Some(fill.into()), border: Border { radius: 1.0.into(), ..Border::default() }, ..Default::default() })
            .into()
    }))
    .spacing(3)
    .align_y(iced::Center)
    .width(Length::Fill)
    .into()
}
fn panel_row<'a>(name: Element<'a, Msg>, control: Element<'a, Msg>) -> widget::Row<'a, Msg> {
    row![container(name).width(112), control].spacing(12).align_y(iced::Center).height(34)
}
fn effect_grid<'a>(a: Element<'a, Msg>, b: Element<'a, Msg>, c: Element<'a, Msg>, d: Element<'a, Msg>, e: Element<'a, Msg>) -> widget::Row<'a, Msg> {
    row![
        container(a).width(36),
        container(b).width(128),
        container(c).width(Length::Fill),
        container(d).width(150),
        container(e).width(150),
    ]
    .spacing(14)
    .align_y(iced::Center)
}
fn db(peak: f32) -> f32 {
    if peak > 0.000001 { 20.0 * peak.log10() } else { -100.0 }
}
fn db_text(peak: f32) -> String {
    let v = db(peak);
    if v <= -99.0 { "−∞ dB".into() } else { format!("{:.0} dB", v).replace('-', "−") }
}

impl App {
    fn clock(&self) -> Clock {
        Clock { epoch: self.epoch, opened: self.opened_at, animate: self.ui_active() }
    }
    fn ring(&self, focused: bool) -> bool {
        focused && self.focus_visible
    }
    fn key_held(&self, vk: u32) -> bool {
        self.keys_down[(vk / 64 % 4) as usize] >> (vk % 64) & 1 != 0
    }
    fn page_main(&self) -> bool {
        !self.details && !self.rvc_page && !self.soundpad_page && !self.logs_page && !self.effects_page
    }

    /// The window: the app, with the update morph layer on top (empty unless morphing). The
    /// layer is always there so the app's widgets keep their state when a morph starts.
    pub fn view(&self, _: window::Id) -> Element<'_, Msg> {
        let (base, anim): (Element<'_, Msg>, _) = match &self.morph {
            None => (self.root(), None),
            Some(m) => (
                match m.base {
                    MorphBase::Root => self.root(),
                    MorphBase::Card(stage) => update_card_on_key(stage, &m.from_version, &m.to_version),
                    MorphBase::Key => container(Space::new()).width(Length::Fill).height(Length::Fill).style(|_| container::Style { background: Some(tacho::KEY.into()), ..Default::default() }).into(),
                },
                m.anim.as_ref(),
            ),
        };
        widget::stack![base, tacho::morph(anim)].width(Length::Fill).height(Length::Fill).into()
    }

    /// The whole app window without any morph.
    fn root(&self) -> Element<'_, Msg> {
        let voice = ((db(self.peak) + 72.0) / 72.0).clamp(0.0, 1.0);
        let logo = tacho::logo(26.0, voice);
        let titlebar = row![
            mouse_area(
                container(row![logo, bold("Mic Noize", 15, INK), Space::new().width(Length::Fill), tacho::signature()].spacing(10).align_y(iced::Center))
                    .padding([0, 16])
                    .width(Length::Fill)
                    .height(46)
                    .center_y(46),
            )
            .on_press(Msg::Drag),
            caption_button(glyph::MINIMIZE, Msg::Minimize, false),
            caption_button(glyph::CLOSE, Msg::Hide, true),
        ]
        .align_y(iced::Center);
        let titlebar = container(titlebar).width(Length::Fill).style(|_| container::Style {
            border: Border { color: LINE, width: 0.0, radius: 0.0.into() },
            ..Default::default()
        });

        // Under an opaque mosaic the page is not drawn at all; it returns as the mosaic fades.
        let page: Element<'_, Msg> = if self.page_shift.is_some() && !self.page_shift_revealed { Space::new().into() } else { self.body() };
        let body = widget::stack![
            page,
            tacho::page_shift(self.page_shift.as_ref(), Msg::PageShiftReveal, Msg::PageShiftDone),
        ]
        .width(Length::Fill)
        .height(Length::Fill);
        let root = column![
            titlebar,
            container(Space::new().height(1)).width(Length::Fill).style(|_| container::Style {
                background: Some(LINE.into()),
                ..Default::default()
            }),
            row![self.rail(), body].height(Length::Fill),
        ];
        container(root)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(BG.into()),
                text_color: Some(INK),
                border: Border { color: Color::from_rgb8(0x2A, 0x2B, 0x30), width: 1.0, radius: 12.0.into() },
                ..Default::default()
            })
            .into()
    }

    /// The current page with its error banner and scrolling, right of the rail.
    fn body(&self) -> Element<'_, Msg> {
        let content: Element<'_, Msg> = if self.logs_page {
            self.logs_view()
        } else if self.soundpad_page {
            self.soundpad_view()
        } else if self.details {
            self.settings_view()
        } else if self.rvc_page {
            self.rvc_view()
        } else if self.effects_page {
            self.effects_view()
        } else {
            self.main_view()
        };
        let mut page = column![].spacing(12).width(Length::Fill).height(Length::Fill);
        if !self.message.is_empty() {
            let (color, glyph) = match self.message.as_str() {
                DEVICE_REPAIRED => (GREEN, glyph::CHECK),
                DEVICE_REPAIRING => (DIM, glyph::REFRESH),
                _ => (RED, glyph::WARNING),
            };
            page = page.push(
                container(row![icon(glyph, 14, color), label(&self.message, 13, color)].spacing(10).align_y(iced::Center))
                    .padding([9, 14])
                    .width(Length::Fill)
                    .style(move |_| container::Style {
                        background: Some(Color { a: 0.08, ..color }.into()),
                        border: Border { color: Color { a: 0.45, ..color }, width: 1.0, radius: 8.0.into() },
                        ..Default::default()
                    }),
            );
        }
        // The soundpad owns its own scrollable list (and the "body" id) so its toolbar and
        // sidebar stay put while hundreds of clips scroll.
        if self.soundpad_page && self.sound_folder.is_some() {
            page.push(container(content).width(Length::Fill).height(Length::Fill)).padding([20, 26]).into()
        } else {
            page.push(
                scrollable(
                    mouse_area(container(content).padding(iced::Padding { right: 12.0, bottom: 8.0, ..Default::default() }))
                        .on_scroll(|d| Msg::Wheel("body", smooth::wheel_pixels(d))),
                )
                .id("body")
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .padding(iced::Padding { top: 20.0, right: 14.0, bottom: 6.0, left: 26.0 })
            .into()
        }
    }

    /// The page area painted offscreen, small, for the page-switch pixelation.
    pub fn page_mosaic(&self) -> Option<std::sync::Arc<tacho::Mosaic>> {
        mosaic_of(self.body(), tacho::page_area()?)
    }
    /// The whole window painted offscreen, small, for the update morphs.
    pub fn window_mosaic(&self, size: Size) -> Option<std::sync::Arc<tacho::Mosaic>> {
        mosaic_of(self.root(), size)
    }
    /// Left rail: what the app does, in order of use; settings and a ready update at the bottom.
    fn rail(&self) -> Element<'_, Msg> {
        let item = |glyph: &'static str, name: &'static str, page: u8, selected: bool| {
            let focused = self.focus == focus::tab(page);
            // The row fills the 40 px button and centres vertically; the glyph gets a fixed,
            // centred cell so icons of different widths line the labels up.
            button(focus_target(
                row![
                    container(icon(glyph, 15, if selected { ORANGE } else { DIM })).center(18),
                    label(name, 14, if selected { INK } else { DIM }),
                ]
                .spacing(11)
                .height(Length::Fill)
                .align_y(iced::Center),
                focused,
            )
            .height(Length::Fill))
            .width(Length::Fill)
            .height(40)
            .padding([0, 12])
            .on_press(Msg::Page(page))
            .style(move |_, status| button::Style {
                background: Some(
                    (if selected {
                        Color::from_rgb8(0x1F, 0x20, 0x23)
                    } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                        Color::from_rgb8(0x1A, 0x1B, 0x1E)
                    } else {
                        Color::TRANSPARENT
                    })
                    .into(),
                ),
                text_color: INK,
                border: Border {
                    color: if focused { ORANGE } else { Color::TRANSPARENT },
                    width: if focused { 2.0 } else { 0.0 },
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
        };
        let mut rail = column![
            item(glyph::MIC, "Шумодав", 0, self.page_main()),
            item(glyph::EFFECTS, "Эффекты", 6, self.effects_page),
            item(glyph::SOUNDPAD, "Саундпад", 4, self.soundpad_page),
            item(glyph::VOICE, "Смена голоса", 1, self.rvc_page),
            Space::new().height(Length::Fill),
        ]
        .spacing(2)
        .padding([14, 10])
        .width(196)
        .height(Length::Fill);
        if self.update_ready {
            rail = rail.push(
                container(
                    column![
                        bold("Обновление готово", 13, INK),
                        label(&self.update_status, 12, DIM),
                        action(
                            container(label("Перезапустить", 13, ORANGE_DARK)).center_x(Length::Fill),
                            Msg::ApplyUpdate,
                            self.focus == focus::UPDATE_BANNER,
                            true,
                        )
                        .width(Length::Fill),
                    ]
                    .spacing(8),
                )
                .padding(12)
                .style(|_| container::Style {
                    background: Some(CARD.into()),
                    border: Border { color: Color::from_rgb8(0x2A, 0x2B, 0x30), width: 1.0, radius: 10.0.into() },
                    ..Default::default()
                }),
            );
            rail = rail.push(Space::new().height(8));
        }
        rail = rail.push(item(glyph::SETTINGS, "Настройки", 2, self.details || self.logs_page));
        container(rail)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(RAIL.into()),
                border: Border { color: LINE, width: 0.0, radius: iced::border::Radius::default().bottom_left(12.0) },
                ..Default::default()
            })
            .into()
    }

    /// Шумодав: the devices you can change on top, the fixed route folded away, then strength.
    fn main_view(&self) -> Element<'_, Msg> {
        use focus::effects::*;
        let clock = self.clock();
        let mic = row![
            tile(glyph::MIC, 52.0, true),
            column![
                label("Микрофон", 12, DIM),
                frame(
                    repaint(self.input.as_ref().map(ToString::to_string), pick_list(self.inputs.as_slice(), self.input.as_ref(), Msg::Input)
                        .placeholder("Выберите микрофон")
                        .text_size(13)
                        .padding([6, 10])
                        .width(Length::Fill)
                        .style(device_style)),
                    self.focus == INPUT,
                ),
            ]
            .spacing(3)
            .width(Length::Fill),
        ]
        .spacing(12)
        .align_y(iced::Center);
        let hp_live = matches!(self.headphone_state, 1 | 2);
        let modded = hp_live || self.headphone_reverse || self.headphone_pitch != 0;
        let gear_focused = self.focus == HEADPHONE_GEAR;
        let open = self.headphone_page;
        let gear = button(focus_target(
            widget::stack![
                container(icon(glyph::SETTINGS, 16, if open { ORANGE_DARK } else { DIM })).center(40),
                container(
                    container(Space::new().width(7).height(7)).style(move |_| container::Style {
                        background: Some((if modded { if open { ORANGE_DARK } else { ORANGE } } else { Color::TRANSPARENT }).into()),
                        border: Border { radius: 4.0.into(), ..Border::default() },
                        ..Default::default()
                    }),
                )
                .padding(iced::Padding { top: 6.0, left: 27.0, ..Default::default() }),
            ],
            gear_focused,
        ))
        .width(40)
        .height(40)
        .padding(0)
        .on_press(Msg::HeadphonePanel(!open))
        .style(move |_, status| {
            let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some((if open { ORANGE } else if hover { HOVER } else { CARD }).into()),
                text_color: INK,
                border: Border {
                    color: if gear_focused || open { ORANGE } else { EDGE },
                    width: if gear_focused { 2.0 } else { 1.0 },
                    radius: 9.0.into(),
                },
                ..Default::default()
            }
        });
        let phones = row![
            tile(glyph::HEADPHONES, 52.0, true),
            column![
                label("Наушники", 12, DIM),
                frame(
                    repaint(self.headphone_output.as_ref().map(ToString::to_string), pick_list(self.headphone_outputs(), self.headphone_output.clone(), Msg::HeadphoneOutput)
                        .placeholder("Выберите наушники")
                        .text_size(13)
                        .padding([6, 10])
                        .width(Length::Fill)
                        .style(device_style)),
                    self.focus == focus::headphones::OUTPUT,
                ),
            ]
            .spacing(3)
            .width(Length::Fill),
            gear,
        ]
        .spacing(12)
        .align_y(iced::Center);
        let device_card = |content| {
            container(content).padding(10).width(Length::Fill).style(|_| container::Style {
                background: Some(CARD2.into()),
                border: Border { color: EDGE, width: 1.0, radius: 10.0.into() },
                ..Default::default()
            })
        };
        let processing = match self.denoiser.0 {
            3 => "DeepFilterNet (процессор)".to_owned(),
            4 => "Шум убран на входе".to_owned(),
            2 => "Без шумодава".to_owned(),
            _ => format!("NVIDIA Denoiser v{}", self.version),
        };
        let gpu = match &self.gpu {
            Ok((_, name)) => format!("{}, буфер {} мс", name.trim_start_matches("NVIDIA ").trim_start_matches("GeForce "), self.buffer),
            Err(_) => format!("буфер {} мс", self.buffer),
        };
        let output = self.output.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "Не выбран".into());
        let route_focused = self.focus == ROUTE;
        let route_toggle = button(focus_target(
            row![
                icon(if self.route_open { glyph::DOWN } else { glyph::RIGHT }, 10, DIM),
                label("Обработка и выход", 12, DIM),
                Space::new().width(Length::Fill),
                label(format!("{processing}  ›  {output}"), 12, FAINT),
            ]
            .spacing(8)
            .align_y(iced::Center),
            route_focused,
        ))
        .width(Length::Fill)
        .padding([8, 14])
        .on_press(Msg::RouteToggle)
        .style(move |_, status| button::Style {
            background: matches!(status, button::Status::Hovered | button::Status::Pressed).then(|| Color::from_rgb8(0x1F, 0x20, 0x23).into()),
            text_color: INK,
            border: Border {
                color: if route_focused { ORANGE } else { Color::TRANSPARENT },
                width: if route_focused { 2.0 } else { 0.0 },
                radius: iced::border::Radius::default().bottom(10.0),
            },
            ..Default::default()
        });
        let fixed = |glyph: &'static str, name: &'static str, main: String, detail: String| {
            row![
                tile(glyph, 38.0, false),
                column![
                    row![icon(glyph::LOCK, 9, FAINT), label(name, 11, FAINT)].spacing(5).align_y(iced::Center),
                    label(main, 13, Color::from_rgb8(0xCF, 0xCB, 0xC2)),
                    label(detail, 11, FAINT),
                ]
                .spacing(1),
            ]
            .spacing(10)
            .align_y(iced::Center)
            .width(Length::Fill)
        };
        let mut devices = column![
            container(row![device_card(mic), device_card(phones)].spacing(12)).padding(12),
            container(Space::new().height(1)).width(Length::Fill).style(|_| container::Style { background: Some(LINE.into()), ..Default::default() }),
            route_toggle,
        ];
        if self.route_open {
            devices = devices.push(
                container(
                    row![
                        fixed(glyph::CHIP, "Обработка", processing.clone(), gpu),
                        icon(glyph::RIGHT, 12, ORANGE),
                        fixed(glyph::OUTPUT, "Выход", output.clone(), "Выберите его микрофоном в Discord".into()),
                    ]
                    .spacing(14)
                    .align_y(iced::Center),
                )
                .padding(iced::Padding { top: 0.0, right: 14.0, bottom: 12.0, left: 14.0 }),
            );
        }
        let devices = container(devices).width(Length::Fill).style(|_| container::Style {
            background: Some(CARD.into()),
            border: Border { color: LINE, width: 1.0, radius: 10.0.into() },
            ..Default::default()
        });

        let before = self.in_peak;
        let after = self.peak;
        // From −72 dB: the raw microphone's hiss (about −60 dB) shows on «До», while what is
        // left after the denoiser (around −80 dB) reads as silence on «После».
        let level = |p: f32| ((db(p) + 72.0) / 72.0).clamp(0.0, 1.0);
        let meters = card(
            column![
                row![label("До", 12, DIM).width(56), tacho::level_meter(level(before), true, Color::from_rgb8(0x8A, 0x8B, 0x92)), container(numbers(db_text(before), 12, DIM)).align_right(64)]
                    .spacing(12)
                    .align_y(iced::Center),
                row![label("После", 12, INK).width(56), tacho::level_meter(level(after), false, if level(after) > 0.93 { ORANGE } else { GREEN }), container(numbers(db_text(after), 12, INK)).align_right(64)]
                    .spacing(12)
                    .align_y(iced::Center),
            ]
            .spacing(10),
        )
        .padding([12, 16]);

        let strength = self.controls.intensity * 100.0;
        let risky = strength > 100.0;
        let noise_card = container(
            column![
                row![label("Сила", 13, DIM), Space::new().width(Length::Fill)].height(26).align_y(iced::Center),
                frame(
                    tacho(0.0..=200.0, strength, Msg::Intensity, clock).default(100.0).red_above(100.0).segments(20),
                    self.ring(self.focus == INTENSITY),
                ),
            ]
            .push(risky.then(|| {
                row![
                    icon(glyph::WARNING, 12, Color::from_rgb8(0xFF, 0x77, 0x76)),
                    label("Выше 100% могут наблюдаться редкие звуковые неполадки в голосе", 12, Color::from_rgb8(0xFF, 0x77, 0x76)),
                ]
                .spacing(7)
                .align_y(iced::Center)
            }))
            .spacing(10),
        )
        .padding([14, 16])
        .width(Length::Fill);
        // In the red zone the card's background carries the police tape under its content.
        // The tape layer is always in the tree (drawn only in the red zone): adding it on the fly
        // would move the slider in the widget tree and drop a drag that crosses 100%.
        let layers = widget::stack![noise_card].width(Length::Fill).push_under(tacho::caution(clock, risky));
        let noise_card = card(layers).padding(0);
        let hold_card = card(
            column![
                row![
                    label("Пока держу", 13, DIM),
                    Space::new().width(Length::Fill),
                    self.bind_button(12, self.keys[12], self.focus == NOISE_BIND, false, 140.0),
                ]
                .height(26)
                .align_y(iced::Center),
                frame(
                    tacho(0.0..=200.0, self.controls.alternate_intensity * 100.0, Msg::AlternateIntensity, clock)
                        .default(15.0)
                        .red_above(100.0)
                        .segments(20)
                        .phase(20.0 * 64.0 + 128.0),
                    self.ring(self.focus == ALT_INTENSITY),
                ),
                label("Смена силы шумодава при удержании", 12, FAINT),
            ]
            .spacing(10),
        );
        let mut body = column![
            title("Шумодав"),
            devices,
            meters,
            row![noise_card, hold_card].spacing(14),
        ]
        .spacing(14);
        // Without NVIDIA the sliders do nothing; say so next to them, not in the error line.
        let note = if self.denoiser.0 == 2 && self.running() {
            Some((format!("Шумодав выключен: {}. Голос, эффекты и виртуальный микрофон работают.", self.denoiser.1), ORANGE))
        } else if self.denoiser.0 == 3 && self.running() {
            Some((format!("Шумодав DeepFilterNet на процессоре (+30 мс). NVIDIA: {}", self.denoiser.1), DIM))
        } else if self.denoiser.0 == 4 && self.running() {
            Some((format!("Шум уже убран на входе ({}): свой шумодав выключен, силу задаёт он.", self.denoiser.1), DIM))
        } else {
            None
        };
        if let Some((note, color)) = note {
            body = body.push(label(note, 12, color));
        }
        if !open {
            return body.into();
        }
        // The headphone settings open over the page, anchored under the gear.
        let panel = self.headphone_panel();
        widget::stack![
            body,
            container(widget::opaque(panel))
                .padding(iced::Padding { top: 138.0, right: 12.0, ..Default::default() })
                .align_right(Length::Fill),
        ]
        .into()
    }

    /// Что слышите вы: processing of the sound you hear, opened from the gear.
    fn headphone_panel(&self) -> Element<'_, Msg> {
        use focus::headphones::*;
        let clock = self.clock();
        let live = matches!(self.headphone_state, 1 | 2) || self.headphone_busy;
        let rowl = panel_row;
        let named = |name: &'static str| -> Element<'static, Msg> { label(name, 13, DIM).into() };
        let noise_name: Element<'static, Msg> = row![label("Шумодав", 13, DIM), Space::new().width(Length::Fill)].into();
        let mut content = column![
            row![
                icon(glyph::HEADPHONES, 15, ORANGE),
                bold("Что слышите вы", 13, INK),
                label("звук приложений", 12, FAINT),
                Space::new().width(Length::Fill),
                caption_button(glyph::CLOSE, Msg::HeadphonePanel(false), false).width(28).height(28),
            ]
            .spacing(8)
            .align_y(iced::Center),
            container(Space::new().height(1)).width(Length::Fill).style(|_| container::Style { background: Some(Color::from_rgb8(0x2E, 0x2F, 0x34).into()), ..Default::default() }),
            row![
                label(if live { "Обработка наушников включена" } else { "Обработка наушников выключена" }, 12, if live { GREEN } else { FAINT }),
                Space::new().width(Length::Fill),
                action(label(if live { "Остановить" } else { "Включить" }, 13, if live { INK } else { ORANGE_DARK }), Msg::HeadphoneToggle, self.focus == TOGGLE, !live),
            ]
            .align_y(iced::Center),
            rowl(
                row![noise_name, frame(switch(self.headphone_denoise, Msg::HeadphoneNoise, !self.headphone_busy), self.focus == NOISE)].align_y(iced::Center).into(),
                frame(
                    tacho(0.0..=200.0, self.headphone_intensity * 100.0, Msg::HeadphoneIntensity, clock)
                        .default(80.0)
                        .red_above(100.0)
                        .segments(16)
                        .compact()
                        .phase(3000.0)
                        .enabled(self.headphone_denoise),
                    self.ring(self.focus == INTENSITY),
                ),
            ),
            rowl(
                named("Громкость"),
                frame(
                    tacho(0.0..=100.0, self.headphone_volume * 100.0, Msg::HeadphoneVolume, clock).default(70.0).segments(16).compact().phase(4200.0),
                    self.ring(self.focus == VOLUME),
                ),
            ),
            rowl(
                named("Высота"),
                frame(
                    tacho(-12.0..=12.0, self.headphone_pitch as f32, Msg::HeadphonePitch, clock)
                        .default(0.0)
                        .origin(0.0)
                        .segments(16)
                        .compact()
                        .phase(5400.0)
                        .format(|v| format!("{:+.0} пт", v).replace('-', "−")),
                    self.ring(self.focus == PITCH),
                ),
            ),
            rowl(
                row![label("Реверс", 13, DIM), Space::new().width(Length::Fill), frame(switch(self.headphone_reverse, Msg::HeadphoneReverse, true), self.focus == REVERSE)].align_y(iced::Center).into(),
                label("куски по 0,2 с задом наперёд, +200 мс", 12, FAINT).into(),
            ),
        ]
        .spacing(10);
        if !self.headphone_message.is_empty() {
            content = content.push(label(&self.headphone_message, 12, RED));
        }
        content = content.push(label("Выход в микшере Windows: Mic Noize Headphones. После остановки верните физические наушники.", 11, FAINT));
        let panel = container(content)
            .padding([14, 16])
            .width(410)
            .style(|_| container::Style {
                background: Some(Color::from_rgb8(0x1F, 0x20, 0x23).into()),
                border: Border { color: EDGE, width: 1.0, radius: 12.0.into() },
                ..Default::default()
            });
        // A dark ring instead of a drop shadow: tiny-skia repaints shadows over partial redraws.
        container(panel)
            .padding(3)
            .style(|_| container::Style {
                background: Some(Color { a: 0.45, ..Color::BLACK }.into()),
                border: Border { radius: 15.0.into(), ..Border::default() },
                ..Default::default()
            })
            .into()
    }

    /// Эффекты: one aligned row per hold effect, then monitoring, Discord level and replays.
    fn effects_view(&self) -> Element<'_, Msg> {
        use focus::effects::*;
        let clock = self.clock();
        let grid = effect_grid;
        let head = grid(
            Space::new().into(),
            label("Эффект", 12, FAINT).into(),
            label("Сила", 12, FAINT).into(),
            row![icon(glyph::MIC, 11, FAINT), label("Мой голос", 12, FAINT)].spacing(6).align_y(iced::Center).into(),
            row![icon(glyph::OUTPUT, 11, FAINT), label("Голоса Discord", 12, FAINT)].spacing(6).align_y(iced::Center).into(),
        )
        .padding([0, 13]);
        let mut rows = column![head].spacing(6);
        for i in 0..5 {
            let (name, glyph, sub, control, bind_focus, active): (&str, &str, Element<'_, Msg>, Element<'_, Msg>, usize, bool) = match i {
                0 => (
                    "Усиление",
                    glyph::BOOST,
                    row![frame(switch(self.controls.overload, Msg::Overload, true), self.focus == OVERLOAD), label("перегрузка", 11, FAINT)]
                        .spacing(2)
                        .align_y(iced::Center)
                        .into(),
                    frame(
                        tacho(100.0..=2000.0, self.controls.boost * 100.0, Msg::Boost, clock)
                            .step(10.0)
                            .default(300.0)
                            .red_above(1600.0)
                            .segments(16)
                            .compact()
                            .phase(0.0),
                        self.ring(self.focus == BOOST),
                    ),
                    BOOST_BIND,
                    self.snapshot.boost_active != 0,
                ),
                1 => (
                    "Высота",
                    glyph::NOTE,
                    label("полутоны", 11, FAINT).into(),
                    frame(
                        tacho(-12.0..=12.0, self.controls.pitch as f32, Msg::Pitch, clock)
                            .default(-5.0)
                            .origin(0.0)
                            .segments(16)
                            .compact()
                            .phase(150.0)
                            .format(|v| format!("{:+.0}", v).replace('-', "−")),
                        self.ring(self.focus == PITCH),
                    ),
                    PITCH_BIND,
                    self.snapshot.pitch_active != 0,
                ),
                // Mirrored so that, as on every other slider, more orange means a stronger
                // effect: ×0.50 (slowest) sits on the right.
                2 => (
                    "Замедление",
                    glyph::SLOW,
                    label("запись, пока держите", 11, FAINT).into(),
                    frame(
                        tacho(-95.0..=-50.0, -self.controls.slow * 100.0, |v| Msg::Slow(-v), clock)
                            .step(5.0)
                            .default(-70.0)
                            .segments(16)
                            .compact()
                            .phase(300.0)
                            .format(|v| format!("×{:.2}", -v / 100.0)),
                        self.ring(self.focus == SLOW),
                    ),
                    SLOW_BIND,
                    (1..=8).contains(&self.phrase_state) && self.phrase_state % 2 == 1,
                ),
                3 => (
                    "Ускорение",
                    glyph::FAST,
                    label("запись, пока держите", 11, FAINT).into(),
                    frame(
                        tacho(105.0..=200.0, self.controls.fast * 100.0, Msg::Fast, clock)
                            .step(5.0)
                            .default(150.0)
                            .segments(16)
                            .compact()
                            .phase(450.0)
                            .format(|v| format!("×{:.2}", v / 100.0)),
                        self.ring(self.focus == FAST),
                    ),
                    FAST_BIND,
                    (1..=8).contains(&self.phrase_state) && self.phrase_state % 2 == 0,
                ),
                _ => (
                    "Реверс",
                    glyph::REVERSE,
                    label("после отпускания", 11, FAINT).into(),
                    self.reverse_demo(),
                    REVERSE_BIND,
                    self.phrase_state >= 9,
                ),
            };
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
            let line = grid(
                container(icon(glyph, 16, if active { ORANGE } else { Color::from_rgb8(0xCF, 0xCB, 0xC2) }))
                    .width(36)
                    .height(36)
                    .center(36)
                    .style(move |_| container::Style {
                        background: Some((if active { Color { a: 0.14, ..ORANGE } } else { Color::from_rgb8(0x25, 0x26, 0x2A) }).into()),
                        border: Border { radius: 8.0.into(), ..Border::default() },
                        ..Default::default()
                    })
                    .into(),
                column![bold(name, 14, INK), sub].spacing(2).into(),
                control,
                binding(false),
                binding(true),
            );
            rows = rows.push(
                container(line)
                    .padding([8, 12])
                    .width(Length::Fill)
                    .style(move |_| container::Style {
                        background: Some((if active { LIVE_BG } else { CARD }).into()),
                        border: Border { color: if active { Color { a: 0.45, ..ORANGE } } else { LINE }, width: 1.0, radius: 10.0.into() },
                        ..Default::default()
                    }),
            );
        }
        // Values mirror mic::PhraseEffect::State in src/effects.hpp: 1/2 record slow/fast,
        // 3/4 play slow/fast, 5/6 tail capture, 7/8 limit reached, 9 record reverse,
        // 10 reverse pause, 11 reverse limit, 12 play reverse.
        let status = match self.phrase_state {
            1 | 2 | 9 => format!("Запись {:.1} / 10 с · отпустите хоткей", self.phrase_seconds),
            10 => "Пауза 0,15 с перед реверсом…".into(),
            3 | 4 | 12 => format!("Воспроизведение · осталось {:.1} с", self.phrase_seconds),
            5 | 6 => "Завершаем последний слог…".into(),
            7 | 8 | 11 => "Записано 10 с · отпустите хоткей".into(),
            _ => String::new(),
        };
        if self.phrase_state != 0 {
            rows = rows.push(
                row![
                    label(status, 12, ORANGE).width(Length::Fill),
                    action(label("Отмена", 12, INK), Msg::CancelPhrase, self.focus == CANCEL_PHRASE, false),
                ]
                .spacing(8)
                .align_y(iced::Center),
            );
        }
        if self.discord_state == 3 {
            rows = rows.push(label(&self.discord_message, 12, RED));
        }
        let full_monitor = if self.monitor_all { self.monitor } else { 0 };
        let can_monitor = !self.busy && !self.quitting && matches!(self.snapshot.state, 2 | 3);
        let monitor_note = if self.monitor_all && matches!(self.monitor, 1 | 2) {
            "Сейчас слышен весь голос; режим эффектов сохранён."
        } else if self.effect_monitoring() && self.monitor == 1 {
            "Подключение наушников…"
        } else if self.effect_monitoring() && self.monitor == 3 {
            "Ошибка прослушивания — см. сообщение сверху."
        } else {
            ""
        };
        let mut hear = column![
            heading_row(glyph::HEADPHONES, "Слышать себя", Space::new().into()),
            row![
                action(
                    label(
                        match full_monitor {
                            1 => "Подключение…",
                            2 => "Слышу весь голос",
                            _ => "Весь голос",
                        },
                        13,
                        if full_monitor == 2 { ORANGE_DARK } else if can_monitor { INK } else { FAINT },
                    ),
                    Msg::Monitor,
                    self.focus == MONITOR,
                    full_monitor == 2,
                )
                .on_press_maybe(can_monitor.then_some(Msg::Monitor)),
                self.bind_button(10, self.keys[10], self.focus == MONITOR_BIND, false, 140.0),
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                frame(switch(self.effects_monitor, Msg::EffectsMonitor, true), self.focus == EFFECTS_MONITOR),
                label("эффекты", 12, DIM),
                Space::new().width(16),
                frame(switch(self.boost_monitor, Msg::BoostMonitor, true), self.focus == BOOST_MONITOR),
                label("усиление", 12, DIM),
            ]
            .spacing(4)
            .align_y(iced::Center),
        ]
        .spacing(8);
        if !monitor_note.is_empty() {
            hear = hear.push(label(monitor_note, 11, FAINT));
        }
        let discord = column![
            heading_row(glyph::VOLUME, "Громкость Discord", label("ваш голос не трогает", 11, FAINT).into()),
            frame(
                tacho(0.0..=DISCORD_VOLUME_MAX_PERCENT, discord_volume_percent(self.controls.discord_volume), Msg::DiscordVolume, clock)
                    .default(100.0)
                    .segments(18)
                    .compact()
                    .phase(600.0),
                self.ring(self.focus == DISCORD_VOLUME),
            ),
        ]
        .spacing(8);
        let replay = column![
            heading_row(glyph::REPEAT, "Повтор последнего", self.bind_button(11, self.keys[11], self.focus == REPLAY_BIND, false, 140.0)),
            self.clips_view(),
        ]
        .spacing(8)
        .width(Length::FillPortion(135));
        let bottom = card(
            row![column![hear, discord].spacing(14).width(Length::FillPortion(100)), replay].spacing(24),
        )
        .padding([13, 16]);
        column![title("Эффекты"), rows, bottom].spacing(14).into()
    }

    /// Reverse row: a practice word that turns around; click it to type your own.
    fn reverse_demo(&self) -> Element<'_, Msg> {
        if self.reverse_edit {
            return frame(
                repaint(&self.reverse_word, text_input("ваше слово", &self.reverse_word)
                    .id("reverse-word")
                    .size(14)
                    .padding([4, 8])
                    .width(180)
                    .on_input(Msg::ReverseWord)
                    .on_submit(Msg::ReverseEdit(false))
                    .style(input_style)),
                self.focus == focus::effects::REVERSE_WORD,
            );
        }
        frame(
            button(tacho::reverse_word(&self.reverse_word, self.clock()))
                .padding([2, 4])
                .on_press(Msg::ReverseEdit(true))
                .style(|_, status| button::Style {
                    background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                        .then(|| Color::from_rgb8(0x22, 0x23, 0x27).into()),
                    text_color: INK,
                    border: Border { radius: 6.0.into(), ..Border::default() },
                    ..Default::default()
                }),
            self.focus == focus::effects::REVERSE_WORD,
        )
    }

    /// The newest recording large, the other five as chips; each can be played and saved.
    fn clips_view(&self) -> Element<'_, Msg> {
        use focus::effects::*;
        if self.clips.is_empty() {
            return label("Появятся после эффектов удержания: высоты, замедления, ускорения или реверса.", 12, FAINT).into();
        }
        let (playing, position, length) = self.sound_playing;
        let progress = |i: usize| -> Option<f32> {
            (playing == Self::clip_id(i)).then(|| if length > 0.0 { (position / length).clamp(0.0, 1.0) } else { 0.0 })
        };
        let save_button = |i: usize| {
            let focused = self.focus == CLIP_BASE + 2 * i + 1;
            let chosen = self.clip_menu == Some(i);
            button(focus_target(container(icon(glyph::SAVE, 12, if chosen { ORANGE_DARK } else { DIM })).center(Length::Fill), focused))
                .width(30)
                .height(Length::Fill)
                .padding(0)
                .on_press(Msg::ClipMenu(Some(i)))
                .style(move |_, status| {
                    let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
                    button::Style {
                        background: Some((if chosen || hover { ORANGE } else { Color::TRANSPARENT }).into()),
                        text_color: INK,
                        border: Border { color: if focused { ORANGE } else { Color::TRANSPARENT }, width: 2.0, radius: 6.0.into() },
                        ..Default::default()
                    }
                })
        };
        let hero = {
            let clip = &self.clips[0];
            let p = progress(0);
            let failed = matches!(clip.state, SoundState::Failed(_));
            let focused = self.focus == CLIP_BASE;
            let play = button(focus_target(
                container(icon(if p.is_some() { glyph::STOP } else { glyph::PLAY }, 14, ORANGE_DARK)).center(Length::Fill),
                focused,
            ))
            .width(40)
            .height(40)
            .padding(0)
            .on_press(Msg::ClipPlay(0))
            .style(move |_, status| button::Style {
                background: Some((if matches!(status, button::Status::Hovered | button::Status::Pressed) { Color::from_rgb8(0xFF, 0xB0, 0x70) } else { ORANGE }).into()),
                text_color: ORANGE_DARK,
                border: Border { color: if focused { INK } else { Color::TRANSPARENT }, width: 2.0, radius: 8.0.into() },
                ..Default::default()
            });
            let seconds = match clip.state {
                SoundState::Loaded(s) => format!("{s:.1} с").replace('.', ","),
                _ => String::new(),
            };
            let bar = waveform(&clip.name, p);
            container(
                row![
                    play,
                    column![
                        numbers(super::clip_label(&clip.name), 14, if failed { RED } else { INK }),
                        label(if seconds.is_empty() { "последняя запись".to_owned() } else { format!("последняя · {seconds}") }, 11, FAINT),
                    ]
                    .spacing(2)
                    .width(118),
                    container(bar).width(Length::Fill).center_y(40),
                    save_button(0),
                ]
                .spacing(12)
                .height(40)
                .align_y(iced::Center),
            )
            .padding([8, 10])
            .style(move |_| container::Style {
                background: Some((if p.is_some() { LIVE_BG } else { CARD2 }).into()),
                border: Border { color: if p.is_some() { Color { a: 0.6, ..ORANGE } } else { Color::from_rgb8(0x2E, 0x2F, 0x34) }, width: 1.0, radius: 10.0.into() },
                ..Default::default()
            })
        };
        let chip = |i: usize| -> Element<'_, Msg> {
            let clip = &self.clips[i];
            let p = progress(i);
            let focused = self.focus == CLIP_BASE + 2 * i;
            let failed = matches!(clip.state, SoundState::Failed(_));
            let play = button(focus_target(
                row![
                    icon(if p.is_some() { glyph::STOP } else { glyph::PLAY }, 9, if p.is_some() { ORANGE } else { DIM }),
                    numbers(super::clip_label(&clip.name), 13, if failed { RED } else if p.is_some() { ORANGE } else { INK }),
                ]
                .spacing(8)
                .align_y(iced::Center),
                focused,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([0, 10])
            .on_press(Msg::ClipPlay(i))
            .style(move |_, status| button::Style {
                background: matches!(status, button::Status::Hovered | button::Status::Pressed).then(|| HOVER.into()),
                text_color: INK,
                border: Border { color: if focused { ORANGE } else { Color::TRANSPARENT }, width: 2.0, radius: 6.0.into() },
                ..Default::default()
            });
            container(row![play, save_button(i)].height(34))
                .style(move |_| container::Style {
                    background: Some((if p.is_some() { LIVE_BG } else { CARD2 }).into()),
                    border: Border { color: if p.is_some() { Color { a: 0.6, ..ORANGE } } else { Color::from_rgb8(0x2E, 0x2F, 0x34) }, width: 1.0, radius: 7.0.into() },
                    ..Default::default()
                })
                .width(Length::Fill)
                .into()
        };
        let mut list = column![hero].spacing(6);
        match self.clip_menu.filter(|i| *i < self.clips.len()) {
            Some(i) => {
                list = list.push(
                    row![
                        label(format!("Сохранить «{}»", super::clip_label(&self.clips[i].name)), 12, DIM),
                        action(label("В саундпад", 12, INK), Msg::ClipSave(i, true), self.focus == CLIP_TO_SOUNDPAD, false),
                        action(label("В папку…", 12, INK), Msg::ClipSave(i, false), self.focus == CLIP_TO_FOLDER, false),
                        caption_button(glyph::CLOSE, Msg::ClipMenu(None), false).width(30).height(30),
                    ]
                    .spacing(6)
                    .align_y(iced::Center),
                );
            }
            None if !self.clip_note.is_empty() => {
                let saved = self.clip_note.starts_with("Сохранено");
                let hint = self.clip_note.starts_with("Выберите");
                list = list.push(
                    row![
                        icon(if saved { glyph::CHECK } else { glyph::WARNING }, 12, if saved { GREEN } else if hint { DIM } else { RED }),
                        label(&self.clip_note, 12, if saved { GREEN } else if hint { DIM } else { RED }),
                    ]
                    .spacing(8)
                    .align_y(iced::Center),
                );
            }
            None => {}
        }
        let others: Vec<usize> = (1..self.clips.len()).collect();
        for line in others.chunks(3) {
            let mut cells = row![].spacing(6);
            for &i in line {
                cells = cells.push(chip(i));
            }
            for _ in line.len()..3 {
                cells = cells.push(Space::new().width(Length::Fill));
            }
            list = list.push(cells);
        }
        list.into()
    }

    fn rvc_view(&self) -> Element<'_, Msg> {
        use focus::rvc::*;
        let clock = self.clock();
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
        let mut top = column![
            row![
                tile(glyph::VOICE, 44.0, self.controls.rvc),
                column![
                    bold("Голос по модели RVC", 15, if self.snapshot.rvc_state == 2 { GREEN } else { INK }),
                    label(rvc_status, 12, if self.snapshot.rvc_state == 3 { RED } else { DIM }),
                ]
                .spacing(2)
                .width(Length::Fill),
                frame(switch(self.controls.rvc, Msg::Rvc, self.rvc_runtime_installed), self.focus == ENABLE),
            ]
            .spacing(12)
            .align_y(iced::Center),
        ]
        .spacing(12);
        if !self.rvc_runtime_installed {
            // Without the runtime this is the only thing on the page that does anything, so it
            // is the page's one orange button.
            top = top.push(
                action(
                    label(if self.rvc_runtime_installing { "Установка RVC…" } else { "Установить RVC runtime" }, 13, ORANGE_DARK),
                    Msg::RvcInstall,
                    self.focus == INSTALL,
                    true,
                )
                .on_press_maybe((!self.rvc_runtime_installing).then_some(Msg::RvcInstall)),
            );
        }
        let model = card(
            column![
                row![
                    container(frame(
                        repaint(self.controls.rvc_options.slot, pick_list(
                            self.rvc_models.as_slice(),
                            self.rvc_models.iter().find(|m| m.slot == self.controls.rvc_options.slot).cloned(),
                            Msg::RvcModel,
                        )
                        .placeholder("Выберите модель")
                        .text_size(13)
                        .padding([6, 10])
                        .width(Length::Fill)
                        .style(device_style)),
                        self.focus == MODEL,
                    ))
                    .width(Length::Fill),
                    action(
                        label(if self.rvc_importing { "Импорт…" } else { "Импорт модели" }, 13, if self.rvc_importing || !self.rvc_runtime_installed { FAINT } else { INK }),
                        Msg::RvcImport,
                        self.focus == IMPORT,
                        false,
                    )
                    .on_press_maybe((self.rvc_runtime_installed && !self.rvc_importing).then_some(Msg::RvcImport)),
                ]
                .spacing(10)
                .align_y(iced::Center),
                row![
                    container(frame(
                        repaint(&self.rvc_name, text_input("Название модели", &self.rvc_name)
                            .id("rvc-name")
                            .size(13)
                            .padding([6, 10])
                            .on_input_maybe(self.rvc_can_manage().then_some(Msg::RvcName))
                            .on_submit_maybe(self.rvc_can_manage().then_some(Msg::RvcRename))
                            .style(input_style)),
                        self.focus == NAME,
                    ))
                    .width(Length::Fill),
                    action(label("Переименовать", 12, if self.rvc_can_manage() { INK } else { FAINT }), Msg::RvcRename, self.focus == RENAME, false)
                        .on_press_maybe(self.rvc_can_manage().then_some(Msg::RvcRename)),
                    action(
                        label(if self.rvc_delete_confirm { "Удалить ещё раз" } else { "Удалить" }, 12, if self.rvc_can_manage() { RED } else { FAINT }),
                        Msg::RvcDelete,
                        self.focus == DELETE,
                        false,
                    )
                    .on_press_maybe(self.rvc_can_manage().then_some(Msg::RvcDelete)),
                ]
                .spacing(10)
                .align_y(iced::Center),
            ]
            .spacing(10),
        );
        let pitch = card(
            column![
                heading_row(glyph::NOTE, "Тон модели", label("±24 полутона", 11, FAINT).into()),
                frame(
                    tacho(-24.0..=24.0, self.controls.rvc_options.pitch as f32, Msg::RvcPitch, clock)
                        .default(0.0)
                        .origin(0.0)
                        .segments(16)
                        .format(|v| format!("{:+.0} пт", v).replace('-', "−")),
                    self.ring(self.focus == PITCH),
                ),
                label("Сдвигает тон готового голоса модели. Ctrl+клик — 0.", 11, FAINT),
            ]
            .spacing(6),
        )
        .height(Length::Fill);
        // The advanced settings open beside the pitch card, not below it, so the page never
        // needs scrolling at the default window size.
        let mut tuning = column![row![
            heading_row(glyph::CHIP, "Тонкая настройка", Space::new().into()),
            action(label(if self.rvc_advanced { "Скрыть" } else { "Показать" }, 12, INK), Msg::RvcAdvanced, self.focus == ADVANCED, false),
        ]
        .spacing(10)
        .align_y(iced::Center)]
        .spacing(8);
        if self.rvc_advanced {
            let index: Element<'_, Msg> = if self.rvc_has_index() {
                frame(tacho(0.0..=100.0, self.controls.rvc_options.index as f32, Msg::RvcIndex, clock).default(0.0).segments(16).compact().phase(900.0), self.ring(self.focus == INDEX))
            } else {
                label("Недоступно: у модели нет .index", 12, FAINT).into()
            };
            tuning = tuning
                .push(label("Влияние индекса", 12, DIM))
                .push(index)
                .push(label("Вход модели", 12, DIM))
                .push(frame(tacho(50.0..=300.0, self.controls.rvc_options.gain as f32, Msg::RvcGain, clock).step(5.0).default(100.0).segments(16).compact().phase(1800.0), self.ring(self.focus == GAIN)))
                .push(
                    row![
                        label("Блок аудио, мс", 12, DIM),
                        frame(repaint(self.controls.rvc_options.chunk, pick_list(rvc::CHUNKS, Some(self.controls.rvc_options.chunk), Msg::RvcChunk).text_size(13).style(device_style)), self.focus == CHUNK),
                        Space::new().width(Length::Fill),
                        action(label("Обновить модели", 12, INK), Msg::RvcRefresh, self.focus == REFRESH, false),
                    ]
                    .spacing(8)
                    .align_y(iced::Center),
                )
                .push(label("Задержка = блок + 200 мс. При лаге модели — тишина, не обычный голос.", 11, FAINT));
        } else {
            tuning = tuning.push(label(
                if self.rvc_has_index() { "Индекс модели, громкость входа и размер блока аудио." } else { "Громкость входа и размер блока аудио." },
                11,
                FAINT,
            ));
        }
        let mut content = column![
            title("Смена голоса"),
            top,
            model,
            row![pitch.width(Length::FillPortion(1)), card(tuning).width(Length::FillPortion(1)).height(Length::Fill)]
                .spacing(14)
                // Both cards share a height; Fill children in a Shrink row would collapse.
                .height(if self.rvc_advanced { 276 } else { 156 }),
        ]
        .spacing(14);
        if !self.rvc_import_note.is_empty() {
            content = content.push(label(&self.rvc_import_note, 12, DIM));
        }
        content = content.push(label("Только микрофон. Выключение выгружает модель; повторный запуск снова её загружает.", 11, FAINT));
        content.into()
    }

    /// One report to copy and send: the buttons first, the text exactly as it will be copied.
    fn logs_view(&self) -> Element<'_, Msg> {
        use focus::logs::*;
        let report: Element<'_, Msg> = if self.logs_text.is_empty() {
            label("Собираем отчёт…", 12, DIM).into()
        } else {
            // Consolas: the generic monospace fallback has no Cyrillic and clips underscores.
            text(&self.logs_text).size(12).color(INK).font(Font::with_name("Consolas")).line_height(1.35).into()
        };
        column![
            row![
                action(row![icon(glyph::BACK, 11, INK), label("Настройки", 13, INK)].spacing(8).align_y(iced::Center), Msg::Page(2), self.focus == BACK, false),
                title("Логи"),
            ]
            .spacing(14)
            .align_y(iced::Center),
            row![
                action(label(if self.logs_copied { "Скопировано" } else { "Копировать всё" }, 13, ORANGE_DARK), Msg::LogsCopy, self.focus == COPY, true),
                action(label("Открыть папку", 13, INK), Msg::LogsFolder, self.focus == FOLDER, false),
                Space::new().width(Length::Fill),
                action(label(if self.report_sending { "Отправляем…" } else { "Отправить разработчику" }, 13, INK), Msg::SendReport, self.focus == SEND, false)
                    .on_press_maybe((!self.report_sending).then_some(Msg::SendReport)),
            ]
            .spacing(8)
            .align_y(iced::Center),
            label("Версия, видеокарта, состояние и последние строки каждого лога. Имя пользователя Windows заменено.", 12, DIM),
            card(report),
        ]
        .spacing(12)
        .into()
    }

    fn soundpad_view(&self) -> Element<'_, Msg> {
        use focus::soundpad::*;
        let clock = self.clock();
        let (playing_id, position, length) = self.sound_playing;
        let playing = playing_id != 0;
        let visible = self.visible_sounds();
        let mut toolbar = row![title("Саундпад")].spacing(10).align_y(iced::Center);
        if self.sound_folder.is_some() {
            toolbar = toolbar.push(numbers(
                if self.sound_filter.trim().is_empty() && self.section == super::Selection::All {
                    format!("{}", self.sounds.len())
                } else {
                    format!("{} из {}", visible.len(), self.sounds.len())
                },
                13,
                FAINT,
            ));
        }
        toolbar = toolbar.push(Space::new().width(Length::Fill));
        if self.sound_folder.is_some() {
            toolbar = toolbar.push(
                container(frame(
                    repaint(&self.sound_filter, text_input("Поиск", &self.sound_filter)
                        .id("sound-filter")
                        .size(13)
                        .padding([6, 10])
                        .on_input(Msg::SoundpadFilter)
                        .style(input_style)),
                    self.focus == FILTER,
                ))
                .width(220),
            );
            toolbar = toolbar.push(frame(
                repaint(self.sound_sort.to_string(), pick_list(SoundSort::ALL, Some(self.sound_sort), Msg::SoundpadSort)
                    .text_size(13)
                    .padding([6, 10])
                    .width(150)
                    .style(device_style)),
                self.focus == SORT,
            ));
        }
        toolbar = toolbar
            .push(
                action(label("Добавить звуки", 13, ORANGE_DARK), Msg::SoundpadAdd, self.focus == ADD, true)
                    .on_press_maybe((!self.sound_dialog && self.sound_folder.is_some()).then_some(Msg::SoundpadAdd)),
            )
            .push(icon_button(glyph::FOLDER, Msg::SoundpadFolder, self.focus == FOLDER).on_press_maybe((!self.sound_dialog).then_some(Msg::SoundpadFolder)))
            .push(icon_button(glyph::REFRESH, Msg::SoundpadRefresh, self.focus == REFRESH).on_press_maybe(self.sound_folder.is_some().then_some(Msg::SoundpadRefresh)));
        let folder_name = self.sound_folder.as_ref().map(|f| f.to_string_lossy().into_owned());
        let mut body = column![toolbar].spacing(12);
        if let Some(name) = folder_name {
            body = body.push(row![icon(glyph::FOLDER, 11, FAINT), label(name, 12, FAINT)].spacing(8).align_y(iced::Center));
        }
        if !self.sound_note.is_empty() {
            body = body.push(label(&self.sound_note, 12, if self.sound_note.starts_with("Добавлено") || self.sound_note.starts_with('«') { GREEN } else { DIM }));
        }
        if self.sound_folder.is_none() {
            return body
                .push(card(
                    column![
                        bold("Звуки поверх голоса в виртуальный микрофон", 15, INK),
                        label("Выберите папку с mp3, wav, ogg или m4a. Каждому звуку в списке назначается свой хоткей; звуки без хоткея запускаются кнопкой в строке.", 12, DIM),
                        label("Хоткей звука: нажатие играет, повторное нажатие останавливает, быстрое двойное перезапускает с начала.", 12, DIM),
                        action(label("Выбрать папку", 13, ORANGE_DARK), Msg::SoundpadFolder, false, true).on_press_maybe((!self.sound_dialog).then_some(Msg::SoundpadFolder)),
                    ]
                    .spacing(10),
                ))
                .push(self.soundpad_footer(clock, playing, playing_id, position, length))
                .into();
        }
        let custom = self.custom_section();
        body = body.push(
            row![self.section_sidebar(custom), self.sound_list(&visible, custom, playing_id, position, length, clock)]
                .spacing(16)
                .height(Length::Fill),
        );
        body.push(self.soundpad_footer(clock, playing, playing_id, position, length)).height(Length::Fill).into()
    }
    /// Now playing on top, the soundpad-wide controls below.
    fn soundpad_footer(&self, clock: Clock, playing: bool, playing_id: u32, position: f32, length: f32) -> Element<'_, Msg> {
        use focus::soundpad::*;
        let name = (playing_id != 0)
            .then(|| self.sounds.iter().enumerate().find(|(i, _)| super::App::sound_id(*i) == playing_id))
            .flatten()
            .map(|(_, s)| s.name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&s.name).to_owned())
            .unwrap_or_else(|| if playing { "Запись".into() } else { "Ничего не играет".into() });
        let now = row![
            button(container(icon(glyph::STOP, 12, if playing { ORANGE_DARK } else { FAINT })).center(Length::Fill))
                .width(32)
                .height(32)
                .padding(0)
                .on_press_maybe(playing.then_some(Msg::SoundpadStop))
                .style(move |_, _| button::Style {
                    background: Some((if playing { ORANGE } else { CARD2 }).into()),
                    border: Border { radius: 7.0.into(), ..Border::default() },
                    ..Default::default()
                }),
            column![
                row![
                    label(name, 13, if playing { INK } else { FAINT }).width(Length::Fill),
                    numbers(if playing { format!("{} / {}", clock_text(position), clock_text(length)) } else { String::new() }, 12, DIM),
                ]
                .spacing(12),
                meter(if playing && length > 0.0 { position / length } else { 0.0 }, ORANGE),
            ]
            .spacing(6)
            .width(Length::Fill),
        ]
        .spacing(12)
        .align_y(iced::Center);
        let controls = row![
            label("Стоп всё", 12, DIM),
            self.bind_button(super::SOUND_STOP_BIND, self.sound_stop_key, self.focus == STOP_BIND, false, 130.0),
            Space::new().width(Length::Fill),
            icon(glyph::VOLUME, 13, DIM),
            container(frame(
                tacho(0.0..=200.0, self.sound_volume * 100.0, Msg::SoundpadVolume, clock).default(100.0).segments(18).compact().phase(0.0),
                self.ring(self.focus == VOLUME),
            ))
            .width(250),
            frame(switch(self.sound_normalize, Msg::SoundpadNormalize, true), self.focus == NORMALIZE),
            label("выравнивать", 12, DIM),
            frame(switch(self.sound_monitor, Msg::SoundpadHear, true), self.focus == HEAR),
            label("в наушниках", 12, DIM),
        ]
        .spacing(8)
        .align_y(iced::Center);
        let mut footer = column![now, container(Space::new().height(1)).width(Length::Fill).style(|_| container::Style { background: Some(LINE.into()), ..Default::default() }), controls].spacing(10);
        let hint = self.monitor_hint();
        if !hint.is_empty() {
            footer = footer.push(label(hint, 11, if self.sound_monitor && self.monitor == 3 { RED } else { FAINT }));
        }
        card(footer).padding([12, 14]).into()
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
                    label(&item.label, 13, if selected { INK } else { DIM }).width(Length::Fill),
                    numbers(item.count.to_string(), 12, if selected { DIM } else { FAINT }),
                ]
                .spacing(6)
                .align_y(iced::Center),
                focused,
            ))
            .width(Length::Fill)
            .padding([6, 10])
            .on_press(Msg::SectionSelect(i))
            .style(move |_, status| button::Style {
                background: Some(
                    (if selected {
                        Color::from_rgb8(0x1F, 0x20, 0x23)
                    } else if hot || matches!(status, button::Status::Hovered | button::Status::Pressed) {
                        Color::from_rgb8(0x1B, 0x1C, 0x1F)
                    } else {
                        Color::TRANSPARENT
                    })
                    .into(),
                ),
                text_color: INK,
                border: Border {
                    color: if hot || focused { ORANGE } else { Color::TRANSPARENT },
                    width: if hot || focused { 2.0 } else { 0.0 },
                    radius: 7.0.into(),
                },
                ..Default::default()
            });
            // Drop targets report the cursor; the global mouse release finishes the drop. Every
            // entry is wrapped so the tree (and the sidebar scroll offset) stays stable.
            list = list.push(mouse_area(entry).on_enter(Msg::DragOver(target)).on_exit(Msg::DragOver(None)));
        }
        let mut sidebar = column![
            label(if self.dragging.is_some() { "Отпустите на разделе" } else { "Разделы" }, 12, if self.dragging.is_some() { ORANGE } else { FAINT }),
            scrollable(
                mouse_area(container(list).padding(iced::Padding { right: 10.0, ..Default::default() }))
                    .on_scroll(|d| Msg::Wheel("sections", smooth::wheel_pixels(d))),
            )
            .id("sections")
            .height(Length::Fill)
            .width(Length::Fill),
            action(label("+ Новый раздел", 12, DIM), Msg::SectionAdd, self.focus == SECTION_ADD, false).width(Length::Fill),
        ]
        .spacing(6)
        .width(180)
        .height(Length::Fill);
        if custom.is_some() {
            sidebar = sidebar.push(
                column![
                    frame(
                        repaint(&self.section_name, text_input(
                            custom.and_then(|i| self.sections.get(i)).map(|s| s.name.as_str()).unwrap_or("Название раздела"),
                            &self.section_name,
                        )
                        .id("section-name")
                        .size(12)
                        .padding([5, 8])
                        .on_input(Msg::SectionName)
                        .on_submit(Msg::SectionRename)
                        .style(input_style)),
                        self.focus == SECTION_NAME,
                    ),
                    action(label("Удалить раздел", 12, RED), Msg::SectionDelete, self.focus == SECTION_DELETE, false).width(Length::Fill),
                    label("Перетащите звук за ≡ из списка; × в строке убирает его из раздела.", 11, FAINT),
                ]
                .spacing(6),
            );
        }
        sidebar.into()
    }
    /// Header plus the virtualised clip list in its own scrollable (id "body" so keyboard
    /// focus reveal keeps working here).
    fn sound_list(&self, visible: &[usize], custom: Option<usize>, playing_id: u32, position: f32, length: f32, clock: Clock) -> Element<'_, Msg> {
        use focus::soundpad::*;
        let header = row![
            Space::new().width(46),
            label("Звук", 12, FAINT).width(Length::Fill),
            label("Громкость", 12, FAINT).width(170),
            label("Клавиша", 12, FAINT).width(if custom.is_some() { 164 } else { 130 }),
        ]
        .spacing(10);
        if self.sounds.is_empty() {
            return column![header, label("В папке пока нет mp3, wav, ogg или m4a.", 13, DIM)].spacing(8).into();
        }
        if visible.is_empty() {
            return column![
                header,
                label(if custom.is_some() { "Раздел пуст: перетащите сюда звуки из «Все звуки» за ≡." } else { "Ничего не найдено." }, 13, DIM)
            ]
            .spacing(8)
            .into();
        }
        // Build only the viewport and one keyboard target. Fixed row pitch lets spacers
        // preserve all skipped distances, including the gap to an off-screen focus target.
        // Keys keep row state (shaped text) attached to the same clip as the window slides.
        const PITCH: f32 = ROW_HEIGHT + ROW_SPACING;
        let (scroll, viewport) = self.sound_scroll;
        let focused = self.focus.checked_sub(ROW_BASE).and_then(|f| visible.iter().position(|&i| i == f / 3));
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
                _ if lit => format!("{} / {}", clock_text(position), clock_text(length)),
                SoundState::Loaded(seconds) => clock_text(*seconds),
                SoundState::Loading => "загрузка…".into(),
                SoundState::Failed(e) => e.clone(),
                SoundState::Unloaded => String::new(),
            };
            let failed = matches!(sound.state, SoundState::Failed(_));
            let play_focused = self.focus == ROW_BASE + 3 * i;
            let volume_focused = self.focus == ROW_BASE + 3 * i + 1;
            let volume_active = self.sound_hover == Some(i) || volume_focused || sound.volume != 100;
            let grip = mouse_area(container(label("≡", 14, if dragged { ORANGE } else { FAINT })).width(14).height(ROW_HEIGHT).center_y(ROW_HEIGHT))
                .on_press(Msg::DragStart(i))
                .interaction(iced::mouse::Interaction::Grab);
            let play = button(focus_target(
                row![
                    container(icon(if lit { glyph::STOP } else { glyph::PLAY }, 10, if lit { ORANGE_DARK } else { DIM }))
                        .width(26)
                        .height(26)
                        .center(26)
                        .style(move |_| container::Style {
                            background: Some((if lit { ORANGE } else { Color::from_rgb8(0x1D, 0x1E, 0x21) }).into()),
                            border: Border { color: if lit { ORANGE } else { Color::from_rgb8(0x34, 0x35, 0x3A) }, width: 1.0, radius: 13.0.into() },
                            ..Default::default()
                        }),
                    label(stem, 13, if lit { Color::from_rgb8(0xFF, 0xB2, 0x7A) } else { INK }).width(Length::Fill),
                    numbers(status, 12, if failed { RED } else if lit { ORANGE } else { FAINT }),
                ]
                .spacing(10)
                .align_y(iced::Center),
                play_focused,
            ))
            .width(Length::Fill)
            .height(ROW_HEIGHT)
            .padding([0, 6])
            .on_press(Msg::SoundPlay(i))
            .style(move |_, status| button::Style {
                background: Some(
                    (if lit {
                        Color { a: 0.08, ..ORANGE }
                    } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                        CARD
                    } else {
                        Color::TRANSPARENT
                    })
                    .into(),
                ),
                text_color: INK,
                border: Border { color: if play_focused { ORANGE } else { Color::TRANSPARENT }, width: 2.0, radius: 8.0.into() },
                ..Default::default()
            });
            let volume: Element<'_, Msg> = if volume_active {
                frame(
                    tacho(0.0..=200.0, sound.volume as f32, move |v| Msg::SoundVolume(i, v), Clock { animate: false, ..clock })
                        .step(5.0)
                        .default(100.0)
                        .segments(12)
                        .compact(),
                    self.ring(volume_focused),
                )
            } else {
                Space::new().into()
            };
            let bind = self.bind_button(super::SOUND_BIND_BASE + i, sound.key, self.focus == ROW_BASE + 3 * i + 2, false, 130.0);
            let mut line = row![grip, play, container(volume).width(170), bind].spacing(10).height(ROW_HEIGHT).align_y(iced::Center);
            if custom.is_some() {
                line = line.push(
                    button(container(icon(glyph::CLOSE, 9, RED)).center(Length::Fill))
                        .width(24)
                        .height(24)
                        .padding(0)
                        .on_press(Msg::SoundUnassign(i))
                        .style(|_, status| button::Style {
                            background: matches!(status, button::Status::Hovered | button::Status::Pressed).then(|| HOVER.into()),
                            text_color: RED,
                            border: Border { radius: 6.0.into(), ..Default::default() },
                            ..Default::default()
                        }),
                );
            }
            // A row-sized clip layer lets tiny-skia invalidate the moving row as a whole.
            // Without it, scattered text/slider damage fragments repaint the same list repeatedly.
            rows = rows.push(
                i,
                widget::stack![
                    Space::new().width(Length::Fill).height(ROW_HEIGHT),
                    mouse_area(line).on_enter(Msg::SoundHover(i, true)).on_exit(Msg::SoundHover(i, false)),
                ]
                .clip(true),
            );
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
        use focus::settings::*;
        let locked = self.running() || self.busy;
        let route: Element<'_, Msg> = if locked {
            column![
                row![label("Микрофон", 12, FAINT).width(150), label(self.input.as_ref().map(|d| d.name.clone()).unwrap_or_default(), 13, INK)],
                row![label("Передать голос в", 12, FAINT).width(150), label(self.output.as_ref().map(|d| d.name.clone()).unwrap_or_default(), 13, INK)],
                row![label("Модель", 12, FAINT).width(150), label(format!("Denoiser v{}  ·  буфер {} мс", self.version, self.buffer), 13, INK)],
                label("Выход и модель меняются, пока обработка остановлена.", 12, FAINT),
            ]
            .spacing(8)
            .into()
        } else {
            column![
                row![
                    label("Микрофон", 12, FAINT).width(150),
                    frame(repaint(self.input.as_ref().map(ToString::to_string), pick_list(self.inputs.as_slice(), self.input.as_ref(), Msg::Input).placeholder("Микрофон отключён / не выбран").width(Length::Fill).text_size(13).style(device_style)), self.focus == INPUT),
                ]
                .align_y(iced::Center),
                row![
                    label("Передать голос в", 12, FAINT).width(150),
                    frame(repaint(self.output.as_ref().map(ToString::to_string), pick_list(self.outputs.as_slice(), self.output.as_ref(), Msg::Output).placeholder("Выберите выход").width(Length::Fill).text_size(13).style(device_style)), self.focus == OUTPUT),
                ]
                .align_y(iced::Center),
                row![
                    label("Модель", 12, FAINT).width(150),
                    frame(repaint(self.version, pick_list([1, 2], Some(self.version), Msg::Version).width(90).style(device_style)), self.focus == VERSION),
                    label("v2 экспериментальная", 12, FAINT),
                    Space::new().width(Length::Fill),
                    label("Буфер, мс", 12, FAINT),
                    frame(repaint(self.buffer, pick_list([10, 20, 30, 40, 60, 80], Some(self.buffer), Msg::Buffer).width(80).style(device_style)), self.focus == BUFFER),
                ]
                .spacing(10)
                .align_y(iced::Center),
            ]
            .spacing(8)
            .into()
        };
        let startup = column![
            row![frame(switch(self.app_autostart, Msg::AppAutostart, true), self.focus == APP_AUTOSTART), label("Запускать Mic Noize вместе с Windows (в трее)", 13, INK)].spacing(8).align_y(iced::Center),
            row![frame(switch(self.autostart, Msg::Autostart, true), self.focus == AUTOSTART), label("Держать виртуальный микрофон доступным после входа в Windows", 13, INK)].spacing(8).align_y(iced::Center),
        ]
        .spacing(6);
        // Only while the virtual microphone is missing: a button that can do nothing is noise.
        let mut device = column![
            row![
                label("Устройство Mic Noize", 13, DIM),
                label(self.device_state.label(), 13, match self.device_state {
                    engine::DeviceState::Ready => GREEN,
                    engine::DeviceState::UserAction => RED,
                    _ => DIM,
                }),
            ]
            .spacing(8),
            label(&self.device_detail, 12, FAINT),
        ]
        .spacing(8);
        if !self.driver_ready {
            device = device.push(label("Виртуальный микрофон не установлен: Windows запросит права администратора.", 12, DIM)).push(
                action(label(if self.driver_installing { "Устанавливаем…" } else { "Установить виртуальный микрофон" }, 13, INK), Msg::InstallDriver, self.focus == DRIVER, false)
                    .on_press_maybe((!self.driver_installing).then_some(Msg::InstallDriver)),
            );
        }
        let device = if self.repair_confirm {
            device.push(
                column![
                    bold("Восстановление устройства", 15, INK),
                    label("Обработка микрофона и наушников будет остановлена на время проверки. Текущая линия сохранится, если перенос не требуется.", 12, DIM),
                    row![frame(switch(self.repair_lines, Msg::RepairLines, true), self.focus == REPAIR_LINES), label("Освободить место для наушников и перенести старые линии Mic Noize", 13, INK)].spacing(8).align_y(iced::Center),
                    label("Стандартный вход TAG Microphone будет освобождён, в том числе после перезагрузок. При переносе старой линии Mic Noize её потребуется снова выбрать в Discord и других программах. Физический микрофон не меняется.", 12, DIM),
                    row![frame(switch(self.repair_reinstall, Msg::RepairReinstall, true), self.focus == REPAIR_REINSTALL), label("Разрешить переустановку драйвера, если проверка и перезапуск не помогут", 13, INK)].spacing(8).align_y(iced::Center),
                    label("При переустановке Windows запросит права администратора. Устройство может получить новый идентификатор — тогда его нужно снова выбрать в Discord и других программах.", 12, DIM),
                    row![
                        action(label("Восстановить", 13, ORANGE_DARK), Msg::RepairConfirm, self.focus == REPAIR_CONFIRM, true),
                        action(label("Отмена", 13, INK), Msg::RepairCancel, self.focus == REPAIR_CANCEL, false),
                    ]
                    .spacing(8),
                ]
                .spacing(8),
            )
        } else {
            device.push(
                row![
                    action(label(if self.repair_resume.is_some() { "Восстанавливаем…" } else { "Восстановить устройство" }, 13, INK), Msg::Repair, self.focus == REPAIR, false)
                        .on_press_maybe((!self.driver_installing && !self.core_installing && !self.quitting && !self.apply_pending).then_some(Msg::Repair)),
                    action(label("Обновить устройства", 13, INK), Msg::Refresh, self.focus == REFRESH, false),
                ]
                .spacing(8),
            )
        };
        let updates = column![
            row![label(format!("Mic Noize {}", env!("CARGO_PKG_VERSION")), 13, INK), label(&self.update_status, 12, if self.update_ready { GREEN } else { FAINT })].spacing(12).align_y(iced::Center),
            // "Обновить сейчас" exists only with an update to apply (the Tab order already
            // skips it otherwise); a disabled orange button read as the page's main action.
            row![
                action(label(if self.update_checking { "Проверка…" } else { "Проверить обновления" }, 13, if self.update_checking { FAINT } else { INK }), Msg::UpdateCheck, self.focus == UPDATE, false)
                    .on_press_maybe((!self.update_checking).then_some(Msg::UpdateCheck)),
            ]
            .push(self.update_ready.then(|| action(label("Обновить сейчас", 13, ORANGE_DARK), Msg::ApplyUpdate, self.focus == APPLY_UPDATE, true)))
            .spacing(8),
        ]
        .spacing(8);
        let diagnostics = column![
            numbers(format!("NVIDIA {:.2} мс   очередь {:.1} мс", self.snapshot.process_ms, self.snapshot.queue_ms), 13, DIM),
            numbers(
                format!(
                    "пропуски {} / {}   pitch {:.1} мс (макс {:.2} мс)",
                    self.snapshot.underruns, self.snapshot.drops, self.snapshot.pitch_delay_ms, self.snapshot.pitch_max_ms
                ),
                13,
                DIM,
            ),
            label("Буфер — запас от обрывов, не полная задержка. Pitch добавляет задержку только при удержании.", 12, FAINT),
            row![
                action(row![icon(glyph::LOGS, 12, INK), label("Логи и отчёт", 13, INK)].spacing(8).align_y(iced::Center), Msg::Page(5), self.focus == LOGS, false),
                Space::new().width(Length::Fill),
                action(label("Выход из Mic Noize", 13, INK), Msg::Quit, self.focus == QUIT, false),
            ]
            .spacing(8)
            .align_y(iced::Center),
            // Plays «Перезапустить» → update window → restart for real, without installing.
            action(label("Проверить анимацию обновления", 13, INK), Msg::RehearseUpdate, self.focus == REHEARSE, false),
        ]
        .spacing(8);
        // Two columns under the device card, so the page fits the default window unscrolled.
        column![
            title("Настройки"),
            card(column![heading_row(glyph::MIC, "Микрофон и выход", Space::new().into()), route].spacing(10)),
            row![
                column![
                    card(column![heading_row(glyph::REFRESH, "Запуск", Space::new().into()), startup].spacing(10)),
                    card(column![heading_row(glyph::SAVE, "Обновления", Space::new().into()), updates].spacing(10)),
                ]
                .spacing(14)
                .width(Length::FillPortion(1)),
                column![
                    card(column![heading_row(glyph::OUTPUT, "Виртуальный микрофон", Space::new().into()), device].spacing(10)),
                    card(column![heading_row(glyph::CHIP, "Диагностика", Space::new().into()), diagnostics].spacing(10)),
                ]
                .spacing(14)
                .width(Length::FillPortion(1)),
            ]
            .spacing(14),
        ]
        .spacing(14)
        .into()
    }
    /// A hotkey shown as keycaps that captures its key in place: click, press the key, done.
    /// A clash stays on the button in red and keeps waiting; a second click or Esc cancels; the
    /// small cross clears an existing binding while capturing.
    fn bind_button(&self, target: usize, key: u32, focused: bool, lit: bool, width: f32) -> Element<'_, Msg> {
        if self.binding != Some(target) {
            let content: Element<'_, Msg> = if key == 0 {
                label("+ клавиша", 12, FAINT).into()
            } else {
                let name = key_name(key);
                widget::Row::with_children(name.split(" + ").map(|part| {
                    let vk = match part { "Ctrl" => 0x11, "Alt" => 0x12, "Shift" => 0x10, _ => key & 255 };
                    tacho::keycap(part, lit || self.key_held(vk))
                }))
                .spacing(3)
                .align_y(iced::Center)
                .into()
            };
            return button(focus_target(content, focused))
                .padding([4, 6])
                .on_press(Msg::Bind(target))
                .style(move |_, status| {
                    let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
                    button::Style {
                        background: hover.then(|| HOVER.into()),
                        text_color: INK,
                        border: Border {
                            color: if focused { ORANGE } else if key == 0 { Color::from_rgb8(0x3A, 0x3B, 0x41) } else { Color::TRANSPARENT },
                            width: if focused { 2.0 } else { 1.0 },
                            radius: 7.0.into(),
                        },
                        ..Default::default()
                    }
                })
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
                border: Border { color: ORANGE, width: 2.0, radius: 8.0.into() },
                ..Default::default()
            });
        if key == 0 {
            return container(capture).width(width).into();
        }
        row![
            capture,
            button(container(icon(glyph::CLOSE, 9, RED)).center(Length::Fill))
                .width(24)
                .height(28)
                .padding(0)
                .on_press(Msg::ClearBind)
                .style(|_, status| button::Style {
                    background: matches!(status, button::Status::Hovered | button::Status::Pressed).then(|| HOVER.into()),
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

const ROW_HEIGHT: f32 = 34.0;
const ROW_SPACING: f32 = 2.0;
/// m:ss for clip lengths and playback position.
fn clock_text(seconds: f32) -> String {
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
    /// Renders every page headlessly: `MNR_DESIGN_DIR=<dir> cargo test design_snapshots -- --ignored`.
    #[test]
    fn update_points_parse() {
        assert_eq!(crate::update_window::parse_point("960.5, 540"), Some(iced::Point::new(960.5, 540.0)));
        assert_eq!(crate::update_window::parse_point("centered"), None);
        assert_eq!(crate::update_window::parse_point("1,NaN"), None);
    }

    /// Frames of the update shrink and grow; keyed (see-through) pixels are drawn as a checker.
    #[test]
    #[ignore]
    fn update_morph_frames() {
        use iced::advanced::{Renderer as _, Layout, graphics::Viewport};
        let dir = PathBuf::from(std::env::var("MNR_DESIGN_DIR").expect("MNR_DESIGN_DIR"));
        let (w, h) = (1040.0_f32, 740.0_f32);
        let size = Size::new(w as u32, h as u32);
        let full = iced::Rectangle::with_size(Size::new(w, h));
        let save = |app: &App, name: String| {
            let mut renderer = iced::Renderer::new(Font::with_name("Segoe UI"), iced::Pixels(14.0));
            let mut tree = iced::advanced::widget::Tree::empty();
            let mut element = app.view(window::Id::unique());
            tree.diff(element.as_widget());
            let layout = element.as_widget_mut().layout(&mut tree, &renderer, &iced::advanced::layout::Limits::new(Size::ZERO, Size::new(w, h)));
            let start = Instant::now();
            renderer.reset(full);
            element.as_widget().draw(&tree, &mut renderer, &Theme::Dark, &iced::advanced::renderer::Style { text_color: INK }, Layout::new(&layout), iced::mouse::Cursor::Unavailable, &full);
            let mut pixels = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
            let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
            renderer.draw(&mut pixels.as_mut(), &mut mask, &Viewport::with_physical_size(size, 1.0), &[full], BG);
            eprintln!("{name}: {:.1} ms", start.elapsed().as_secs_f64() * 1000.0);
            let mut data = pixels.data().to_vec();
            for (i, px) in data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                px.swap(0, 2);
                if px[0] == 1 && px[1] == 0 && px[2] == 1 {
                    let (x, y) = (i % size.width as usize / 16, i / size.width as usize / 16);
                    let v = if (x + y) % 2 == 0 { 0x50 } else { 0x68 };
                    px[0] = v; px[1] = v; px[2] = v + 0x10;
                }
            }
            let mut encoder = png::Encoder::new(std::fs::File::create(dir.join(format!("{name}.png"))).unwrap(), size.width, size.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&data).unwrap();
        };
        let (mut app, _) = App::from_settings(Settings::for_test("")).unwrap().unwrap();
        app.window = Some(window::Id::unique());
        let window = Size::new(w, h);
        let card = iced::Rectangle { x: (w - UPDATE_CARD.width) / 2.0, y: (h - UPDATE_CARD.height) / 2.0, width: UPDATE_CARD.width, height: UPDATE_CARD.height };
        let root = app.window_mosaic(window).unwrap();
        let waiting = mosaic_of::<Msg>(update_card(tacho::BarStage::Waiting, "0.2.14", "0.2.15"), UPDATE_CARD).unwrap();
        let done = mosaic_of::<Msg>(update_card(tacho::BarStage::Done, "", "0.2.15"), UPDATE_CARD).unwrap();
        for (kind, from, to, from_rect, to_rect, timeline, times) in [
            ("shrink", root.clone(), waiting, iced::Rectangle::with_size(window), card, tacho::MorphTimeline::SHRINK, [0u64, 150, 320, 450, 700, 900]),
            ("grow", done, root, card, iced::Rectangle::with_size(window), tacho::MorphTimeline::GROW, [0, 150, 300, 450, 700, 900]),
        ] {
            for ms in times {
                let base = match (kind, ms) {
                    ("shrink", ms) if ms < 120 => MorphBase::Root,
                    ("shrink", ms) if ms >= 800 => MorphBase::Card(tacho::BarStage::Waiting),
                    ("grow", ms) if ms < 100 => MorphBase::Card(tacho::BarStage::Done),
                    ("grow", ms) if ms >= 820 => MorphBase::Root,
                    _ => MorphBase::Key,
                };
                app.morph = Some(MorphView {
                    base,
                    from_version: "0.2.14".into(),
                    to_version: "0.2.15".into(),
                    anim: Some(tacho::Morph { from: from.clone(), to: to.clone(), from_rect, to_rect, start: Instant::now() - Duration::from_millis(ms), timeline, events: Vec::new() }),
                    hwnd: None,
                    center: None,
                });
                save(&app, format!("morph-{kind}-{ms:03}"));
            }
        }
        app.morph = Some(MorphView { base: MorphBase::Card(tacho::BarStage::Running(Instant::now() - Duration::from_millis(270))), from_version: "0.2.14".into(), to_version: "0.2.15".into(), anim: None, hwnd: None, center: None });
        save(&app, "update-window".into());
        app.morph = Some(MorphView { base: MorphBase::Card(tacho::BarStage::Launching), from_version: "0.2.14".into(), to_version: "0.2.15".into(), anim: None, hwnd: None, center: None });
        save(&app, "update-launching".into());
    }

    #[test]
    fn page_swap_repaints_whole_window() {
        let (mut app, _) = App::from_settings(Settings::for_test("")).unwrap().unwrap();
        let first = app.backdrop();
        let _ = app.update(Msg::Page(0));
        assert_eq!(app.backdrop(), first, "same page keeps region repaints");
        let _ = app.update(Msg::Page(2));
        assert_ne!(app.backdrop(), first, "a page swap forces one full pass");
        assert_eq!(app.backdrop().into_rgba8(), BG.into_rgba8(), "the same pixels");
        let _ = app.update(Msg::PageShiftReveal);
        assert_eq!(app.backdrop(), first);
        let _ = app.update(Msg::SoundpadFilter("a".into()));
        assert_ne!(app.backdrop(), first, "a rebuilt clip list forces one full pass");
    }
    /// Whole-window cost of each tab switch as the window pays it: update, view/diff/layout on
    /// the persistent tree, then a damaged-region raster. `MNR_TAB_BENCH_FOLDER` = real clips.
    #[test]
    #[ignore]
    fn tab_switch_timing() {
        use iced::advanced::{Renderer as _, Layout, graphics::{damage, Viewport}};
        let folder = std::env::var("MNR_TAB_BENCH_FOLDER").ok();
        let settings = folder.as_ref().map(|f| format!("[soundpad]\nfolder={f}")).unwrap_or_default();
        let (mut app, _) = App::from_settings(Settings::for_test(&settings)).unwrap().unwrap();
        app.window = Some(window::Id::unique());
        let scale: f32 = std::env::var("MNR_TAB_BENCH_SCALE").ok().map(|s| s.parse().unwrap()).unwrap_or(1.0);
        let (w, h) = (1040.0_f32, 740.0_f32);
        let size = Size::new((w * scale) as u32, (h * scale) as u32);
        let full = iced::Rectangle::with_size(Size::new(w, h));
        let mut renderer = iced::Renderer::new(Font::with_name("Segoe UI"), iced::Pixels(14.0));
        let mut pixels = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
        let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
        let viewport = Viewport::with_physical_size(size, scale);
        let mut tree = iced::advanced::widget::Tree::empty();
        let mut previous = Vec::new();
        let mut backdrop = app.backdrop();
        let mut frame = |app: &App| {
            let start = Instant::now();
            let mut element = app.view(window::Id::unique());
            tree.diff(element.as_widget());
            let layout = element.as_widget_mut().layout(&mut tree, &renderer, &iced::advanced::layout::Limits::new(Size::ZERO, Size::new(w, h)));
            let layout_ms = start.elapsed().as_secs_f64() * 1000.0;
            let start = Instant::now();
            renderer.reset(full);
            element.as_widget().draw(&tree, &mut renderer, &Theme::Dark, &iced::advanced::renderer::Style { text_color: INK }, Layout::new(&layout), iced::mouse::Cursor::Unavailable, &full);
            // As the tiny-skia compositor: a changed background repaints the window in one pass.
            let changes = if app.backdrop() != backdrop { vec![full] } else {
                damage::group(damage::diff(&previous, renderer.layers(), |layer| vec![layer.bounds], iced_tiny_skia::Layer::damage), full)
            };
            backdrop = app.backdrop();
            previous = renderer.layers().to_vec();
            let regions = changes.len();
            renderer.draw(&mut pixels.as_mut(), &mut mask, &viewport, &changes, BG);
            (layout_ms, start.elapsed().as_secs_f64() * 1000.0, regions)
        };
        let _ = frame(&app);
        eprintln!("{} clips, scale {scale}", app.sounds.len());
        for round in 0..2 {
            for (name, page) in [("effects", 6u8), ("soundpad", 4), ("rvc", 1), ("settings", 2), ("logs", 5), ("main", 0)] {
                let start = Instant::now();
                let _ = app.update(Msg::Page(page));
                let update_ms = start.elapsed().as_secs_f64() * 1000.0;
                let start = Instant::now();
                let mosaic = app.page_mosaic();
                let mosaic_ms = start.elapsed().as_secs_f64() * 1000.0;
                app.page_shift = None;
                let (layout_ms, draw_ms, regions) = frame(&app);
                eprintln!("round {round} {name:9}: update {update_ms:5.1} | mosaic {mosaic_ms:5.1} ({}) | new page view+layout {layout_ms:5.1} draw {draw_ms:5.1} ms in {regions} region(s)",
                    mosaic.is_some());
            }
        }
        let _ = app.update(Msg::Page(4));
        let _ = frame(&app);
        let sort = |app: &App| app.sound_sort;
        let mut steps: Vec<(&str, Msg)> = vec![("filter a", Msg::SoundpadFilter("a".into())), ("filter ge", Msg::SoundpadFilter("ge".into())), ("filter clear", Msg::SoundpadFilter(String::new()))];
        steps.push(("sort new", Msg::SoundpadSort(SoundSort::from_code((sort(&app).code() + 3) % 4))));
        steps.push(("sort back", Msg::SoundpadSort(sort(&app))));
        for (name, msg) in steps {
            let _ = app.update(msg);
            let (layout_ms, draw_ms, regions) = frame(&app);
            eprintln!("soundpad {name:12}: view+layout {layout_ms:5.1} draw {draw_ms:5.1} ms in {regions} region(s)");
        }
    }
    /// Frames of the page-switch pixelation, with their draw + raster time.
    #[test]
    #[ignore]
    fn page_shift_frames() {
        use iced::advanced::{Renderer as _, Layout, graphics::Viewport};
        let dir = PathBuf::from(std::env::var("MNR_DESIGN_DIR").expect("MNR_DESIGN_DIR"));
        let (w, h) = (1040.0_f32, 740.0_f32);
        let size = Size::new(w as u32, h as u32);
        let full = iced::Rectangle::with_size(Size::new(w, h));
        let frame = |app: &App| {
            let mut renderer = iced::Renderer::new(Font::with_name("Segoe UI"), iced::Pixels(14.0));
            let mut tree = iced::advanced::widget::Tree::empty();
            let mut element = app.view(window::Id::unique());
            tree.diff(element.as_widget());
            let layout = element.as_widget_mut().layout(&mut tree, &renderer, &iced::advanced::layout::Limits::new(Size::ZERO, Size::new(w, h)));
            let start = Instant::now();
            renderer.reset(full);
            element.as_widget().draw(&tree, &mut renderer, &Theme::Dark, &iced::advanced::renderer::Style { text_color: INK }, Layout::new(&layout), iced::mouse::Cursor::Unavailable, &full);
            let mut pixels = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
            let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
            renderer.draw(&mut pixels.as_mut(), &mut mask, &Viewport::with_physical_size(size, 1.0), &[full], BG);
            (pixels, start.elapsed().as_secs_f64() * 1000.0)
        };
        // As the window does it: repaint only the regions that changed since the last frame.
        let windowed = |app: &App, tree: &mut iced::advanced::widget::Tree, previous: &mut Vec<iced_tiny_skia::Layer>, renderer: &mut iced::Renderer, pixels: &mut tiny_skia::Pixmap| {
            use iced::advanced::graphics::damage;
            let mut element = app.view(window::Id::unique());
            tree.diff(element.as_widget());
            let layout = element.as_widget_mut().layout(tree, renderer, &iced::advanced::layout::Limits::new(Size::ZERO, Size::new(w, h)));
            let start = Instant::now();
            renderer.reset(full);
            element.as_widget().draw(tree, renderer, &Theme::Dark, &iced::advanced::renderer::Style { text_color: INK }, Layout::new(&layout), iced::mouse::Cursor::Unavailable, &full);
            let changes = damage::group(damage::diff(previous, renderer.layers(), |layer| vec![layer.bounds], iced_tiny_skia::Layer::damage), full);
            *previous = renderer.layers().to_vec();
            let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
            renderer.draw(&mut pixels.as_mut(), &mut mask, &Viewport::with_physical_size(size, 1.0), &changes, BG);
            (changes.len(), start.elapsed().as_secs_f64() * 1000.0)
        };
        let (mut app, _) = App::from_settings(Settings::for_test("")).unwrap().unwrap();
        app.window = Some(window::Id::unique());
        let _ = frame(&app); // lays out the page area
        app.sound_folder = Some(PathBuf::from("test sounds"));
        app.sounds = (0..454).map(|i| Sound {
            name: format!("Section {} - Sound {i:03}.wav", i / 5), path: PathBuf::new(),
            key: 0, volume: 100, played: 0, modified: 0, state: SoundState::Unloaded,
        }).collect();
        for (name, page) in [("main", 0u8), ("effects", 6), ("soundpad", 4), ("rvc", 1), ("settings", 2), ("logs", 5)] {
            let _ = app.update(Msg::Page(page));
            let started = Instant::now();
            let mosaic = app.page_mosaic();
            eprintln!("mosaic {name}: {:.1} ms ({:?})", started.elapsed().as_secs_f64() * 1000.0, mosaic.map(|m| (m.width, m.height)));
        }
        let _ = app.update(Msg::Page(0));
        app.page_shift = None;
        let started = Instant::now();
        let from = app.page_mosaic().unwrap();
        app.effects_page = true;
        let to = app.page_mosaic().unwrap();
        eprintln!("two offscreen pages: {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
        {
            let mut renderer = iced::Renderer::new(Font::with_name("Segoe UI"), iced::Pixels(14.0));
            let mut pixels = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
            let mut previous = Vec::new();
            let mut tree = iced::advanced::widget::Tree::empty();
            let begin = Instant::now() - Duration::from_millis(1);
            app.page_shift = Some((from.clone(), to.clone(), begin));
            for step in 0..24u64 {
                app.page_shift = Some((from.clone(), to.clone(), begin - Duration::from_millis(step * 16)));
                let (regions, took) = windowed(&app, &mut tree, &mut previous, &mut renderer, &mut pixels);
                eprintln!("windowed frame {:3} ms: {regions} damage regions, {took:.1} ms", step * 16);
            }
        }
        for ms in [0u64, 60, 120, 169, 200, 260, 330, 370] {
            app.page_shift = Some((from.clone(), to.clone(), Instant::now() - Duration::from_millis(ms)));
            let (pixels, took) = frame(&app);
            eprintln!("frame at {ms} ms: {took:.1} ms");
            let mut data = pixels.data().to_vec();
            for px in data.as_chunks_mut::<4>().0 {
                px.swap(0, 2);
            }
            let mut encoder = png::Encoder::new(std::fs::File::create(dir.join(format!("shift-{ms:03}.png"))).unwrap(), size.width, size.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&data).unwrap();
        }
    }
    #[test]
    #[ignore]
    fn design_snapshots() {
        use iced::advanced::{Renderer as _, Layout, graphics::{damage, Viewport}};
        let dir = PathBuf::from(std::env::var("MNR_DESIGN_DIR").expect("MNR_DESIGN_DIR"));
        let render = |app: &App, name: &str| {
            let (w, h) = (1040.0_f32, 740.0_f32);
            let size = Size::new(w as u32, h as u32);
            let mut renderer = iced::Renderer::new(Font::with_name("Segoe UI"), iced::Pixels(14.0));
            let mut tree = iced::advanced::widget::Tree::empty();
            let limits = iced::advanced::layout::Limits::new(Size::ZERO, Size::new(w, h));
            let mut element = app.view(window::Id::unique());
            tree.diff(element.as_widget());
            let layout = element.as_widget_mut().layout(&mut tree, &renderer, &limits);
            renderer.reset(iced::Rectangle::with_size(Size::new(w, h)));
            element.as_widget().draw(&tree, &mut renderer, &Theme::Dark,
                &iced::advanced::renderer::Style { text_color: INK }, Layout::new(&layout),
                iced::mouse::Cursor::Unavailable, &iced::Rectangle::with_size(Size::new(w, h)));
            let changes = damage::group(damage::diff(&[], renderer.layers(),
                |layer| vec![layer.bounds], iced_tiny_skia::Layer::damage),
                iced::Rectangle::with_size(Size::new(w, h)));
            let mut pixels = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
            let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
            renderer.draw(&mut pixels.as_mut(), &mut mask, &Viewport::with_physical_size(size, 1.0), &changes, BG);
            // The renderer writes BGRA into the pixmap; PNG wants RGBA.
            let mut data = pixels.data().to_vec();
            for px in data.as_chunks_mut::<4>().0 {
                px.swap(0, 2);
            }
            let mut encoder = png::Encoder::new(std::fs::File::create(dir.join(format!("design-{name}.png"))).unwrap(), size.width, size.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&data).unwrap();
        };
        let (mut app, _) = App::from_settings(Settings::for_test("")).unwrap().unwrap();
        app.window = Some(window::Id::unique());
        app.inputs = vec![Device { id: "mic".into(), name: "Microphone (HyperX QuadCast S)".into() }];
        app.input = app.inputs.first().cloned();
        app.outputs = vec![
            Device { id: "TAG".into(), name: "Mic Noize Microphone".into() },
            Device { id: "dac".into(), name: "Speakers (SMSL USB DAC)".into() },
        ];
        app.output = app.outputs.first().cloned();
        app.headphone_output = app.outputs.get(1).cloned();
        // (modifiers << 8) | virtual key: 1 Ctrl, 2 Alt; 36 Home, 4/6 mouse 3/5, 0x54 T.
        app.keys = [3 << 8 | 0x54, 3 << 8 | 36, 1 << 8 | 36, 2 << 8 | 36, 36, 0, 3 << 8 | 6, 1 << 8 | 6, 2 << 8 | 6, 6, 1 << 8 | 4, 2 << 8 | 4, 4];
        app.in_peak = 0.2;
        app.peak = 0.08;
        app.controls.intensity = 0.99;
        render(&app, "main");
        app.controls.intensity = 1.35;
        app.route_open = true;
        render(&app, "main-red");
        app.controls.intensity = 0.99;
        app.route_open = false;
        app.headphone_page = true;
        render(&app, "headphones");
        app.headphone_page = false;
        app.effects_page = true;
        app.clips = (0..6).map(|i| Sound {
            name: format!("Запись 2026-09-25 14-2{i}-0{i} (mix).wav"), path: PathBuf::new(),
            key: 0, volume: 100, played: 0, modified: 0, state: SoundState::Loaded(2.4),
        }).collect();
        render(&app, "effects");
        app.keys_down[0] = 1 << 0x11;
        render(&app, "effects-ctrl-held");
        app.keys_down = [0; 4];
        app.effects_page = false;
        app.soundpad_page = true;
        app.sound_folder = Some(PathBuf::from(r"E:\Dropbox\sounds"));
        app.sounds = ["a chto", "a-a chevo", "aga spasiba", "aleeo", "bogdan - 48chasov", "bruh", "chto proishodit", "davay davay", "gerych - nu davay", "nastya - privet"]
            .iter().map(|n| Sound {
                name: format!("{n}.wav"), path: PathBuf::new(), key: 0, volume: 100, played: 0, modified: 0,
                state: SoundState::Loaded(3.0),
            }).collect();
        app.sound_scroll = (0.0, 420.0);
        render(&app, "soundpad");
        app.soundpad_page = false;
        app.rvc_page = true;
        render(&app, "rvc");
        app.rvc_advanced = true;
        render(&app, "rvc-advanced");
        app.rvc_advanced = false;
        app.rvc_page = false;
        app.details = true;
        render(&app, "settings");
        app.details = false;
        app.logs_page = true;
        render(&app, "logs");
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
        assert_eq!(clock_text(0.0), "0:00");
        assert_eq!(clock_text(7.4), "0:07");
        assert_eq!(clock_text(125.6), "2:06");
    }
    #[test]
    fn focus_scroll_moves_only_when_outside_viewport() {
        assert_eq!(focus_scroll_delta(120.0, 30.0, 100.0, 200.0), 0.0);
        assert_eq!(focus_scroll_delta(90.0, 30.0, 100.0, 200.0), -18.0);
        assert_eq!(focus_scroll_delta(280.0, 30.0, 100.0, 200.0), 18.0);
    }
}
