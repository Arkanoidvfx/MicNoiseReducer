//! "Тахометр": the segmented slider of the 0.2.8 design. Skewed shapes are anti-aliased
//! tiny-skia paths (its `geometry` feature), so they look like the mockup at any scale. Every
//! animation is time-based and self-scheduled through redraw requests, so a still, unfocused
//! or hidden window draws nothing extra.
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::text::{self, Paragraph as _, Renderer as _, Text};
use iced::Renderer;
use iced_tiny_skia::Geometry;
use iced_tiny_skia::geometry::{Cache, Frame};
use iced_tiny_skia::graphics::cache::{Cached as _, Group};
use iced_tiny_skia::graphics::geometry::{Path, Renderer as _, frame::Backend as _};
use std::cell::RefCell;
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell};
use iced::keyboard;
use iced::mouse;
use iced::window::{self, RedrawRequest};
use iced::{Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme};
use std::ops::RangeInclusive;
use std::time::{Duration, Instant};

const SKEW: f32 = 0.364; // tan 20°
const WAVE_CYCLE: f32 = 8000.0;
const WAVE_STEP: f32 = 64.0;
const WAVE_LEN: f32 = 568.0;
const STAGGER: f32 = 9.0;
const IGNITE_STEP: f32 = 14.0;
const IDLE_AFTER: Duration = Duration::from_millis(1500);
const OFF: Color = Color::from_rgb8(0x1E, 0x1F, 0x22);
const OFF_RED: Color = Color::from_rgb8(0x2C, 0x1A, 0x1A);
const OFF_EDGE: Color = Color::from_rgb8(0x2A, 0x2B, 0x30);
const GLOW: Color = Color::from_rgb8(0xFF, 0xBE, 0x8C);
const LOW: [f32; 3] = [196.0, 108.0, 44.0];
const HIGH: [f32; 3] = [255.0, 176.0, 112.0];
const HEAD: Color = Color::from_rgb8(0xFF, 0xE3, 0xC7);
pub const HOT: Color = Color::from_rgb8(0xFF, 0x5A, 0x4E);
const TAG: Color = Color::from_rgb8(0xFF, 0x9F, 0x56);
const INK: Color = Color::from_rgb8(242, 237, 227);
const DARK: Color = Color::from_rgb8(0x1A, 0x12, 0x06);

/// The numbers font of the design: Windows' own condensed DIN-like face.
pub fn numbers() -> Font {
    Font { weight: iced::font::Weight::Bold, ..Font::with_name("Bahnschrift") }
}

/// Shared clocks: `epoch` keeps the idle waves of all sliders in step; `opened` starts the
/// warm-up sweep once per shown window.
#[derive(Clone, Copy)]
pub struct Clock {
    pub epoch: Instant,
    pub opened: Option<Instant>,
    /// False while the window is unfocused: nothing keeps animating then.
    pub animate: bool,
}

pub struct Tacho<'a, Message> {
    range: RangeInclusive<f32>,
    value: f32,
    step: f32,
    default: f32,
    on_change: Box<dyn Fn(f32) -> Message + 'a>,
    format: Box<dyn Fn(f32) -> String + 'a>,
    segments: usize,
    compact: bool,
    /// Fill from this value instead of the left edge (signed pitch).
    origin: Option<f32>,
    /// Values strictly above this are the red zone.
    red_above: Option<f32>,
    phase: f32,
    clock: Clock,
    enabled: bool,
    width: Length,
}

pub fn tacho<'a, Message>(
    range: RangeInclusive<f32>,
    value: f32,
    on_change: impl Fn(f32) -> Message + 'a,
    clock: Clock,
) -> Tacho<'a, Message> {
    let default = *range.start();
    Tacho {
        range,
        value,
        step: 1.0,
        default,
        on_change: Box::new(on_change),
        format: Box::new(|v| format!("{v:.0}%")),
        segments: 20,
        compact: false,
        origin: None,
        red_above: None,
        phase: 0.0,
        clock,
        enabled: true,
        width: Length::Fill,
    }
}

