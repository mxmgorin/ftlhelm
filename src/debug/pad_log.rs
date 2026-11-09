//! Pad diagnostics in the log, so one log shows how a pad's buttons and sticks land. On a
//! pad's first sight its key codes and its event node (ids, axes with their ranges) are
//! logged. Then, once each, the first press of every raw button logs its key code and the
//! inputs it reaches, and the first push of every stick direction the node's axes moved
//! from their rest with it. A direction a stick already holds when its trace starts never
//! logs, so a trigger read as a stick resting at one end does not log at rest. The trace
//! stops polling once every button and direction has logged.

use crate::core::layer::Layer;
use crate::core::util::log;
use crate::game::api::api;
use crate::input::evdev::EventNode;
use crate::input::pad::{PadFrame, STICK_KEY_ON};
use crate::input::sdl_sticks::TRIGGER_REST;

/// The sticks traced: both sticks, then the triggers as one more.
const STICKS_TRACED: usize = 3;
/// Stick directions in bit order: x-, x+, y-, y+.
const DIRECTIONS: [&str; 4] = ["left", "right", "up", "down"];
const _: () = assert!(STICKS_TRACED * DIRECTIONS.len() <= u16::BITS as usize, "a u16 holds every direction");

/// What one pad has logged so far.
pub struct PadTrace {
    /// SIL pad and input device.
    pub key: (i32, i32),
    node: Option<EventNode>,
    /// Raw buttons with their key codes, and which have logged.
    buttons: Vec<(i32, u16)>,
    logged_buttons: Vec<bool>,
    /// The pad's sticks as last read, at most STICKS_TRACED.
    sticks: usize,
    /// Sticks sampled once, one bit each.
    started: u8,
    /// Stick directions logged, one bit each, four per stick.
    logged_dirs: u16,
}

impl PadTrace {
    /// Starts the trace of the pad at `key` with key codes `codes`, logging the codes and the
    /// pad's event node.
    pub fn new(key: (i32, i32), name: &str, codes: &[u16], buttons: Vec<(i32, u16)>) -> PadTrace {
        let dev = key.0;
        let codes: Vec<String> = codes.iter().map(u16::to_string).collect();
        log!("pad {dev}: keys {}", codes.join(" "));
        let node = EventNode::open(key.1, name);
        match &node {
            Some(n) => {
                let [bus, _, _, version] = n.id;
                let axes: Vec<String> = n.axes.iter().map(|a| format!("{} {}..{}", a.code, a.min, a.max)).collect();
                log!("pad {dev}: event{}, bus {bus:04x}, version {version:04x}, axes {}", n.node, axes.join(", "));
            }
            None => log!("pad {dev}: no readable event node"),
        }
        let logged_buttons = vec![false; buttons.len()];
        PadTrace { key, node, buttons, logged_buttons, sticks: 0, started: 0, logged_dirs: 0 }
    }

    /// Whether every button and every direction of the traced sticks has logged.
    fn done(&self) -> bool {
        let dirs = (1u32 << (self.sticks * DIRECTIONS.len())) - 1;
        self.logged_buttons.iter().all(|&b| b) && self.logged_dirs as u32 == dirs
    }
}

/// The directions of stick `stick` at `(x, y)` pushed for the first time, as indices into
/// DIRECTIONS; marks them logged, as it does the directions held at the stick's first sample.
fn newly_pushed(stick: usize, (x, y): (f32, f32), started: &mut u8, logged: &mut u16) -> Vec<usize> {
    let first = *started & 1 << stick == 0;
    *started |= 1 << stick;
    let mut out = Vec::new();
    for (d, v) in [-x, x, -y, y].into_iter().enumerate() {
        let bit = 1 << (stick * DIRECTIONS.len() + d);
        if v < STICK_KEY_ON || *logged & bit != 0 {
            continue;
        }
        *logged |= bit;
        if !first {
            out.push(d);
        }
    }
    out
}

