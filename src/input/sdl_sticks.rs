//! Sticks and triggers read on the axes an SDL mapping names, for a pad whose mapping puts
//! a stick on other axes than SIL reads it from. The axes are queried on the pad's event node
//! each frame and scaled as SIL scales its own: a stick to -1..1, a trigger from -1 at rest.

use crate::input::evdev::{Axis, EventNode};
use crate::input::sdl_map::{self, MappedAxis, AXES, STICK_AXES};

/// A stick axis must rest within this of its centre, scaled, for the mapping to take over.
const CENTRED: f32 = 0.5;
/// SIL reads a released trigger at one end of its stick axis.
pub const TRIGGER_REST: f32 = -1.0;

pub struct SdlSticks {
    node: EventNode,
    /// The mapped axes with their ranges and inversion, in AXIS_LABELS order.
    axes: [Option<(Axis, bool)>; AXES],
}

/// Value `v` of axis `a` scaled to -1 at its minimum and 1 at its maximum, negated when
/// `inverted`. The axis must span a range.
fn scaled(v: i32, a: &Axis, inverted: bool) -> f32 {
    let span = a.max as i64 - a.min as i64;
    let s = ((2 * (v as i64 - a.min as i64)) as f32 / span as f32 - 1.0).clamp(-1.0, 1.0);
    if inverted {
        -s
    } else {
        s
    }
}

/// Why the mapping's axes leave the sticks to SIL.
#[derive(Debug, PartialEq)]
pub enum Kept {
    /// The mapping's stick axes are SIL's.
    SameAxes,
    /// A stick axis rests off its centre: the mapping names a trigger or a missing axis.
    OffCentre(u16),
}

impl SdlSticks {
    /// Reads the sticks on `node` through mapped axes `mapped` when they move a stick off
    /// SIL's axes and every stick axis rests at its centre.
    pub fn take_over(node: EventNode, mapped: &[Option<MappedAxis>; AXES]) -> Result<SdlSticks, Kept> {
        if !sdl_map::moves_sticks(mapped) {
            return Err(Kept::SameAxes);
        }
        let ranged =
            |m: MappedAxis| node.axes.iter().find(|a| a.code == m.code && a.max > a.min).map(|&a| (a, m.inverted));
        let axes = mapped.map(|m| m.and_then(ranged));
        for (k, m) in mapped[..STICK_AXES].iter().enumerate() {
            let Some(m) = m else {
                continue;
            };
            let rest = axes[k].and_then(|(a, _)| node.value(a.code).map(|v| scaled(v, &a, false)));
            if !rest.is_some_and(|r| r.abs() <= CENTRED) {
                return Err(Kept::OffCentre(m.code));
            }
        }
        Ok(SdlSticks { node, axes })
    }

    /// The number of the event node read.
    pub fn event_node(&self) -> i32 {
        self.node.node
    }

    /// Both sticks as x and y, then the triggers when the mapping names one; an axis that
    /// does not read rests.
    pub fn read(&self) -> ([[f32; 2]; 2], Option<[f32; 2]>) {
        let v: [f32; AXES] = std::array::from_fn(|k| {
            let rest = if k < STICK_AXES { 0.0 } else { TRIGGER_REST };
            self.axes[k].map_or(rest, |(a, inv)| self.node.value(a.code).map_or(rest, |v| scaled(v, &a, inv)))
        });
        let triggers = self.axes[STICK_AXES..].iter().any(Option::is_some).then_some([v[4], v[5]]);
        ([[v[0], v[1]], [v[2], v[3]]], triggers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_axis_scales_from_its_range() {
        let stick = Axis { code: 2, min: -4096, max: 4096 };
        assert_eq!(scaled(0, &stick, false), 0.0);
        assert_eq!(scaled(-4096, &stick, false), -1.0);
        assert_eq!(scaled(2048, &stick, false), 0.5);
        assert_eq!(scaled(2048, &stick, true), -0.5);
        assert_eq!(scaled(5000, &stick, false), 1.0, "clamped past the range");
        let trigger = Axis { code: 10, min: 0, max: 255 };
        assert_eq!(scaled(0, &trigger, false), TRIGGER_REST);
        assert_eq!(scaled(255, &trigger, false), 1.0);
        let full = Axis { code: 0, min: i32::MIN, max: i32::MAX };
        assert_eq!(scaled(i32::MAX, &full, false), 1.0);
    }
}