impl<'a, Message> Tacho<'a, Message> {
    pub fn step(mut self, step: f32) -> Self { self.step = step; self }
    pub fn default(mut self, default: f32) -> Self { self.default = default; self }
    pub fn format(mut self, format: impl Fn(f32) -> String + 'a) -> Self { self.format = Box::new(format); self }
    pub fn segments(mut self, n: usize) -> Self { self.segments = n.max(2); self }
    pub fn compact(mut self) -> Self { self.compact = true; self }
    pub fn origin(mut self, origin: f32) -> Self { self.origin = Some(origin); self }
    pub fn red_above(mut self, value: f32) -> Self { self.red_above = Some(value); self }
    /// Milliseconds the idle wave lags behind the shared clock, to chain sliders.
    pub fn phase(mut self, ms: f32) -> Self { self.phase = ms; self }
    pub fn enabled(mut self, enabled: bool) -> Self { self.enabled = enabled; self }
    pub fn width(mut self, width: impl Into<Length>) -> Self { self.width = width.into(); self }

    fn frac(&self, v: f32) -> f32 {
        let (min, max) = (*self.range.start(), *self.range.end());
        if max <= min { 0.0 } else { ((v - min) / (max - min)).clamp(0.0, 1.0) }
    }
    /// Lit segments `[lo, hi)` and the head index.
    fn lit(&self, v: f32) -> (i32, i32, i32) {
        let n = self.segments as f32;
        match self.origin {
            Some(o) => {
                let mid = (self.frac(o) * n).round() as i32;
                let pos = (self.frac(v) * n).round() as i32;
                if pos < mid { (pos, mid, pos) } else { (mid, pos, pos - 1) }
            }
            None => {
                let pos = (self.frac(v) * n - 1e-4).ceil().max(0.0) as i32;
                (0, pos, pos - 1)
            }
        }
    }
    fn red_from(&self) -> i32 {
        self.red_above
            .map(|r| (self.frac(r) * self.segments as f32).floor() as i32)
            .unwrap_or(i32::MAX)
    }
    fn in_red(&self, v: f32) -> bool {
        self.red_above.is_some_and(|r| v > r)
    }
    fn track(&self, bounds: Rectangle) -> Rectangle {
        let right = if self.compact { 64.0 } else { 6.0 };
        let (h, bottom) = if self.compact { (16.0, 7.0) } else { (24.0, 6.0) };
        Rectangle {
            x: bounds.x + 6.0,
            y: bounds.y + bounds.height - bottom - h,
            width: (bounds.width - 6.0 - right).max(1.0),
            height: h,
        }
    }
    fn seg_width(&self) -> f32 {
        if self.compact { 7.0 } else { 14.0 }
    }
    fn seg_x(&self, track: Rectangle, i: usize) -> f32 {
        let w = self.seg_width();
        track.x + (track.width - w) * i as f32 / (self.segments - 1) as f32
    }
    fn locate(&self, track: Rectangle, x: f32) -> f32 {
        let (min, max) = (*self.range.start(), *self.range.end());
        let t = ((x - track.x) / track.width).clamp(0.0, 1.0);
        let v = min + t * (max - min);
        ((v - min) / self.step).round() * self.step + min
    }
}

#[derive(Default)]
struct State {
    ready: bool,
    drag: bool,
    mods: keyboard::Modifiers,
    hover: bool,
    shown: f32,
    last: f32,
    old: (i32, i32),
    changed: Option<Instant>,
    head_at: Option<Instant>,
    peak: Option<(i32, Instant)>,
    touched: Option<Instant>,
    frame: Option<Instant>,
    hover_since: Option<Instant>,
    painted: Painted,
}

impl<'a, Message> Widget<Message, Theme, Renderer> for Tacho<'a, Message> {
    fn tag(&self) -> tree::Tag { tree::Tag::of::<State>() }
    fn state(&self) -> tree::State { tree::State::new(State::default()) }
    fn size(&self) -> Size<Length> {
        Size { width: self.width, height: Length::Fixed(if self.compact { 30.0 } else { 66.0 }) }
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::atomic(limits, self.width, if self.compact { 30.0 } else { 66.0 })
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Renderer,
        _: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let track = self.track(bounds);
        let hit = Rectangle { width: track.width + 12.0, x: track.x - 6.0, ..bounds };
        let publish = |v: f32, shell: &mut Shell<'_, Message>| {
            let v = v.clamp(*self.range.start(), *self.range.end());
            if (v - self.value).abs() > f32::EPSILON {
                shell.publish((self.on_change)(v));
            }
        };
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if self.enabled => {
                if let Some(p) = cursor.position_over(hit) {
                    if state.mods.command() {
                        publish(self.default, shell);
                    } else {
                        publish(self.locate(track, p.x), shell);
                        state.drag = true;
                    }
                    state.touched = Some(Instant::now());
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.drag => {
                state.drag = false;
                state.touched = Some(Instant::now());
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.drag {
                    publish(self.locate(track, position.x), shell);
                    state.touched = Some(Instant::now());
                    shell.capture_event();
                }
                let hover = self.enabled && hit.contains(*position);
                if hover != state.hover {
                    state.hover = hover;
                    state.hover_since = hover.then(Instant::now);
                    shell.request_redraw();
                }
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => state.mods = *m,
            Event::Window(window::Event::RedrawRequested(now)) => {
                let now = *now;
                let v = self.value;
                if !state.ready {
                    state.ready = true;
                    state.shown = v;
                    state.last = v;
                    let (lo, hi, _) = self.lit(v);
                    state.old = (lo, hi);
                }
                if (v - state.last).abs() > f32::EPSILON {
                    let (lo, hi, head) = self.lit(state.last);
                    let (_, _, new_head) = self.lit(v);
                    state.old = (lo, hi);
                    state.changed = Some(now);
                    state.touched = Some(now);
                    if new_head != head {
                        state.head_at = Some(now);
                        if self.origin.is_none() && new_head < head {
                            let top = state.peak.map_or(head, |(p, _)| p.max(head));
                            state.peak = Some((top, now));
                        }
                    }
                    state.last = v;
                }
                let dt = state.frame.map_or(0.0, |f| now.saturating_duration_since(f).as_secs_f32());
                state.frame = Some(now);
                let k = 1.0 - (-dt / 0.06).exp();
                state.shown += (v - state.shown) * k;
                if (v - state.shown).abs() < self.step * 0.5 {
                    state.shown = v;
                }
                if state.peak.is_some_and(|(_, at)| now.saturating_duration_since(at) > Duration::from_millis(600)) {
                    state.peak = None;
                }
                if let Some(next) = self.next_frame(state, now) {
                    shell.request_redraw_at(next);
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let now = Instant::now();
        let bounds = layout.bounds();
        let track = self.track(bounds);
        let n = self.segments;
        let alpha = if self.enabled { 1.0 } else { 0.35 };
        let (lo, hi, head) = self.lit(self.value);
        let red_from = self.red_from();
        let in_red = self.in_red(self.value);
        let ignition = self.ignition(now);
        let idle = self.idle(state, now);
        let since_change = state.changed.map(|c| now.saturating_duration_since(c).as_secs_f32() * 1000.0);
        let flash = state.head_at
            .map(|a| now.saturating_duration_since(a).as_secs_f32() / 0.3)
            .filter(|t| *t < 1.0);
        let pulse = 0.85 + 0.15 * (now.saturating_duration_since(self.clock.epoch).as_secs_f32() * std::f32::consts::TAU / 1.8).cos();
        let scan = state.hover_since.filter(|_| state.hover && self.clock.animate).map(|s| {
            let t = now.saturating_duration_since(s).as_secs_f32() / 0.75;
            (t.fract() * 1.6 - 0.3) * track.width + track.x
        });
        let press = if state.drag { 1.2 } else { 1.0 };
        let w = self.seg_width();
        let mut shapes = Vec::with_capacity(n * 6);
        for i in 0..n {
            let ii = i as i32;
            let mut lit = ii >= lo && ii < hi;
            let mut is_head = lit && ii == head;
            if let (Some(ms), false) = (since_change, ignition.is_some()) {
                // Segments change in a ripple that starts at the new head.
                if ms < (ii - head).unsigned_abs() as f32 * STAGGER {
                    lit = ii >= state.old.0 && ii < state.old.1;
                    is_head = false;
                }
            }
            if let Some(sweep) = ignition {
                match sweep {
                    Sweep::Up(t) => { lit = t >= i as f32 * IGNITE_STEP; is_head = false; }
                    Sweep::Down(t) => {
                        if t < (n - i) as f32 * IGNITE_STEP { lit = true; is_head = false; }
                    }
                }
            }
            let red = ii >= red_from;
            let hot = lit && red && !is_head;
            let ghost = !lit && state.peak.is_some_and(|(p, _)| p == ii);
            let mut color = if lit {
                if is_head { HEAD } else if red { HOT } else { lerp(i as f32 / n as f32) }
            } else if ghost {
                Color { a: 0.4, ..HEAD }
            } else if red { OFF_RED } else { OFF };
            if hot {
                color = scale(color, pulse);
            }
            let x = self.seg_x(track, i);
            let mut height = track.height * press;
            let mut lift = 0.0;
            let mut glow = 0.0;
            if is_head && let Some(t) = flash {
                height *= 1.0 + 0.5 * (1.0 - t);
                glow = 1.0 - t;
            }
            if idle {
                let (dy, b) = wave(self.wave_time(now, i), self.compact);
                lift = dy;
                glow = glow.max(b);
            }
            if let Some(sx) = scan {
                let d = ((x + w / 2.0) - sx).abs() / (track.width * 0.08);
                if d < 1.0 { glow = glow.max((1.0 - d) * 0.5); }
            }
            if glow > 0.0 {
                color = brighten(color, glow);
            }
            let y = track.y + track.height + lift - height;
            if self.enabled && is_head {
                halo(&mut shapes, x, y, w, height, if self.compact { 7.0 } else { 11.0 }, Color { a: 0.6, ..GLOW });
            } else if self.enabled && hot {
                halo(&mut shapes, x, y, w, height, 6.0, Color { a: 0.35 * pulse, ..HOT });
            }
            if lit {
                segment(&mut shapes, x, y, w, height, Color { a: color.a * alpha, ..color });
            } else {
                // Unlit segments are outlined, as in the mockup.
                segment(&mut shapes, x, y, w, height, Color { a: alpha, ..OFF_EDGE });
                segment(&mut shapes, x + 1.0, y + 1.0, w - 2.0, height - 2.0, Color { a: color.a * alpha, ..color });
            }
        }
        let text_value = if matches!(ignition, Some(Sweep::Up(_))) {
            "MAX".to_owned()
        } else {
            let shown = if state.ready { state.shown } else { self.value };
            (self.format)((shown / self.step).round() * self.step)
        };
        let red_text = in_red && ignition.is_none();
        if self.compact {
            state.painted.draw(renderer, bounds.expand(24.0), shapes);
            put(
                renderer,
                label(text_value, 15.0, Size::new(58.0, bounds.height), text::Alignment::Right),
                Point::new(bounds.x + bounds.width, bounds.center_y()),
                Color { a: alpha, ..if red_text { HOT } else { INK } },
                bounds,
            );
        } else {
            let text = label(text_value, 14.0, Size::new(120.0, 23.0), text::Alignment::Center);
            let tw = measure(&text).width + 24.0;
            let at = if hi > lo { head as f32 + 0.5 } else { lo as f32 };
            let cx = track.x + w / 2.0 + (track.width - w) * (at - 0.5).max(0.0) / (n - 1) as f32;
            let tx = (cx - tw / 2.0).clamp(bounds.x, bounds.x + bounds.width - tw);
            let ty = bounds.y + if state.drag { -4.0 } else { 0.0 };
            // The tag's slant matches the mockup's clip-path: 7 px over its height.
            slant(&mut shapes, tx, ty, tw - 7.0, 23.0, 7.0, Color { a: alpha, ..if red_text { HOT } else { TAG } });
            state.painted.draw(renderer, bounds.expand(24.0), shapes);
            put(renderer, text, Point::new(tx + tw / 2.0, ty + 11.5), Color { a: alpha, ..DARK }, bounds.expand(8.0));
        }
    }

    fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, _: &Rectangle, _: &Renderer) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if self.enabled && (state.drag || cursor.is_over(layout.bounds())) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

#[derive(Clone, Copy)]
enum Sweep {
    Up(f32),
    Down(f32),
}

impl<Message> Tacho<'_, Message> {
    fn ignition(&self, now: Instant) -> Option<Sweep> {
        let opened = self.clock.opened?;
        let t = now.saturating_duration_since(opened).as_secs_f32() * 1000.0 - 300.0;
        let up = self.segments as f32 * IGNITE_STEP + 260.0;
        let down = self.segments as f32 * IGNITE_STEP + 60.0;
        if t < 0.0 {
            None
        } else if t < up {
            Some(Sweep::Up(t))
        } else if t < up + down {
            Some(Sweep::Down(t - up))
        } else {
            None
        }
    }
    fn idle(&self, state: &State, now: Instant) -> bool {
        self.clock.animate
            && self.enabled
            && !state.drag
            && self.ignition(now).is_none()
            && state.touched.is_none_or(|t| now.saturating_duration_since(t) >= IDLE_AFTER)
    }
    fn wave_time(&self, now: Instant, i: usize) -> f32 {
        let t = now.saturating_duration_since(self.clock.epoch).as_secs_f32() * 1000.0
            - self.phase
            - i as f32 * WAVE_STEP;
        t.rem_euclid(WAVE_CYCLE)
    }
    /// When the next frame is needed, if at all.
    fn next_frame(&self, state: &State, now: Instant) -> Option<RedrawRequest> {
        if !self.clock.animate {
            return None;
        }
        let soon = |ms: u64| RedrawRequest::At(now + Duration::from_millis(ms));
        let busy = state.drag
            || (self.value - state.shown).abs() > f32::EPSILON
            || state.changed.is_some_and(|c| now.saturating_duration_since(c) < Duration::from_millis(self.segments as u64 * STAGGER as u64 + 20))
            || state.head_at.is_some_and(|a| now.saturating_duration_since(a) < Duration::from_millis(320))
            || state.peak.is_some()
            || self.ignition(now).is_some()
            || self.clock.opened.is_some_and(|o| now.saturating_duration_since(o) < Duration::from_millis(300));
        if busy {
            return Some(RedrawRequest::NextFrame);
        }
        if state.hover || (self.in_red(self.value) && self.enabled) {
            return Some(soon(33));
        }
        if let Some(t) = state.touched {
            let quiet = now.saturating_duration_since(t);
            if quiet < IDLE_AFTER {
                return Some(RedrawRequest::At(t + IDLE_AFTER));
            }
        }
        // Idle: frames only while the wave crosses this slider, then sleep until it returns.
        let first = self.wave_time(now, 0);
        let span = (self.segments - 1) as f32 * WAVE_STEP + WAVE_LEN;
        if first < span {
            Some(soon(33))
        } else {
            Some(RedrawRequest::At(now + Duration::from_millis((WAVE_CYCLE - first) as u64)))
        }
    }
}

/// Lift in px and brightening of the idle wave at `t` ms into a segment's cycle.
fn wave(t: f32, compact: bool) -> (f32, f32) {
    let keys: [(f32, f32, f32); 5] = if compact {
        [(0.0, 0.0, 0.0), (144.0, -2.0, 0.2), (284.0, -6.0, 0.7), (424.0, -2.0, 0.2), (WAVE_LEN, 0.0, 0.0)]
    } else {
        [(0.0, 0.0, 0.0), (144.0, -3.0, 0.2), (284.0, -10.0, 0.7), (424.0, -3.0, 0.2), (WAVE_LEN, 0.0, 0.0)]
    };
    if t >= WAVE_LEN {
        return (0.0, 0.0);
    }
    for pair in keys.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if t <= b.0 {
            let k = (t - a.0) / (b.0 - a.0);
            return (a.1 + (b.1 - a.1) * k, (a.2 + (b.2 - a.2) * k) * 0.6);
        }
    }
    (0.0, 0.0)
}

pub fn lerp(t: f32) -> Color {
    let c = |i: usize| (LOW[i] + (HIGH[i] - LOW[i]) * t) / 255.0;
    Color::from_rgb(c(0), c(1), c(2))
}
fn brighten(c: Color, k: f32) -> Color {
    let k = k.clamp(0.0, 1.0) * 0.45;
    Color { r: c.r + (1.0 - c.r) * k, g: c.g + (1.0 - c.g) * k, b: c.b + (1.0 - c.b) * k, a: c.a }
}
fn scale(c: Color, k: f32) -> Color {
    Color { a: c.a * k, ..c }
}

/// A parallelogram whose top edge sits `lean` px right of its bottom edge.
#[derive(Clone, Copy, PartialEq)]
struct Shape {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    lean: f32,
    /// Corner rounding, for glow rings.
    round: f32,
    color: Color,
}

fn slant(shapes: &mut Vec<Shape>, x: f32, y: f32, w: f32, h: f32, lean: f32, color: Color) {
    if w > 0.0 && h > 0.0 && color.a > 0.0 {
        shapes.push(Shape { x, y, w, h, lean, round: 0.0, color });
    }
}

fn geometry(clip: Rectangle, shapes: &[Shape]) -> Geometry {
    let mut frame = Frame::new(clip);
    fill_shapes(&mut frame, shapes);
    frame.into_geometry()
}

fn fill_shapes(frame: &mut Frame, shapes: &[Shape]) {
    for &Shape { x, y, w, h, lean, round, color } in shapes {
        let corners = [Point::new(x + lean, y), Point::new(x + lean + w, y), Point::new(x + w, y + h), Point::new(x, y + h)];
        let path = Path::new(|p| {
            if round > 0.0 {
                let [a, b, ..] = corners;
                p.move_to(Point::new((a.x + b.x) / 2.0, y));
                for i in 1..=4 {
                    p.arc_to(corners[i % 4], corners[(i + 1) % 4], round);
                }
            } else {
                p.move_to(corners[0]);
                for &c in &corners[1..] {
                    p.line_to(c);
                }
            }
            p.close();
        });
        frame.fill(&path, color);
    }
}

/// The last drawn shapes. tiny-skia treats uncached geometry as changed on every frame, which
/// would repaint each slider whenever anything else in the window redraws; an unchanged frame
/// reuses the cache instead and costs nothing.
#[derive(Default)]
struct Painted(RefCell<Option<(Rectangle, Vec<Shape>, Cache)>>);

impl Painted {
    fn draw(&self, renderer: &mut Renderer, clip: Rectangle, shapes: Vec<Shape>) {
        let mut last = self.0.borrow_mut();
        if !last.as_ref().is_some_and(|(c, s, _)| *c == clip && *s == shapes) {
            let cache = geometry(clip, &shapes).cache(Group::unique(), None);
            *last = Some((clip, shapes, cache));
        }
        if let Some((_, _, cache)) = last.as_ref() {
            renderer.draw_geometry(Geometry::load(cache));
        }
    }
}

/// A box skewed by 20° around its centre, like the mockup's `skewX(-20deg)`.
fn segment(shapes: &mut Vec<Shape>, x: f32, y: f32, w: f32, h: f32, color: Color) {
    slant(shapes, x - h * SKEW / 2.0, y, w, h, h * SKEW, color);
}

/// A soft glow around a segment: stacked, growing translucent copies of its shape stand in
/// for the mockup's blurred box-shadow, which tiny-skia cannot blur.
/// Each ring is the segment grown by `e` with corners rounded by `e`, the outline a blur's
/// level lines follow, so the glow has no spikes at the slanted corners.
fn halo(shapes: &mut Vec<Shape>, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
    const RINGS: usize = 8;
    let cos = 1.0 / (1.0 + SKEW * SKEW).sqrt();
    for k in 1..=RINGS {
        let e = radius * k as f32 / RINGS as f32;
        let (gh, side) = (h + 2.0 * e, e / cos);
        shapes.push(Shape {
            x: x - h * SKEW / 2.0 - side - e * SKEW,
            y: y - e,
            w: w + 2.0 * side,
            h: gh,
            lean: gh * SKEW,
            round: e,
            color: Color { a: color.a * 0.09, ..color },
        });
    }
}

fn text_width(content: &str, size: f32, font: Font) -> f32 {
    measure(&Text {
        content: content.to_owned(),
        bounds: Size::INFINITE,
        size: Pixels(size),
        line_height: text::LineHeight::default(),
        font,
        align_x: text::Alignment::Left,
        align_y: iced::alignment::Vertical::Top,
        shaping: text::Shaping::Basic,
        wrapping: text::Wrapping::None,
    })
    .width
}

fn measure(text: &Text<String, Font>) -> Size {
    <Renderer as text::Renderer>::Paragraph::with_text(Text {
        content: text.content.as_str(),
        bounds: text.bounds,
        size: text.size,
        line_height: text.line_height,
        font: text.font,
        align_x: text::Alignment::Left,
        align_y: iced::alignment::Vertical::Top,
        shaping: text.shaping,
        wrapping: text.wrapping,
    })
    .min_bounds()
}

/// `fill_text` anchored by the text's own alignment. tiny-skia repaints only damaged regions and
/// records a text as starting at its position, so centred or right-aligned text that moves or
/// changes would leave stale pixels behind; drawing it from its measured top-left avoids that.
fn put(renderer: &mut Renderer, text: Text<String, Font>, at: Point, color: Color, clip: Rectangle) {
    let size = measure(&text);
    let x = match text.align_x {
        text::Alignment::Center => at.x - size.width / 2.0,
        text::Alignment::Right => at.x - size.width,
        _ => at.x,
    };
    let y = match text.align_y {
        iced::alignment::Vertical::Center => at.y - size.height / 2.0,
        iced::alignment::Vertical::Bottom => at.y - size.height,
        iced::alignment::Vertical::Top => at.y,
    };
    renderer.fill_text(
        Text {
            bounds: Size::new(size.width + 2.0, size.height),
            align_x: text::Alignment::Left,
            align_y: iced::alignment::Vertical::Top,
            ..text
        },
        Point::new(x, y),
        color,
        clip,
    );
}

fn label(content: String, size: f32, bounds: Size, align: text::Alignment) -> Text<String, Font> {
    Text {
        content,
        bounds,
        size: Pixels(size),
        line_height: text::LineHeight::default(),
        font: numbers(),
        align_x: align,
        align_y: iced::alignment::Vertical::Center,
        shaping: text::Shaping::Basic,
        wrapping: text::Wrapping::None,
    }
}

/// Police tape behind a card whose value is in the red zone: two hatched bands with the
/// word crawling along them. It rolls out once, then only the text moves.
pub fn caution<'a, Message: 'a>(clock: Clock, shown: bool) -> Element<'a, Message> {
    Element::new(Caution { clock, shown })
}
struct Caution {
    clock: Clock,
    /// Hidden tapes stay in the tree so the card's content keeps its widget state.
    shown: bool,
}
#[derive(Default)]
struct Born(Option<Instant>);
impl<Message> Widget<Message, Theme, Renderer> for Caution {
    fn tag(&self) -> tree::Tag { tree::Tag::of::<Born>() }
    fn state(&self) -> tree::State { tree::State::new(Born::default()) }
    fn size(&self) -> Size<Length> { Size { width: Length::Fill, height: Length::Fill } }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fill)
    }
    fn update(&mut self, tree: &mut Tree, event: &Event, _: Layout<'_>, _: mouse::Cursor, _: &Renderer, _: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, _: &Rectangle) {
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let born = tree.state.downcast_mut::<Born>();
            if !self.shown {
                born.0 = None;
                return;
            }
            born.0.get_or_insert(*now);
            if self.clock.animate {
                shell.request_redraw_at(RedrawRequest::At(*now + Duration::from_millis(40)));
            }
        }
    }
    fn draw(&self, tree: &Tree, renderer: &mut Renderer, _: &Theme, _: &renderer::Style, layout: Layout<'_>, _: mouse::Cursor, _: &Rectangle) {
        let b = layout.bounds();
        let now = Instant::now();
        if !self.shown {
            return;
        }
        // Without a frame event yet (a headless render) the tapes are shown fully rolled out.
        let age = tree.state.downcast_ref::<Born>().0.map_or(1.0, |born| now.saturating_duration_since(born).as_secs_f32());
        let t = now.saturating_duration_since(self.clock.epoch).as_secs_f32();
        renderer.with_layer(b, |renderer| {
            renderer.fill_quad(
                Quad { bounds: b, border: Border { radius: 10.0.into(), ..Border::default() }, ..Quad::default() },
                Color { a: 0.035, ..HOT },
            );
            let word = "ЭКСПЕРИМЕНТАЛЬНО   ///   ";
            let run = text_width(word, 11.0, numbers());
            let mut frame = Frame::new(b);
            // Crossed like police tape, not mirrored: they cross high on the card and stay above
            // the slider's track, so the slider hides little of them.
            let cross = Point::new(b.x + b.width * 0.62, b.y + b.height * 0.30);
            for (k, degrees) in [-7.0_f32, 11.0].into_iter().enumerate() {
                let fade = ((age - k as f32 * 0.12) / 0.55).clamp(0.0, 1.0);
                if fade <= 0.0 {
                    continue;
                }
                let (h, len) = (24.0, b.width * 1.8);
                let (x0, y0) = (-len / 2.0, -h / 2.0);
                let mut shapes = vec![Shape { x: x0, y: y0, w: len, h, lean: 0.0, round: 0.0, color: Color { a: 0.08 * fade, ..HOT } }];
                // 45° hatching like the mockup's repeating gradient: 14 px stripes across.
                let (stripe, period) = (14.0 * std::f32::consts::SQRT_2, 28.0 * std::f32::consts::SQRT_2);
                let mut x = x0 - h - period + (t * 6.0) % period;
                while x < x0 + len {
                    slant(&mut shapes, x, y0, stripe, h, h, Color { a: 0.13 * fade, ..HOT });
                    x += period;
                }
                for edge in [y0, y0 + h - 1.0] {
                    slant(&mut shapes, x0, edge, len, 1.0, 0.0, Color { a: 0.35 * fade, ..HOT });
                }
                frame.push_transform();
                frame.translate(iced::Vector::new(cross.x, cross.y));
                frame.rotate(degrees.to_radians());
                fill_shapes(&mut frame, &shapes);
                // The word crawls; the two tapes move in opposite directions.
                let speed = if k == 0 { -11.0 } else { 9.0 };
                let start = x0 - run + (t * speed).rem_euclid(run);
                let repeat = (len / run) as usize + 2;
                frame.fill_text(tape_text(word.repeat(repeat), Point::new(start, 0.0), Color { a: 0.5 * fade, ..Color::from_rgb8(0xFF, 0x82, 0x78) }));
                frame.pop_transform();
            }
            renderer.draw_geometry(frame.into_geometry());
        });
    }
}