impl Layer {
    /// Logs what pad `dev` has not logged yet: the first press of each raw button and the
    /// first push of each stick direction in its frame `p`.
    pub fn trace_pad(&mut self, dev: i32, p: &PadFrame) {
        let a = api();
        let Some(i) = self.pad.traces.iter().position(|t| t.key.0 == dev) else {
            return;
        };
        self.pad.traces[i].sticks = self.pad.stick_count(dev, p).min(STICKS_TRACED);
        let t = &self.pad.traces[i];
        if t.done() {
            return;
        }
        // SAFETY: the engine's button query on a connected pad.
        let down = |raw: i32| unsafe { (a.joy_button_state)(dev, raw) } != 0;
        let pressed: Vec<(usize, i32, u16)> = (t.buttons.iter().enumerate())
            .filter(|&(n, &(raw, _))| !t.logged_buttons[n] && down(raw))
            .map(|(n, &(raw, code))| (n, raw, code))
            .collect();
        for &(n, raw, code) in &pressed {
            let inputs: Vec<&str> = self.pad.inputs_of_raw(dev, raw).iter().map(|i| i.ini_name()).collect();
            let reached = if inputs.is_empty() { "none".to_string() } else { inputs.join(" ") };
            log!("pad {dev}: key {code} (raw {raw}) -> {reached}");
            self.pad.traces[i].logged_buttons[n] = true;
        }
        let read = [p.stick[0], p.stick[1], p.triggers.unwrap_or([TRIGGER_REST; 2])];
        for (stick, &[x, y]) in read.iter().enumerate().take(self.pad.traces[i].sticks) {
            let t = &mut self.pad.traces[i];
            for d in newly_pushed(stick, (x, y), &mut t.started, &mut t.logged_dirs) {
                let axes = match &t.node {
                    Some(n) => {
                        let moved: Vec<String> = n.moved().iter().map(|(c, v)| format!("{c}={v}")).collect();
                        format!("axes {}", if moved.is_empty() { "none".to_string() } else { moved.join(" ") })
                    }
                    None => "no event node".to_string(),
                };
                log!("pad {dev}: stick {stick} {}, {axes}", DIRECTIONS[d]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::layout::SIL_BUTTONS;
    use crate::game::fake::{set_pad, with_game, FakePad};
    use crate::input::sdl_map::BTN_JOYSTICK;

    const NONE: [usize; 0] = [];

    #[test]
    fn a_direction_logs_once_and_the_start_state_never() {
        let (mut started, mut logged) = (0, 0);
        assert_eq!(newly_pushed(0, (-1.0, 0.0), &mut started, &mut logged), NONE, "held at the start");
        assert_eq!(newly_pushed(0, (-0.9, 0.0), &mut started, &mut logged), NONE);
        assert_eq!(newly_pushed(0, (0.9, 0.0), &mut started, &mut logged), [1]);
        assert_eq!(newly_pushed(0, (0.9, 0.0), &mut started, &mut logged), NONE, "logged once");
        assert_eq!(newly_pushed(0, (0.0, 0.8), &mut started, &mut logged), [3]);
    }

    #[test]
    fn a_trigger_resting_at_one_end_logs_its_pull_and_not_its_release() {
        let (mut started, mut logged) = (0, 0);
        assert_eq!(newly_pushed(2, (-1.0, -1.0), &mut started, &mut logged), NONE);
        assert_eq!(newly_pushed(2, (0.0, -1.0), &mut started, &mut logged), NONE);
        assert_eq!(newly_pushed(2, (1.0, -1.0), &mut started, &mut logged), [1]);
        assert_eq!(newly_pushed(2, (-1.0, -1.0), &mut started, &mut logged), NONE, "back at rest");
        assert_eq!(logged, 0b0111 << (2 * DIRECTIONS.len()));
    }

    #[test]
    fn a_raw_button_logs_its_first_press_once() {
        with_game(|_| {
            const RAW: i32 = 100;
            let mapping: Vec<i32> = (0..SIL_BUTTONS as i32).map(|b| b + RAW).collect();
            let keys: Vec<(u16, i32)> =
                mapping.iter().enumerate().map(|(b, &raw)| (BTN_JOYSTICK + b as u16, raw)).collect();
            let pad = |held: Vec<i32>| FakePad {
                index: Some(0),
                mapping: mapping.clone(),
                keys: keys.clone(),
                held,
                ..FakePad::default()
            };
            let mut l = Layer::default();
            set_pad(pad(vec![]));
            l.poll_pad(0.0);
            let logged = |l: &Layer| l.pad.traces[0].logged_buttons.iter().filter(|&&b| b).count();
            assert_eq!(l.pad.traces[0].buttons.len(), SIL_BUTTONS);
            assert_eq!(logged(&l), 0);
            set_pad(pad(vec![RAW]));
            l.poll_pad(0.0);
            l.poll_pad(0.0);
            assert_eq!(logged(&l), 1, "a held button logs once");
            set_pad(pad(mapping.clone()));
            l.poll_pad(0.0);
            assert_eq!(logged(&l), SIL_BUTTONS);
            assert!(!l.pad.traces[0].done(), "the sticks have not logged");
        });
    }
}
