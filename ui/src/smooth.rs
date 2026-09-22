//! Smooth wheel scrolling for every `scrollable`. Iced jumps by 60 px per notch; here the
//! wheel only moves a target and the offset eases towards it on window frames, so a
//! long list glides and stops softly instead of stepping.
use iced::advanced::widget::{Id, Operation, operation};
use std::time::Instant;

/// Pixels per wheel notch: three clip rows.
pub const NOTCH: f32 = 96.0;
/// Time constant, independent of the monitor refresh rate and timer resolution.
const EASE_SECONDS: f32 = 0.040;

/// One scrollable being eased: where it is, where it should end up, and the last measured
/// viewport / content heights used to clamp the target.
#[derive(Clone, Copy, Debug)]
pub struct Anim {
    pub current: f32,
    pub target: f32,
    pub viewport: f32,
    pub content: f32,
    pub last_frame: Instant,
}
impl Anim {
    pub fn max(&self) -> f32 {
        (self.content - self.viewport).max(0.0)
    }
    pub fn push(&mut self, delta: f32) {
        self.target = (self.target + delta).clamp(0.0, self.max());
    }
    /// Advance one frame; returns false once settled on the target.
    pub fn step(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last_frame).as_secs_f32();
        self.last_frame = self.last_frame.max(now);
        self.target = self.target.clamp(0.0, self.max());
        let remaining = self.target - self.current;
        if remaining.abs() < 0.4 {
            self.current = self.target;
            return false;
        }
        self.current += remaining * (1.0 - (-elapsed / EASE_SECONDS).exp());
        true
    }
}

pub fn wheel_pixels(delta: iced::mouse::ScrollDelta) -> f32 {
    match delta {
        iced::mouse::ScrollDelta::Lines { y, .. } => -y * NOTCH,
        iced::mouse::ScrollDelta::Pixels { y, .. } => -y,
    }
}

/// Reads the vertical offset of one scrollable once when a wheel gesture starts:
/// (offset, viewport height, content height).
pub struct Probe {
    id: Id,
    result: Option<(f32, f32, f32)>,
}
impl Operation<Option<(f32, f32, f32)>> for Probe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<(f32, f32, f32)>>)) {
        operate(self);
    }
    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: iced::Rectangle,
        content_bounds: iced::Rectangle,
        translation: iced::Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        if id != Some(&self.id) {
            return;
        }
        let max = (content_bounds.height - bounds.height).max(0.0);
        let y = translation.y.clamp(0.0, max);
        self.result = Some((y, bounds.height, content_bounds.height));
    }
    fn finish(&self) -> operation::Outcome<Option<(f32, f32, f32)>> {
        operation::Outcome::Some(self.result)
    }
}
pub fn probe<M: Send + 'static>(
    id: &'static str,
    map: impl Fn(Option<(f32, f32, f32)>) -> M + Send + 'static,
) -> iced::Task<M> {
    iced::advanced::widget::operate(Probe {
        id: Id::new(id),
        result: None,
    })
    .map(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn eases_towards_a_clamped_target() {
        let mut anim = Anim {
            current: 0.0,
            target: 0.0,
            viewport: 400.0,
            content: 1000.0,
            last_frame: Instant::now(),
        };
        anim.push(wheel_pixels(iced::mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 }));
        assert_eq!(anim.target, NOTCH);
        anim.push(10_000.0);
        assert_eq!(anim.target, 600.0);
        anim.push(-10_000.0);
        assert_eq!(anim.target, 0.0);
        anim.target = 100.0;
        let mut frames = 0;
        while anim.step(anim.last_frame + Duration::from_millis(8)) {
            frames += 1;
            assert!(frames < 100, "never settles");
        }
        assert_eq!(anim.current, 100.0);
        assert!(frames > 5, "settled too abruptly: {frames} frames");
    }
    #[test]
    fn easing_depends_on_elapsed_time_not_frame_count() {
        let start = Instant::now();
        let mut fast = Anim { current: 0.0, target: 500.0, viewport: 400.0,
            content: 1000.0, last_frame: start };
        let mut slow = fast;
        for frame in 1..=12 { fast.step(start + Duration::from_millis(frame * 8)); }
        for frame in 1..=6 { slow.step(start + Duration::from_millis(frame * 16)); }
        assert!((fast.current - slow.current).abs() < 0.001);
        assert!(fast.current > 450.0);
        slow.content = 450.0;
        slow.step(start + Duration::from_secs(1));
        assert_eq!(slow.target, 50.0);
    }
}