/// A tape's caption; rotated geometry text is drawn as glyph outlines, clipped with the card.
fn tape_text(content: String, position: Point, color: Color) -> iced_tiny_skia::graphics::geometry::Text {
    iced_tiny_skia::graphics::geometry::Text {
        content,
        position,
        max_width: f32::INFINITY,
        color,
        size: Pixels(11.0),
        line_height: text::LineHeight::default(),
        font: numbers(),
        align_x: text::Alignment::Left,
        align_y: iced::alignment::Vertical::Center,
        shaping: text::Shaping::Basic,
    }
}

/// The reverse row's practice word: its letters fly over to the mirrored places and back.
/// Hovering holds it reversed so it can be read aloud.
pub fn reverse_word<'a, Message: 'a>(word: &str, clock: Clock) -> Element<'a, Message> {
    let word: Vec<char> = word.trim().to_uppercase().chars().take(12).collect();
    let word: Vec<char> = if word.is_empty() { "ПРИВЕТ".chars().collect() } else { word };
    let widths = word.iter().map(|c| text_width(&c.to_string(), 15.0, numbers())).collect();
    Element::new(Reverse { word, widths, clock })
}
struct Reverse {
    word: Vec<char>,
    /// Each letter's own advance, so Ж and Г keep natural spacing both ways round.
    widths: Vec<f32>,
    clock: Clock,
}
#[derive(Default)]
struct Hover(bool);
const LETTER_GAP: f32 = 1.5;
const REV_CYCLE: f32 = 4000.0;
impl Reverse {
    fn cycle(&self, now: Instant) -> f32 {
        (now.saturating_duration_since(self.clock.epoch).as_secs_f32() * 1000.0).rem_euclid(REV_CYCLE) / REV_CYCLE
    }
    /// 0 plain, 1 reversed, in between while the letters fly.
    fn progress(&self, now: Instant, hover: bool) -> f32 {
        if hover {
            return 1.0;
        }
        let t = self.cycle(now);
        let ease = |x: f32| x * x * (3.0 - 2.0 * x);
        if t < 0.25 {
            0.0
        } else if t < 0.40 {
            ease((t - 0.25) / 0.15)
        } else if t < 0.75 {
            1.0
        } else if t < 0.90 {
            1.0 - ease((t - 0.75) / 0.15)
        } else {
            0.0
        }
    }
    fn width(&self) -> f32 {
        24.0 + self.widths.iter().map(|w| w + LETTER_GAP).sum::<f32>() + 4.0
    }
}
impl<Message> Widget<Message, Theme, Renderer> for Reverse {
    fn tag(&self) -> tree::Tag { tree::Tag::of::<Hover>() }
    fn state(&self) -> tree::State { tree::State::new(Hover::default()) }
    fn size(&self) -> Size<Length> {
        Size { width: Length::Fixed(self.width()), height: Length::Fixed(28.0) }
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::atomic(limits, Length::Fixed(self.width()), Length::Fixed(28.0))
    }
    fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, _: &Renderer, _: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, _: &Rectangle) {
        let hover = &mut tree.state.downcast_mut::<Hover>().0;
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let over = cursor.is_over(layout.bounds());
                if over != *hover {
                    *hover = over;
                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) if self.clock.animate && !*hover => {
                let t = self.cycle(*now);
                let moving = (0.25..0.40).contains(&t) || (0.75..0.90).contains(&t);
                if moving {
                    shell.request_redraw_at(RedrawRequest::NextFrame);
                } else {
                    let edge = [0.25, 0.75, 1.25].into_iter().find(|e| *e > t).unwrap_or(1.25);
                    let wait = ((edge - t) * REV_CYCLE).max(16.0);
                    shell.request_redraw_at(RedrawRequest::At(*now + Duration::from_millis(wait as u64)));
                }
            }
            _ => {}
        }
    }
    fn draw(&self, tree: &Tree, renderer: &mut Renderer, _: &Theme, _: &renderer::Style, layout: Layout<'_>, _: mouse::Cursor, _: &Rectangle) {
        let b = layout.bounds();
        let hover = tree.state.downcast_ref::<Hover>().0;
        let p = self.progress(Instant::now(), hover);
        let tint = Color {
            r: INK.r + (TAG.r - INK.r) * p,
            g: INK.g + (TAG.g - INK.g) * p,
            b: INK.b + (TAG.b - INK.b) * p,
            a: 1.0,
        };
        let glyph = |content: String, size: f32, font: Font| Text {
            content,
            bounds: Size::new(40.0, 28.0),
            size: Pixels(size),
            line_height: text::LineHeight::default(),
            font,
            align_x: text::Alignment::Center,
            align_y: iced::alignment::Vertical::Center,
            shaping: text::Shaping::Basic,
            wrapping: text::Wrapping::None,
        };
        let clip = Rectangle { y: b.y - 12.0, height: b.height + 24.0, ..b };
        put(
            renderer,
            glyph("\u{E7A7}".into(), 12.0, Font::with_name("Segoe MDL2 Assets")),
            Point::new(b.x + 8.0, b.center_y()),
            if p > 0.5 { TAG } else { Color::from_rgb8(0x85, 0x86, 0x8D) },
            clip,
        );
        let arc = (p * std::f32::consts::PI).sin();
        let advance = |w: &f32| w + LETTER_GAP;
        for (i, ch) in self.word.iter().enumerate() {
            // Letter i starts after the letters before it, and ends up after those behind it.
            let from = self.widths[..i].iter().map(advance).sum::<f32>();
            let to = self.widths[i + 1..].iter().map(advance).sum::<f32>();
            let d = to - from;
            let lift = if d.abs() < 0.5 { -4.0 } else { -d.signum() * (3.0 + d.abs() / 10.0) };
            let x = b.x + 24.0 + from + self.widths[i] / 2.0 + d * p;
            put(renderer, glyph(ch.to_string(), 15.0, numbers()), Point::new(x, b.center_y() + lift * arc), tint, clip);
        }
    }
    fn mouse_interaction(&self, _: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, _: &Rectangle, _: &Renderer) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) { mouse::Interaction::Text } else { mouse::Interaction::default() }
    }
}

