//! The options screen's volume sliders, stepped from the pad. The game takes both volumes
//! from the sliders' markers every frame while the screen is open.

use super::mem::Obj;
use super::offsets::*;

/// How many even steps lead from silence to full volume.
pub const VOLUME_STEPS: i32 = 20;

/// The volume slider of `options` at index `id` among OPTIONS_SLIDERS.
pub fn slider(options: Obj, id: i32) -> Option<Obj> {
    let off = *OPTIONS_SLIDERS.get(usize::try_from(id).ok()?)?;
    Some(options.at(off))
}

/// Moves a slider's marker to the next even step past it toward the sign of `dir`; returns
/// false when no step lies that way.
pub fn step(slider: Obj, dir: i32) -> bool {
    let (min, max) = (slider.i32(SLIDER_MIN), slider.i32(SLIDER_MAX));
    let x = slider.i32(SLIDER_MARKER_X);
    let at = |k: i32| min + ((max - min) as f32 * k as f32 / VOLUME_STEPS as f32).round() as i32;
    let mut steps = (0..=VOLUME_STEPS).map(at);
    let next = if dir > 0 { steps.find(|&s| s > x) } else { steps.rfind(|&s| s < x) };
    let Some(next) = next else {
        return false;
    };
    slider.set(SLIDER_MARKER_X, next);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fake::Arena;

    const MIN: i32 = 400;
    const MAX: i32 = 607;

    fn slider(arena: &mut Arena, x: i32) -> Obj {
        let s = arena.alloc(SLIDER_MAX + size_of::<i32>());
        s.set(SLIDER_MIN, MIN);
        s.set(SLIDER_MAX, MAX);
        s.set(SLIDER_MARKER_X, x);
        s
    }

    #[test]
    fn steps_walk_every_even_step_from_end_to_end() {
        let mut arena = Arena::default();
        let s = slider(&mut arena, MIN);
        let mut seen = vec![MIN];
        while step(s, 1) {
            seen.push(s.i32(SLIDER_MARKER_X));
        }
        assert_eq!(seen.len() as i32, VOLUME_STEPS + 1);
        assert_eq!(seen.last(), Some(&MAX));
        assert!(!step(s, 1), "nothing past full volume");
        while step(s, -1) {}
        assert_eq!(s.i32(SLIDER_MARKER_X), MIN);
        assert!(!step(s, -1), "nothing below silence");
    }

    #[test]
    fn a_marker_between_steps_goes_to_the_nearest_step_that_way() {
        let mut arena = Arena::default();
        let s = slider(&mut arena, MIN + 25);
        assert!(step(s, 1));
        assert_eq!(s.i32(SLIDER_MARKER_X), MIN + 31);
        s.set(SLIDER_MARKER_X, MIN + 25);
        assert!(step(s, -1));
        assert_eq!(s.i32(SLIDER_MARKER_X), MIN + 21);
    }
}