/// A hotkey keycap that sinks and lights up while its key is held, as in the mockup's `kpress`:
/// a quick dip past the rest depth, then it settles; releasing springs back with a small lift.
pub fn keycap<'a, Message: 'a>(label: &str, down: bool) -> Element<'a, Message> {
    let width = text_width(label, 11.0, Font::with_name("Consolas")) + 14.0;
    Element::new(Keycap { label: label.to_owned(), down, width })
}
struct Keycap {
    label: String,
    down: bool,
    width: f32,
}
#[derive(Default)]
struct KeyMotion {
    down: bool,
    since: Option<Instant>,
}
const CAP_H: f32 = 19.0;
const TRAVEL: f32 = 3.0;
const PRESS_MS: f32 = 280.0;
const RELEASE_MS: f32 = 240.0;
impl Keycap {
    /// Depth in px, cap scale and orange mix for the current moment.
    fn pose(&self, motion: &KeyMotion, now: Instant) -> (f32, f32, f32) {
        let ms = motion.since.map_or(f32::MAX, |s| now.saturating_duration_since(s).as_secs_f32() * 1000.0);
        let ease = |x: f32| 1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3);
        if self.down {
            let t = ms / PRESS_MS;
            if t >= 1.0 {
                return (TRAVEL, 1.0, 1.0);
            }
            let (depth, scale) = if t < 0.4 {
                let k = ease(t / 0.4);
                (TRAVEL * 1.5 * k, 1.0 - 0.1 * k)
            } else {
                let k = ease((t - 0.4) / 0.6);
                (TRAVEL * (1.5 - 0.5 * k), 0.9 + 0.1 * k)
            };
            (depth, scale, ease(t / 0.35))
        } else {
            let t = ms / RELEASE_MS;
            if t >= 1.0 {
                return (0.0, 1.0, 0.0);
            }
            let depth = if t < 0.5 { TRAVEL - (TRAVEL + 1.2) * ease(t / 0.5) } else { -1.2 * (1.0 - ease((t - 0.5) / 0.5)) };
            (depth, 1.0, 1.0 - ease(t / 0.5))
        }
    }
}
impl<Message> Widget<Message, Theme, Renderer> for Keycap {
    fn tag(&self) -> tree::Tag { tree::Tag::of::<KeyMotion>() }
    fn state(&self) -> tree::State { tree::State::new(KeyMotion { down: self.down, since: None }) }
    fn size(&self) -> Size<Length> {
        Size { width: Length::Fixed(self.width), height: Length::Fixed(CAP_H + TRAVEL + 2.0) }
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::atomic(limits, Length::Fixed(self.width), Length::Fixed(CAP_H + TRAVEL + 2.0))
    }
    fn update(&mut self, tree: &mut Tree, event: &Event, _: Layout<'_>, _: mouse::Cursor, _: &Renderer, _: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, _: &Rectangle) {
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let motion = tree.state.downcast_mut::<KeyMotion>();
            if motion.down != self.down {
                motion.down = self.down;
                motion.since = Some(*now);
            }
            let span = if self.down { PRESS_MS } else { RELEASE_MS };
            if motion.since.is_some_and(|s| now.saturating_duration_since(s).as_secs_f32() * 1000.0 < span) {
                shell.request_redraw_at(RedrawRequest::NextFrame);
            }
        }
    }
    fn draw(&self, tree: &Tree, renderer: &mut Renderer, _: &Theme, _: &renderer::Style, layout: Layout<'_>, _: mouse::Cursor, _: &Rectangle) {
        let b = layout.bounds();
        let (depth, scale, lit) = self.pose(tree.state.downcast_ref::<KeyMotion>(), Instant::now());
        let mix = |a: Color, b: Color| Color { r: a.r + (b.r - a.r) * lit, g: a.g + (b.g - a.g) * lit, b: a.b + (b.b - a.b) * lit, a: 1.0 };
        let top = b.y + 1.0;
        let (w, h) = (b.width * scale, CAP_H * scale);
        let cap = Rectangle { x: b.center_x() - w / 2.0, y: top + depth.max(-1.2) + (CAP_H - h) / 2.0, width: w, height: h };
        for e in [6.0_f32, 4.0, 2.0] {
            if lit > 0.0 {
                renderer.fill_quad(
                    Quad { bounds: cap.expand(e), border: Border { radius: (5.0 + e).into(), ..Border::default() }, ..Quad::default() },
                    Color { a: 0.12 * lit, ..TAG },
                );
            }
        }
        // The key's side below the cap; a held cap sinks into it.
        renderer.fill_quad(
            Quad { bounds: Rectangle { x: b.x, y: top + TRAVEL, width: b.width, height: CAP_H }, border: Border { radius: 5.0.into(), ..Border::default() }, ..Quad::default() },
            mix(Color::from_rgb8(0x11, 0x12, 0x14), Color::from_rgb8(0x8A, 0x4E, 0x1F)),
        );
        renderer.fill_quad(
            Quad { bounds: cap, border: Border { color: mix(Color::from_rgb8(0x3A, 0x3B, 0x41), Color::from_rgb8(0xC9, 0x72, 0x2F)), width: 1.0, radius: 5.0.into() }, ..Quad::default() },
            mix(Color::from_rgb8(0x2A, 0x2B, 0x30), TAG),
        );
        put(
            renderer,
            Text {
                content: self.label.clone(),
                bounds: Size::new(b.width, CAP_H),
                size: Pixels(11.0 * scale),
                line_height: text::LineHeight::default(),
                font: Font::with_name("Consolas"),
                align_x: text::Alignment::Center,
                align_y: iced::alignment::Vertical::Center,
                shaping: text::Shaping::Basic,
                wrapping: text::Wrapping::None,
            },
            cap.center(),
            mix(Color::from_rgb8(0xE8, 0xE3, 0xD9), DARK),
            b.expand(8.0),
        );
    }
}

/// The До / После level bars. The raw microphone is drawn as ragged, gently flickering bars; the denoised output as crisp slanted segments in the sliders'
/// style, with a glowing head and a peak mark that holds, then sinks. `level` is 0..1.
pub fn level_meter<'a, Message: 'a>(level: f32, noisy: bool, color: Color) -> Element<'a, Message> {
    Element::new(LevelMeter { level: level.clamp(0.0, 1.0), noisy, color })
}
struct LevelMeter {
    level: f32,
    noisy: bool,
    color: Color,
}
const METER_H: f32 = 14.0;
const CLEAN_SEGMENTS: usize = 40;
const PEAK_HOLD: Duration = Duration::from_millis(900);
#[derive(Default)]
struct MeterState {
    peak: f32,
    peak_at: Option<Instant>,
    frame: Option<Instant>,
    painted: Painted,
}
/// Stable pseudo-random 0..1 for bar `i` in flicker frame `t`.
fn grain(i: u32, t: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B1) ^ t.wrapping_mul(0x85EB_CA77);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}
impl<Message> Widget<Message, Theme, Renderer> for LevelMeter {
    fn tag(&self) -> tree::Tag { tree::Tag::of::<MeterState>() }
    fn state(&self) -> tree::State { tree::State::new(MeterState::default()) }
    fn update(&mut self, tree: &mut Tree, event: &Event, _: Layout<'_>, _: mouse::Cursor, _: &Renderer, _: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, _: &Rectangle) {
        let Event::Window(window::Event::RedrawRequested(now)) = event else { return };
        if self.noisy {
            return;
        }
        let m = tree.state.downcast_mut::<MeterState>();
        let dt = m.frame.map_or(0.0, |f| now.saturating_duration_since(f).as_secs_f32());
        m.frame = Some(*now);
        if self.level >= m.peak {
            m.peak = self.level;
            m.peak_at = Some(*now);
        } else if m.peak_at.is_none_or(|at| now.saturating_duration_since(at) >= PEAK_HOLD) {
            m.peak = (m.peak - dt * 0.6).max(self.level);
        }
        if m.peak > self.level {
            let wake = m.peak_at.map_or(*now, |at| at + PEAK_HOLD);
            shell.request_redraw_at(if wake > *now { RedrawRequest::At(wake) } else { RedrawRequest::NextFrame });
        }
    }
    fn size(&self) -> Size<Length> {
        Size { width: Length::Fill, height: Length::Fixed(METER_H) }
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fixed(METER_H))
    }
    fn draw(&self, tree: &Tree, renderer: &mut Renderer, _: &Theme, _: &renderer::Style, layout: Layout<'_>, _: mouse::Cursor, _: &Rectangle) {
        let b = layout.bounds();
        let track = Color::from_rgb8(0x26, 0x27, 0x2B);
        if !self.noisy {
            let m = tree.state.downcast_ref::<MeterState>();
            let n = CLEAN_SEGMENTS;
            let (gap, h) = (3.0, METER_H - 2.0);
            let w = (b.width - 8.0 - gap * (n - 1) as f32) / n as f32;
            let lit = (self.level * n as f32).round() as usize;
            let peak = ((m.peak.max(self.level) * n as f32).round() as usize).min(n);
            let dim = Color { r: self.color.r * 0.38, g: self.color.g * 0.38, b: self.color.b * 0.38, a: 1.0 };
            let mut shapes = Vec::with_capacity(n * 2 + 8);
            for i in 0..n {
                let x = b.x + 4.0 + i as f32 * (w + gap);
                let y = b.y + 1.0;
                if i < lit {
                    // A calm gradient from deep to full colour; the newest segment glows.
                    let t = i as f32 / n as f32;
                    let c = Color { r: dim.r + (self.color.r - dim.r) * t, g: dim.g + (self.color.g - dim.g) * t, b: dim.b + (self.color.b - dim.b) * t, a: 1.0 };
                    if i + 1 == lit {
                        halo(&mut shapes, x, y, w, h, 6.0, Color { a: 0.55, ..self.color });
                        segment(&mut shapes, x, y, w, h, brighten(self.color, 0.9));
                    } else {
                        segment(&mut shapes, x, y, w, h, c);
                    }
                } else if i + 1 == peak && peak > lit {
                    // The held peak: an outlined segment in the bar's colour.
                    segment(&mut shapes, x, y, w, h, Color { a: 0.9, ..self.color });
                    segment(&mut shapes, x + 1.5, y + 1.5, w - 3.0, h - 3.0, OFF);
                } else {
                    segment(&mut shapes, x, y, w, h, OFF_EDGE);
                    segment(&mut shapes, x + 1.0, y + 1.0, w - 2.0, h - 2.0, OFF);
                }
            }
            m.painted.draw(renderer, b.expand(10.0), shapes);
            return;
        }
        // The meter redraws with every level update; the flicker frame follows real time,
        // slow enough to read as a restless signal rather than strobing.
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| (d.as_millis() / 160) as u32);
        const BARS: u32 = 60;
        let gap = 2.0;
        let w = (b.width - gap * (BARS - 1) as f32) / BARS as f32;
        let lit = (self.level * BARS as f32).round() as u32;
        for i in 0..BARS {
            let x = b.x + i as f32 * (w + gap);
            let r = grain(i, t);
            if i < lit {
                // Ragged heights and uneven brightness: hiss, not a clean signal.
                let h = METER_H * (0.55 + 0.45 * r);
                let k = 0.85 + 0.3 * grain(i + 97, t);
                let c = Color { r: (self.color.r * k).min(1.0), g: (self.color.g * k).min(1.0), b: (self.color.b * k).min(1.0), a: 1.0 };
                renderer.fill_quad(Quad { bounds: Rectangle { x, y: b.y + (METER_H - h) / 2.0, width: w, height: h }, ..Quad::default() }, c);
            } else {
                renderer.fill_quad(Quad { bounds: Rectangle { x, y: b.y + METER_H / 2.0 - 1.0, width: w, height: 2.0 }, ..Quad::default() }, track);
            }
        }
    }
}

/// The app mark from the design canvas: grey noise specks on the left turning into clean orange
/// voice bars on a dark tile. The bars rise a little with the output `level` (0..1).
pub fn logo<'a, Message: 'a>(size: f32, level: f32) -> Element<'a, Message> {
    Element::new(Logo { size, level: level.clamp(0.0, 1.0) })
}
struct Logo {
    size: f32,
    level: f32,
}
/// x, y, w, h, corner radius and grey level on the mark's 64-unit grid; grey 0 means orange.
const LOGO_BARS: [(f32, f32, f32, f32, f32, u8); 9] = [
    (8.0, 29.0, 4.0, 4.0, 1.0, 0x4A),
    (13.0, 21.0, 4.0, 4.0, 1.0, 0x5E),
    (13.0, 37.0, 3.0, 3.0, 1.0, 0x5E),
    (18.0, 28.0, 4.0, 4.0, 1.0, 0x7A),
    (24.0, 20.0, 5.0, 10.0, 2.5, 0x8E),
    (24.0, 33.0, 5.0, 11.0, 2.5, 0x8E),
    (33.0, 24.0, 6.0, 16.0, 3.0, 0),
    (42.0, 13.0, 6.0, 38.0, 3.0, 0),
    (51.0, 20.0, 6.0, 24.0, 3.0, 0),
];
impl<Message> Widget<Message, Theme, Renderer> for Logo {
    fn size(&self) -> Size<Length> {
        Size { width: Length::Fixed(self.size), height: Length::Fixed(self.size) }
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::atomic(limits, Length::Fixed(self.size), Length::Fixed(self.size))
    }
    fn draw(&self, _: &Tree, renderer: &mut Renderer, _: &Theme, _: &renderer::Style, layout: Layout<'_>, _: mouse::Cursor, _: &Rectangle) {
        let b = layout.bounds();
        let k = self.size / 64.0;
        renderer.fill_quad(
            Quad { bounds: b, border: Border { color: Color::from_rgb8(0x2A, 0x2B, 0x30), width: 1.0, radius: (14.0 * k).into() }, ..Quad::default() },
            Color::from_rgb8(0x1B, 0x1C, 0x1F),
        );
        for (x, y, w, h, r, grey) in LOGO_BARS {
            let (y, h, color) = if grey == 0 {
                // Voice bars grow about their centre with the level, never past the tile.
                let grown = (h * (1.0 + 0.4 * self.level)).min(54.0);
                (y + (h - grown) / 2.0, grown, TAG)
            } else {
                (y, h, Color::from_rgb8(grey, grey + 1, grey + 7))
            };
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle { x: b.x + x * k, y: b.y + y * k, width: w * k, height: h * k },
                    border: Border { radius: (r * k).into(), ..Border::default() },
                    snap: false,
                    ..Quad::default()
                },
                color,
            );
        }
    }
}

/// A page painted offscreen at 1/[`MOSAIC_CELL`] scale: one RGB cell per 4×4 logical px.
pub struct Mosaic {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<[u8; 3]>,
}
pub const MOSAIC_CELL: f32 = 4.0;
const SHIFT_OUT_MS: f32 = 170.0;
const SHIFT_IN_MS: f32 = 210.0;
const BLOCK_MAX: f32 = 48.0;
/// The page area's last laid-out size, so pages can be painted offscreen at the same size.
// ponytail: one window, one page area; a per-window map if the UI ever opens a second one.
static PAGE_AREA: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn page_area() -> Option<Size> {
    let v = PAGE_AREA.load(std::sync::atomic::Ordering::Relaxed);
    let size = Size::new(f32::from_bits((v >> 32) as u32), f32::from_bits(v as u32));
    (size.width >= 1.0 && size.height >= 1.0).then_some(size)
}

/// The page switch: the old page pixelates into big blocks, then the new one resolves out of
/// them. `shift` holds both pages and the start; `done` is sent when it has played out.
/// `reveal` is sent when the mosaic starts to fade: until then it hides the page completely, so the
/// page underneath need not be drawn at all.
pub fn page_shift<'a, Message: Clone + 'a>(shift: Option<&(std::sync::Arc<Mosaic>, std::sync::Arc<Mosaic>, Instant)>, reveal: Message, done: Message) -> Element<'a, Message> {
    Element::new(PageShift { shift: shift.cloned(), reveal, done })
}
struct PageShift<Message> {
    shift: Option<(std::sync::Arc<Mosaic>, std::sync::Arc<Mosaic>, Instant)>,
    reveal: Message,
    done: Message,
}
/// When the new page starts to show through the fading mosaic.
const SHIFT_REVEAL_MS: f32 = SHIFT_OUT_MS + SHIFT_IN_MS * 0.7;
#[derive(Default)]
struct ShiftState {
    revealed: Option<Instant>,
}
impl<Message: Clone> Widget<Message, Theme, Renderer> for PageShift<Message> {
    fn tag(&self) -> tree::Tag { tree::Tag::of::<ShiftState>() }
    fn state(&self) -> tree::State { tree::State::new(ShiftState::default()) }
    fn size(&self) -> Size<Length> {
        Size { width: Length::Fill, height: Length::Fill }
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        let node = layout::atomic(limits, Length::Fill, Length::Fill);
        let size = node.size();
        PAGE_AREA.store(((size.width.to_bits() as u64) << 32) | size.height.to_bits() as u64, std::sync::atomic::Ordering::Relaxed);
        node
    }
    fn update(&mut self, tree: &mut Tree, event: &Event, _: Layout<'_>, _: mouse::Cursor, _: &Renderer, _: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, _: &Rectangle) {
        if let (Event::Window(window::Event::RedrawRequested(now)), Some((_, _, start))) = (event, &self.shift) {
            let ms = now.saturating_duration_since(*start).as_secs_f32() * 1000.0;
            let state = tree.state.downcast_mut::<ShiftState>();
            if ms >= SHIFT_REVEAL_MS && state.revealed != Some(*start) {
                state.revealed = Some(*start);
                shell.publish(self.reveal.clone());
            }
            if ms >= SHIFT_OUT_MS + SHIFT_IN_MS {
                shell.publish(self.done.clone());
            } else {
                shell.request_redraw_at(RedrawRequest::NextFrame);
            }
        }
    }
    fn draw(&self, _: &Tree, renderer: &mut Renderer, _: &Theme, _: &renderer::Style, layout: Layout<'_>, _: mouse::Cursor, _: &Rectangle) {
        let Some((from, to, start)) = &self.shift else { return };
        let b = layout.bounds();
        let ms = Instant::now().saturating_duration_since(*start).as_secs_f32() * 1000.0;
        let (page, block, alpha) = if ms < SHIFT_OUT_MS {
            // Blocks grow at an even pace, so every frame shows a new size.
            let p = ms / SHIFT_OUT_MS;
            (from, MOSAIC_CELL + (BLOCK_MAX - MOSAIC_CELL) * p, 1.0)
        } else {
            // Coarse blocks resolve quickly, the fine last steps settle gently; the last stretch
            // fades over the real, sharp page underneath.
            let p = ((ms - SHIFT_OUT_MS) / SHIFT_IN_MS).min(1.0);
            (to, BLOCK_MAX - (BLOCK_MAX - MOSAIC_CELL) * (1.0 - (1.0 - p).powi(3)), ((1.0 - p) / (1.0 - 0.7)).min(1.0))
        };
        // Any whole-pixel block size, sampled from the fixed small mosaic: the picture changes on
        // every frame instead of in 4 px jumps, which read as a low frame rate.
        let side = block.round().max(MOSAIC_CELL);
        let (cols, rows) = ((b.width / side).ceil() as usize, (b.height / side).ceil() as usize);
        // One cached geometry for the whole mosaic: the window then repaints a single region per
        // frame. Thousands of separate quads made it repaint the page dozens of times a frame.
        let mut shapes = Vec::new();
        for row in 0..rows {
            // Merge equal neighbours into one shape: flat backgrounds cost one per row.
            let mut run: Option<(usize, [u8; 3])> = None;
            for col in 0..=cols {
                let color = (col < cols).then(|| {
                    let (u0, v0) = (col as f32 * side / b.width, row as f32 * side / b.height);
                    let (u1, v1) = (((col + 1) as f32 * side / b.width).min(1.0), ((row + 1) as f32 * side / b.height).min(1.0));
                    sample(page, u0, v0, u1, v1).map(|v| v.round() as u8)
                });
                let same = |a: [u8; 3], z: [u8; 3]| a.iter().zip(z).all(|(x, y)| x.abs_diff(y) <= 3);
                match (run, color) {
                    (Some((_, c)), Some(next)) if same(c, next) => {}
                    (current, next) => {
                        if let Some((first, c)) = current {
                            // A hair of overlap hides anti-aliased seams.
                            slant(
                                &mut shapes,
                                b.x + first as f32 * side,
                                b.y + row as f32 * side,
                                (col - first) as f32 * side + 0.6,
                                side + 0.6,
                                0.0,
                                Color { a: alpha, ..Color::from_rgb8(c[0], c[1], c[2]) },
                            );
                        }
                        run = next.map(|c| (col, c));
                    }
                }
            }
        }
        let cache = geometry(b, &shapes).cache(Group::unique(), None);
        renderer.draw_geometry(Geometry::load(&cache));
    }
}

/// The colour Windows keys out of a colour-keyed (layered) window: pixels of exactly this colour
/// are transparent and let clicks through. Nothing in the palette is this dark purple-black.
pub const KEY: Color = Color::from_rgb8(1, 0, 1);

/// What the update window's bar shows.
#[derive(Clone, Copy, PartialEq)]
pub enum BarStage {
    /// Empty: shown until the window is on screen, so every process draws the same frame.
    Waiting,
    /// A solid block of lit segments runs across from this moment; no fading trail.
    Running(Instant),
    Done,
}

/// The update window's bar in the sliders' segment style.
pub fn run_bar<'a, Message: 'a>(stage: BarStage) -> Element<'a, Message> {
    Element::new(RunBar { stage })
}
struct RunBar {
    stage: BarStage,
}
const BAR_SEGMENTS: usize = 24;
const BAR_BLOCK: usize = 5;
const BAR_STEP_MS: u64 = 30;
impl RunBar {
    fn position(&self, now: Instant) -> usize {
        match self.stage {
            BarStage::Running(since) => (now.saturating_duration_since(since).as_millis() as u64 / BAR_STEP_MS) as usize % (BAR_SEGMENTS + BAR_BLOCK),
            _ => 0,
        }
    }
}
impl<Message> Widget<Message, Theme, Renderer> for RunBar {
    fn tag(&self) -> tree::Tag { tree::Tag::of::<Painted>() }
    fn state(&self) -> tree::State { tree::State::new(Painted::default()) }
    fn size(&self) -> Size<Length> {
        Size { width: Length::Fill, height: Length::Fixed(20.0) }
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fixed(20.0))
    }
    fn update(&mut self, _: &mut Tree, event: &Event, _: Layout<'_>, _: mouse::Cursor, _: &Renderer, _: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, _: &Rectangle) {
        if let (Event::Window(window::Event::RedrawRequested(now)), BarStage::Running(since)) = (event, self.stage) {
            let step = Duration::from_millis(BAR_STEP_MS);
            let elapsed = now.saturating_duration_since(since);
            let next = since + step * (elapsed.as_millis() as u32 / BAR_STEP_MS as u32 + 1);
            shell.request_redraw_at(RedrawRequest::At(next));
        }
    }
    fn draw(&self, tree: &Tree, renderer: &mut Renderer, _: &Theme, _: &renderer::Style, layout: Layout<'_>, _: mouse::Cursor, _: &Rectangle) {
        let b = layout.bounds();
        let (w, h) = (13.0, 20.0);
        let pos = self.position(Instant::now());
        let mut shapes = Vec::with_capacity(BAR_SEGMENTS * 2);
        let green = |t: f32| {
            let (lo, hi) = ([36.0, 84.0, 52.0], [111.0, 217.0, 138.0]);
            let c = |i: usize| (lo[i] + (hi[i] - lo[i]) * t) / 255.0;
            Color::from_rgb(c(0), c(1), c(2))
        };
        for i in 0..BAR_SEGMENTS {
            let x = b.x + (h * SKEW / 2.0) + (b.width - w - h * SKEW) * i as f32 / (BAR_SEGMENTS - 1) as f32;
            let lit = match self.stage {
                BarStage::Running(_) => (i < pos && i + BAR_BLOCK >= pos).then(|| if i + 1 == pos { HEAD } else { lerp(i as f32 / BAR_SEGMENTS as f32) }),
                BarStage::Done => Some(if i + 1 == BAR_SEGMENTS { Color::from_rgb8(0xD9, 0xFF, 0xE2) } else { green(i as f32 / BAR_SEGMENTS as f32) }),
                BarStage::Waiting => None,
            };
            match lit {
                Some(color) => segment(&mut shapes, x, b.y, w, h, color),
                None => {
                    segment(&mut shapes, x, b.y, w, h, OFF_EDGE);
                    segment(&mut shapes, x + 1.0, b.y + 1.0, w - 2.0, h - 2.0, OFF);
                }
            }
        }
        tree.state.downcast_ref::<Painted>().draw(renderer, b.expand(8.0), shapes);
    }
}

/// A window turning into another: the frame moves between two rectangles while its content is a
/// mosaic blending the two pictures; outside the frame is [`KEY`], see-through on a keyed window.
#[derive(Clone)]
pub struct Morph<Message> {
    pub from: std::sync::Arc<Mosaic>,
    pub to: std::sync::Arc<Mosaic>,
    /// Frame rectangles relative to the window's top-left.
    pub from_rect: Rectangle,
    pub to_rect: Rectangle,
    pub start: Instant,
    pub timeline: MorphTimeline,
    /// Sent once each when their time (ms) has passed; the last one ends the morph.
    pub events: Vec<(f32, Message)>,
}
/// Times in ms: the frame moves over `move_ms`, pixels grow to `BLOCK_MAX` over `pixelate`,
/// hold, and sharpen over `sharpen`; `blend` crosses the pictures; `fade_in`/`fade_out` blend
/// the mosaic with the real window underneath at both ends.
#[derive(Clone, Copy)]
pub struct MorphTimeline {
    pub fade_in: f32,
    pub move_ms: (f32, f32),
    pub pixelate: f32,
    pub sharpen: (f32, f32),
    pub blend: (f32, f32),
    pub fade_out: (f32, f32),
}
impl MorphTimeline {
    /// The app shrinking into the update window.
    pub const SHRINK: Self = Self { fade_in: 120.0, move_ms: (120.0, 570.0), pixelate: 320.0, sharpen: (570.0, 920.0), blend: (320.0, 500.0), fade_out: (800.0, 920.0) };
    /// The update window growing into the app.
    pub const GROW: Self = Self { fade_in: 100.0, move_ms: (0.0, 500.0), pixelate: 300.0, sharpen: (520.0, 950.0), blend: (230.0, 430.0), fade_out: (820.0, 950.0) };
}
#[derive(Default)]
struct MorphState {
    fired: usize,
}
pub fn morph<'a, Message: Clone + 'a>(morph: Option<&Morph<Message>>) -> Element<'a, Message> {
    Element::new(MorphWidget { morph: morph.cloned() })
}
struct MorphWidget<Message> {
    morph: Option<Morph<Message>>,
}
/// Average colour of the part of `m` under the normalised rectangle `[u0, u1) × [v0, v1)`.
fn sample(m: &Mosaic, u0: f32, v0: f32, u1: f32, v1: f32) -> [f32; 3] {
    let x0 = ((u0 * m.width as f32) as usize).min(m.width - 1);
    let y0 = ((v0 * m.height as f32) as usize).min(m.height - 1);
    let x1 = ((u1 * m.width as f32).ceil() as usize).clamp(x0 + 1, m.width);
    let y1 = ((v1 * m.height as f32).ceil() as usize).clamp(y0 + 1, m.height);
    let mut sum = [0.0f32; 3];
    for y in y0..y1 {
        for x in x0..x1 {
            let c = m.cells[y * m.width + x];
            for i in 0..3 {
                sum[i] += c[i] as f32;
            }
        }
    }
    let n = ((x1 - x0) * (y1 - y0)) as f32;
    sum.map(|v| v / n)
}
impl<Message: Clone> Widget<Message, Theme, Renderer> for MorphWidget<Message> {
    fn tag(&self) -> tree::Tag { tree::Tag::of::<MorphState>() }
    fn state(&self) -> tree::State { tree::State::new(MorphState::default()) }
    fn size(&self) -> Size<Length> {
        Size { width: Length::Fill, height: Length::Fill }
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fill)
    }
    fn update(&mut self, tree: &mut Tree, event: &Event, _: Layout<'_>, _: mouse::Cursor, _: &Renderer, _: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, _: &Rectangle) {
        let (Event::Window(window::Event::RedrawRequested(now)), Some(m)) = (event, &self.morph) else { return };
        let state = tree.state.downcast_mut::<MorphState>();
        let ms = now.saturating_duration_since(m.start).as_secs_f32() * 1000.0;
        while let Some((at, message)) = m.events.get(state.fired) {
            if ms < *at {
                break;
            }
            shell.publish(message.clone());
            state.fired += 1;
        }
        if state.fired < m.events.len() {
            shell.request_redraw_at(RedrawRequest::NextFrame);
        }
    }
    fn draw(&self, _: &Tree, renderer: &mut Renderer, _: &Theme, _: &renderer::Style, layout: Layout<'_>, _: mouse::Cursor, _: &Rectangle) {
        let Some(m) = &self.morph else { return };
        let b = layout.bounds();
        let ms = Instant::now().saturating_duration_since(m.start).as_secs_f32() * 1000.0;
        let tl = m.timeline;
        let span = |a: f32, z: f32| ((ms - a) / (z - a)).clamp(0.0, 1.0);
        let ease = |x: f32| x * x * (3.0 - 2.0 * x);
        let k = ease(span(tl.move_ms.0, tl.move_ms.1));
        let block = if ms < tl.sharpen.0 {
            MOSAIC_CELL + (BLOCK_MAX - MOSAIC_CELL) * ease(span(0.0, tl.pixelate))
        } else {
            BLOCK_MAX - (BLOCK_MAX - MOSAIC_CELL) * ease(span(tl.sharpen.0, tl.sharpen.1))
        };
        let blend = ease(span(tl.blend.0, tl.blend.1));
        let alpha = span(0.0, tl.fade_in).min(1.0 - span(tl.fade_out.0, tl.fade_out.1));
        let lerp_rect = |a: Rectangle, z: Rectangle| Rectangle {
            x: b.x + a.x + (z.x - a.x) * k,
            y: b.y + a.y + (z.y - a.y) * k,
            width: a.width + (z.width - a.width) * k,
            height: a.height + (z.height - a.height) * k,
        };
        let frame = lerp_rect(m.from_rect, m.to_rect);
        let mut shapes = Vec::new();
        // Keyed surroundings are pixel-snapped quads, never anti-aliased: an edge blended with the
        // key rounds to a near-key colour Windows would not cut out, leaving hairlines.
        for (x, y, w, h) in [
            (b.x, b.y, b.width, frame.y - b.y),
            (b.x, frame.y + frame.height, b.width, b.y + b.height - frame.y - frame.height),
            (b.x, frame.y, frame.x - b.x, frame.height),
            (frame.x + frame.width, frame.y, b.x + b.width - frame.x - frame.width, frame.height),
        ] {
            if w > 0.01 && h > 0.01 {
                renderer.fill_quad(Quad { bounds: Rectangle { x, y, width: w, height: h }, snap: true, ..Quad::default() }, KEY);
            }
        }
        let side = block.round().max(MOSAIC_CELL);
        let cols = (frame.width / side).ceil().max(1.0) as usize;
        let rows = (frame.height / side).ceil().max(1.0) as usize;
        for row in 0..rows {
            let mut run: Option<(usize, [u8; 3])> = None;
            for col in 0..=cols {
                let color = (col < cols).then(|| {
                    let (u0, v0) = (col as f32 * side / frame.width, row as f32 * side / frame.height);
                    let (u1, v1) = (((col + 1) as f32 * side / frame.width).min(1.0), ((row + 1) as f32 * side / frame.height).min(1.0));
                    let a = sample(&m.from, u0, v0, u1, v1);
                    let z = sample(&m.to, u0, v0, u1, v1);
                    [0, 1, 2].map(|i| (a[i] + (z[i] - a[i]) * blend).round() as u8)
                });
                let same = |a: [u8; 3], z: [u8; 3]| a.iter().zip(z).all(|(x, y)| x.abs_diff(y) <= 3);
                match (run, color) {
                    (Some((_, c)), Some(next)) if same(c, next) => {}
                    (current, next) => {
                        if let Some((first, c)) = current {
                            let x = frame.x + first as f32 * side;
                            let y = frame.y + row as f32 * side;
                            let w = ((col - first) as f32 * side).min(frame.x + frame.width - x);
                            let h = side.min(frame.y + frame.height - y);
                            slant(&mut shapes, x, y, w + 0.6, h + 0.6, 0.0, Color { a: alpha, ..Color::from_rgb8(c[0], c[1], c[2]) });
                        }
                        run = next.map(|c| (col, c));
                    }
                }
            }
        }
        let cache = geometry(b, &shapes).cache(Group::unique(), None);
        renderer.draw_geometry(Geometry::load(&cache));
    }
}

impl<'a, Message: 'a> From<Tacho<'a, Message>> for Element<'a, Message> {
    fn from(t: Tacho<'a, Message>) -> Self {
        Element::new(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn t(range: RangeInclusive<f32>, v: f32) -> Tacho<'static, ()> {
        tacho(range, v, |_| (), Clock { epoch: Instant::now(), opened: None, animate: false })
    }
    #[test]
    fn red_zone_starts_strictly_above_the_threshold() {
        let s = t(0.0..=200.0, 100.0).segments(20).red_above(100.0);
        assert_eq!(s.lit(100.0), (0, 10, 9));
        assert!(!s.in_red(100.0));
        assert_eq!(s.lit(101.0), (0, 11, 10));
        assert!(s.in_red(101.0));
        assert_eq!(s.red_from(), 10);
        assert_eq!(s.lit(0.0), (0, 0, -1));
    }
    #[test]
    fn signed_fill_grows_from_the_origin() {
        let s = t(-12.0..=12.0, 0.0).segments(16).origin(0.0);
        assert_eq!(s.lit(0.0).0, s.lit(0.0).1);
        assert_eq!(s.lit(-3.0), (6, 8, 6));
        assert_eq!(s.lit(6.0), (8, 12, 11));
    }
    #[test]
    fn wave_lifts_and_settles() {
        assert_eq!(wave(0.0, false).0, 0.0);
        assert!(wave(284.0, false).0 < -9.0);
        assert_eq!(wave(WAVE_LEN + 1.0, true), (0.0, 0.0));
    }
}
