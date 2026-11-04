//! Touch as the mouse: the game's touch events turned into pointer moves and clicks.
//!
//! The engine reports touches to the CApp's OnTouch handlers, which the PC build leaves
//! empty. Hooks on them queue each event; once a frame the layer runs the queue through
//! `Gestures`, a state machine of one finger, and applies the acts it hands out. A tap clicks
//! where the finger landed, a finger held past the long press right-clicks there when it
//! lifts, and a finger moved past the slop drags with the left button from there; a finger
//! that goes down takes over, unless it joins the first for a two-finger tap, which pauses
//! the game as the pause button does. A finger moved up or down from a system icon or an
//! armament box steps its power instead, a bar per step: up adds, down removes; an armament
//! box's item only switches on or off. Events arrive
//! in window pixels, where the slop and the step are measured; a gesture acts at the frame
//! point the screen shows under the finger, through the zoomed view.
//!
//! The game resolves what lies under the pointer a frame after a move, so a press waits for
//! the frame after the move before it. A finger that lands on nothing the game takes moves,
//! once the game has flagged the point, to the nearest target within reach: a zone or a HUD
//! box. A finger that drags or pairs before the flag stays where it landed; a finger landing
//! while the game looks waits for the flag.

use std::collections::VecDeque;

use crate::core::layer::Layer;
use crate::core::util::{log, now_s};
use crate::game::actions::{toggle_pause, MouseButton};
use crate::game::api::{api, Rect};
use crate::game::state;
use crate::game::zones::{hud_boxes, HudBox, ZoneKind};
use crate::ui::draw::{draw_ring_timer, RING_LIT_COLOR, RING_READY_COLOR, RING_SEGMENTS};

/// Events kept for the next frame; past this many, further moves are dropped.
const MAX_QUEUED_EVENTS: usize = 64;
/// Frame pixels a snapped point keeps inside its target's edge, at most half the target.
const SNAP_INSET: f32 = 2.0;
/// Seconds within which a second finger joins the first for a two-finger tap; both lift
/// before the long press for it.
const PAIR_JOIN_S: f64 = 0.2;
/// Frames the game takes to flag what lies under a moved pointer.
const PROBE_FRAMES: u32 = 2;
/// Seconds after a touch event within which a mouse move is its echo: a compositor putting
/// its own pointer back over the window, not a mouse in use.
const TOUCH_ECHO_S: f64 = 0.25;
/// The ring timer around a finger held toward a right click, in window pixels: wider than a
/// fingertip.
const HOLD_RING_R: f32 = 52.0;
const HOLD_RING_W: f32 = 8.0;
/// The share of the long press a finger stays down before its ring shows: a tap shows none.
const HOLD_RING_FROM: f32 = 0.25;
/// The ring left where a touch clicked, in window pixels, and the seconds it takes to fade.
const MARK_R: f32 = 16.0;
const MARK_W: f32 = 4.0;
const MARK_S: f64 = 0.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Down,
    Move,
    Up,
    Cancel,
}

impl TouchPhase {
    const ALL: [TouchPhase; 4] = [TouchPhase::Down, TouchPhase::Move, TouchPhase::Up, TouchPhase::Cancel];

    pub fn name(self) -> &'static str {
        match self {
            TouchPhase::Down => "down",
            TouchPhase::Move => "move",
            TouchPhase::Up => "up",
            TouchPhase::Cancel => "cancel",
        }
    }

    pub fn from_name(name: &str) -> Option<TouchPhase> {
        TouchPhase::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// One touch event as the engine reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchEvent {
    pub phase: TouchPhase,
    /// The finger, as the engine numbers it.
    pub id: i32,
    /// Window pixels.
    pub x: i32,
    pub y: i32,
    /// Seconds since load.
    pub t: f64,
}

/// What a gesture does to the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    /// The pointer to a frame point, the left button as it is.
    Move(f32, f32),
    /// A click; the layer picks the button.
    Tap,
    /// The finger held long enough to right-click when it lifts.
    Hold,
    /// A right click.
    LongPress,
    /// The left button down.
    Press,
    /// The left button up.
    Release,
    /// The game's pause, on or off.
    Pause,
    /// A bar of power added (1) or removed (-1) where the pointer is.
    Step(i32),
}

/// The settings the gestures take.
#[derive(Clone, Copy)]
pub struct Limits {
    /// Seconds a finger stays down before it long-presses.
    pub long_press: f64,
    /// Window pixels a finger may drift and still tap.
    pub slop: f32,
    /// Window pixels a finger on a power box travels per bar; 0: it drags instead.
    pub step: f32,
}

/// What a finger landed on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Landing {
    /// Anything else.
    Open,
    /// The overlay took the touch.
    Overlay,
    /// A system icon or an armament box: an up or down move steps its power.
    Power(PowerBox),
}

/// A box an up or down move steps the power of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerBox {
    /// A system icon: its power runs in bars.
    Bars,
    /// An armament box: its item is on or off, one bar either way from where the finger landed.
    Switch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Down, still a tap or a long press.
    Pending,
    /// Held past the long press: a right click when it lifts, a drag when it moves.
    Held,
    /// Moved past the slop; the left button is down.
    Drag,
    /// Moved up or down past the slop from a power box: the bars stepped so far, up positive.
    Swipe { stepped: i32 },
    /// Joined by a second finger: a pause if both lift before the long press, within the slop.
    Pair {
        /// The second finger and where it went down, in window pixels.
        partner: i32,
        start: (i32, i32),
        /// The one of the two that lifted, if one has.
        lifted: Option<i32>,
    },
    /// Taken by the overlay; the rest of the touch does nothing.
    Spent,
}

#[derive(Clone, Copy)]
struct Finger {
    id: i32,
    /// Where it went down, in window pixels, and when.
    start: (i32, i32),
    since: f64,
    /// The frame point it acts on.
    at: (f32, f32),
    stage: Stage,
    /// The power box it landed on.
    power: Option<PowerBox>,
}

/// One finger's gestures, and the acts waiting for their frame.
#[derive(Default)]
pub struct Gestures {
    finger: Option<Finger>,
    due: VecDeque<Act>,
}

impl Gestures {
    /// Whether `id` is the finger down or its partner.
    fn follows(&self, id: i32) -> bool {
        self.finger.is_some_and(|f| f.id == id || matches!(f.stage, Stage::Pair { partner, .. } if partner == id))
    }

    /// Forgets a power box the pointer did not reach: the steps waiting are dropped, a swipe
    /// does nothing more, and a finger yet to move moves as from anywhere else.
    pub fn unpower(&mut self) {
        self.due.retain(|a| !matches!(a, Act::Step(_)));
        if let Some(f) = self.finger.as_mut() {
            f.power = None;
            if matches!(f.stage, Stage::Swipe { .. }) {
                f.stage = Stage::Spent;
            }
        }
    }

    /// Whether the finger down left the point it landed on: a drag, or a pair.
    pub fn moved_on(&self) -> bool {
        self.finger.is_some_and(|f| matches!(f.stage, Stage::Drag | Stage::Pair { .. }))
    }

    /// Moves the pointer to frame point `at` ahead of the acts waiting; the finger down, if
    /// any, acts there from now on.
    pub fn retarget(&mut self, at: (f32, f32)) {
        if let Some(f) = self.finger.as_mut() {
            f.at = at;
        }
        self.due.push_front(Act::Move(at.0, at.1));
    }

    /// The frame point a finger still tapping or holding acts on, and the share of the long
    /// press it has stayed down at `now`, up to 1.
    pub fn hold(&self, now: f64, limits: Limits) -> Option<((f32, f32), f32)> {
        let f = self.finger.filter(|f| matches!(f.stage, Stage::Pending | Stage::Held))?;
        let share = if f.stage == Stage::Held { 1.0 } else { ((now - f.since) / limits.long_press).min(1.0) as f32 };
        Some((f.at, share))
    }

    /// Ends the touch of the finger down, releasing the left button a drag holds.
    pub fn lift(&mut self) {
        if self.finger.take().is_some_and(|f| f.stage == Stage::Drag) {
            self.due.push_back(Act::Release);
        }
    }

    /// Takes one event acting at frame point `at`; `landing` tells what a down landed on.
    pub fn event(&mut self, ev: &TouchEvent, (x, y): (f32, f32), landing: Landing, limits: Limits) {
        match ev.phase {
            TouchPhase::Down => {
                let taken = landing == Landing::Overlay;
                let joins = |f: &Finger| f.stage == Stage::Pending && f.id != ev.id && ev.t - f.since <= PAIR_JOIN_S;
                if let Some(f) = self.finger.as_mut().filter(|f| !taken && joins(f)) {
                    f.stage = Stage::Pair { partner: ev.id, start: (ev.x, ev.y), lifted: None };
                    return;
                }
                // The newest finger takes over, so a lost up leaves no finger behind.
                self.lift();
                let stage = if taken { Stage::Spent } else { Stage::Pending };
                let power = match landing {
                    Landing::Power(power) => Some(power),
                    Landing::Open | Landing::Overlay => None,
                };
                let (start, since) = ((ev.x, ev.y), ev.t);
                self.finger = Some(Finger { id: ev.id, start, since, at: (x, y), stage, power });
                if !taken {
                    self.due.push_back(Act::Move(x, y));
                }
            }
            _ if !self.follows(ev.id) => {}
            TouchPhase::Move => {
                let Some(f) = self.finger.as_mut() else {
                    return;
                };
                let start = match f.stage {
                    Stage::Pair { partner, start, .. } if partner == ev.id => start,
                    _ => f.start,
                };
                let drift = ((ev.x - start.0) as f32).hypot((ev.y - start.1) as f32);
                match f.stage {
                    Stage::Pair { .. } if drift > limits.slop => f.stage = Stage::Spent,
                    Stage::Pending | Stage::Held if drift > limits.slop => {
                        let upright = (ev.y - start.1).abs() >= (ev.x - start.0).abs();
                        if f.power.is_some() && limits.step > 0.0 && upright {
                            f.stage = Stage::Swipe { stepped: 0 };
                        } else {
                            f.stage = Stage::Drag;
                            self.due.extend([Act::Press, Act::Move(x, y)]);
                        }
                    }
                    Stage::Drag => self.due.push_back(Act::Move(x, y)),
                    _ => {}
                }
                if let Stage::Swipe { stepped } = f.stage {
                    let steps = ((start.1 - ev.y) as f32 / limits.step).trunc() as i32;
                    let steps = if f.power == Some(PowerBox::Switch) { steps.clamp(-1, 1) } else { steps };
                    let dir = (steps - stepped).signum();
                    self.due.extend(std::iter::repeat_n(Act::Step(dir), (steps - stepped).unsigned_abs() as usize));
                    f.stage = Stage::Swipe { stepped: steps };
                }
            }
            TouchPhase::Up => {
                let Some(mut f) = self.finger.take() else {
                    return;
                };
                match f.stage {
                    // A repeated up of the finger that lifted.
                    Stage::Pair { lifted: Some(id), .. } if id == ev.id => self.finger = Some(f),
                    Stage::Pair { partner, start, lifted: None } => {
                        f.stage = Stage::Pair { partner, start, lifted: Some(ev.id) };
                        self.finger = Some(f);
                    }
                    Stage::Pair { .. } if ev.t - f.since < limits.long_press => self.due.push_back(Act::Pause),
                    Stage::Pending if ev.t - f.since < limits.long_press => self.due.push_back(Act::Tap),
                    Stage::Pending | Stage::Held => self.due.push_back(Act::LongPress),
                    Stage::Drag => self.due.extend([Act::Move(x, y), Act::Release]),
                    Stage::Swipe { .. } | Stage::Pair { .. } | Stage::Spent => {}
                }
            }
            TouchPhase::Cancel => self.lift(),
        }
    }

    /// Marks a finger held past the long press at `now`; returns the acts due this frame:
    /// those waiting, up to a press after a move handed out with them.
    pub fn frame(&mut self, now: f64, limits: Limits) -> Vec<Act> {
        if let Some(f) = self.finger.as_mut() {
            if f.stage == Stage::Pending && now - f.since >= limits.long_press {
                f.stage = Stage::Held;
                self.due.push_back(Act::Hold);
            }
        }
        let mut out = Vec::new();
        let mut moved = false;
        while let Some(&act) = self.due.front() {
            match act {
                Act::Move(..) => moved = true,
                Act::Tap | Act::LongPress | Act::Press | Act::Step(_) if moved => break,
                _ => {}
            }
            out.push(act);
            self.due.pop_front();
        }
        out
    }
}

/// The point a touch at frame point `at` acts on: `at` when a target holds it, else the
/// nearest point inside the nearest target within `reach` frame pixels, else `at`.
fn snap(targets: &[Rect], (x, y): (f32, f32), reach: f32) -> (f32, f32) {
    if targets.iter().any(|r| r.contains(x.floor() as i32, y.floor() as i32)) {
        return (x, y);
    }
    let nearest = |v: f32, lo: i32, len: i32| {
        let inset = SNAP_INSET.min((len - 1) as f32 / 2.0);
        v.clamp(lo as f32 + inset, (lo + len - 1) as f32 - inset)
    };
    targets
        .iter()
        .filter(|r| r.w > 0 && r.h > 0)
        .map(|r| (nearest(x, r.x, r.w), nearest(y, r.y, r.h)))
        .map(|p| (p, (p.0 - x).hypot(p.1 - y)))
        .filter(|&(_, d)| d <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or((x, y), |(p, _)| p)
}

/// A finger landed beside a target: it moves there unless the game takes the point it
/// landed on. The acts wait while the game looks.
#[derive(Clone, Copy)]
struct Probe {
    landed: (f32, f32),
    target: (f32, f32),
    /// Frames left until the game's flag tells, from the pointer's move to `landed`.
    settle: Option<u32>,
}

/// Where a touch clicked, and when; a right click shows ready.
#[derive(Clone, Copy)]
struct Mark {
    at: (f32, f32),
    since: f64,
    right: bool,
}

/// Touch events queued for the next frame, and the gestures.
#[derive(Default)]
pub struct Touch {
    /// All four handlers are hooked; events count only then.
    pub hooked: bool,
    queued: Vec<TouchEvent>,
    gestures: Gestures,
    probe: Option<Probe>,
    mark: Option<Mark>,
    /// A touch came last, no pad or mouse since: the input in use.
    in_use: bool,
    /// A touch event came this run.
    seen: bool,
    /// When the last touch event came.
    last_event: Option<f64>,
}

/// Whether the engine found a touchscreen and so takes touches as touches.
pub fn touchscreen_present() -> bool {
    // SAFETY: the engine's flag, set once the display is open.
    unsafe { (api().touchscreen_present as *const u8).read() != 0 }
}

/// Logs whether the engine found a touchscreen.
pub fn report() {
    log!("touch: {}", if touchscreen_present() { "touchscreen found" } else { "no touchscreen found" });
}

impl Layer {
    /// Queues a touch event at window pixel (`x`, `y`) for the next frame, unless touch is
    /// off. Logs the run's first.
    pub fn queue_touch(&mut self, phase: TouchPhase, id: i32, x: i32, y: i32) {
        if !self.touch.hooked || !self.cfg.touch {
            return;
        }
        if !self.touch.seen {
            self.touch.seen = true;
            log!("touch: first event");
        }
        let t = now_s();
        self.touch.last_event = Some(t);
        if phase != TouchPhase::Move || self.touch.queued.len() < MAX_QUEUED_EVENTS {
            self.touch.queued.push(TouchEvent { phase, id, x, y, t });
        }
    }

    /// Whether a mouse move now is a touch's echo: a touch event came within TOUCH_ECHO_S.
    pub fn touch_echo(&self) -> bool {
        self.touch.last_event.is_some_and(|t| now_s() - t < TOUCH_ECHO_S)
    }

    fn touch_limits(&self) -> Limits {
        Limits { long_press: self.cfg.touch_long_press as f64, slop: self.cfg.touch_slop, step: self.cfg.touch_step }
    }

    /// Runs the queued touch events through the gestures and applies the acts due at `now`.
    pub fn apply_touch(&mut self, now: f64) {
        let limits = self.touch_limits();
        if !self.cfg.touch {
            self.touch.gestures.lift();
            self.touch.probe = None;
        }
        if let Some(p) = self.touch.probe.as_mut() {
            p.settle = p.settle.map(|n| n.saturating_sub(1));
        }
        if let Some(p) = self.touch.probe.filter(|p| p.settle == Some(0)) {
            self.touch.probe = None;
            let taken = state::pointer_valid();
            if self.debug {
                log!("touch: {}", if taken { "the game takes the point" } else { "to the nearest target" });
            }
            if taken {
                self.touch.gestures.unpower();
            } else {
                self.touch.gestures.retarget(p.target);
            }
        }
        let mut queued = std::mem::take(&mut self.touch.queued);
        let mut next = 0;
        while let Some(&ev) = queued.get(next) {
            // A new finger waits for the probe, so the acts before it land where it resolves.
            if ev.phase == TouchPhase::Down && self.probe_settling() {
                break;
            }
            next += 1;
            let at = self.frame_at(ev.x, ev.y);
            if self.debug {
                log!("touch {} {} at {},{}: frame {:.0},{:.0}", ev.phase.name(), ev.id, ev.x, ev.y, at.0, at.1);
            }
            let mut landing = Landing::Open;
            if ev.phase == TouchPhase::Down {
                let taken = self.overlay_takes_touch(at);
                self.touch.in_use |= !taken;
                let hud = if state::ship_view() { hud_boxes() } else { Vec::new() };
                let target = if taken { at } else { self.snap_touch(at, &hud) };
                self.touch.probe = (target != at).then_some(Probe { landed: at, target, settle: None });
                let under = hud.iter().find(|b| b.rect.contains(target.0.floor() as i32, target.1.floor() as i32));
                landing = match (taken, under) {
                    (true, _) => Landing::Overlay,
                    (false, Some(b)) => {
                        Landing::Power(if b.powered.is_some() { PowerBox::Switch } else { PowerBox::Bars })
                    }
                    (false, None) => Landing::Open,
                };
            }
            self.touch.gestures.event(&ev, at, landing, limits);
            if self.touch.gestures.moved_on() {
                self.touch.probe = None;
            }
        }
        queued.drain(..next);
        queued.append(&mut self.touch.queued);
        self.touch.queued = queued;
        if self.probe_settling() {
            return;
        }
        for act in self.touch.gestures.frame(now, limits) {
            self.apply_act(act, now);
        }
    }

    /// Whether a probe waits for the game's flag; the acts wait with it.
    fn probe_settling(&self) -> bool {
        self.touch.probe.is_some_and(|p| p.settle.is_some())
    }

    /// The point a finger landing at frame point `at` would move to: the nearest target within
    /// `touch_snap`, the zones and the HUD boxes `hud`.
    fn snap_touch(&self, at: (f32, f32), hud: &[HudBox]) -> (f32, f32) {
        let reach = self.cfg.touch_snap * self.frame_per_window();
        if reach <= 0.0 {
            return at;
        }
        let targets: Vec<Rect> = self.zones.list.iter().map(|z| z.rect).chain(hud.iter().map(|b| b.rect)).collect();
        snap(&targets, at, reach)
    }

    /// Whether touch is on and the input in use.
    pub fn touch_in_use(&self) -> bool {
        self.cfg.touch && self.touch.in_use
    }

    /// Ends touch's use, as another input takes over.
    pub fn leave_touch(&mut self) {
        self.touch.in_use = false;
    }

    /// Draws a ring timer around a finger held toward a right click: its segments light up as
    /// the long press nears, and all turn white once it is reached. A touch's click leaves a
    /// small ring that fades.
    pub fn draw_touch(&self) {
        if !self.cfg.touch_marks {
            return;
        }
        let now = now_s();
        let k = self.frame_per_window();
        if let Some(m) = self.touch.mark.filter(|m| now - m.since < MARK_S) {
            let color = if m.right { RING_READY_COLOR } else { RING_LIT_COLOR };
            let opacity = (1.0 - (now - m.since) / MARK_S) as f32;
            draw_ring_timer(m.at.0, m.at.1, MARK_R * k, MARK_W * k, RING_SEGMENTS, color, opacity);
        }
        let Some(((x, y), share)) = self.touch.gestures.hold(now, self.touch_limits()) else {
            return;
        };
        if share <= HOLD_RING_FROM {
            return;
        }
        let lit = ((share - HOLD_RING_FROM) / (1.0 - HOLD_RING_FROM) * RING_SEGMENTS as f32).ceil() as usize;
        let color = if share >= 1.0 { RING_READY_COLOR } else { RING_LIT_COLOR };
        draw_ring_timer(x, y, HOLD_RING_R * k, HOLD_RING_W * k, lit, color, 1.0);
    }

    /// Lets the help, a notice, an open wheel or the pause button take a touch landing at
    /// frame point `at`: the help or the notice closes, a wheel acts on the plate there, the
    /// button pauses or resumes. Returns whether one took it.
    fn overlay_takes_touch(&mut self, (x, y): (f32, f32)) -> bool {
        if self.help.shown {
            self.touch_help((x, y));
        } else if self.notice.is_some() && self.notice_shown() {
            self.notice = None;
        } else if self.wheel.open.is_some() {
            self.touch_wheel(x, y);
        } else if self.pause_button_hit((x, y)) {
            toggle_pause();
        } else {
            return false;
        }
        if self.debug {
            log!("touch: taken by the overlay");
        }
        true
    }

    fn apply_act(&mut self, act: Act, now: f64) {
        if self.debug && act != Act::Tap {
            log!("touch: {act:?}");
        }
        match act {
            Act::Move(x, y) => {
                (self.cursor.x, self.cursor.y) = (x, y);
                self.send_cursor();
                if let Some(p) = self.touch.probe.as_mut().filter(|p| p.landed == (x, y)) {
                    p.settle = Some(PROBE_FRAMES);
                }
            }
            Act::Tap => {
                let button = self.tap_button();
                if self.debug {
                    log!("touch: Tap, {button:?} button");
                }
                self.click_tap(button);
                self.mark_click(false, now);
            }
            Act::Hold => self.buzz(),
            Act::LongPress => {
                self.click_tap(MouseButton::Right);
                self.mark_click(true, now);
            }
            Act::Press => self.click(MouseButton::Left, true),
            Act::Release => self.click(MouseButton::Left, false),
            Act::Pause => toggle_pause(),
            Act::Step(dir) => self.step_power(dir, now),
        }
    }

    /// Adds a bar of power to the box under the pointer for a positive `dir`, else takes one
    /// away, by a left or right click; none on an armament box it would not switch, whose
    /// powered item a left click arms.
    fn step_power(&mut self, dir: i32, now: f64) {
        let (x, y) = (self.cursor.last_x, self.cursor.last_y);
        let powered = hud_boxes().into_iter().find(|b| b.rect.contains(x, y)).and_then(|b| b.powered);
        if powered == Some(dir > 0) {
            return;
        }
        self.click_tap(if dir > 0 { MouseButton::Left } else { MouseButton::Right });
        self.mark_click(dir < 0, now);
    }

    /// Leaves a mark where the pointer is at `now`, for a touch's click.
    fn mark_click(&mut self, right: bool, now: f64) {
        self.touch.mark = Some(Mark { at: (self.cursor.x, self.cursor.y), since: now, right });
    }

    /// The button a tap clicks: the right one, which orders the selected crew, over a room
    /// with no crew member under the pointer; else the left.
    fn tap_button(&self) -> MouseButton {
        let room = self
            .zone_under_cursor()
            .is_some_and(|i| matches!(self.zones.list[i].kind, ZoneKind::Room | ZoneKind::EnemyRoom));
        if room && state::any_crew_selected() && !state::aiming() && !state::crew_under_pointer() {
            MouseButton::Right
        } else {
            MouseButton::Left
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::layout::Input;
    use crate::game::api::Point;
    use crate::game::fake::{calls, with_game, Call, WEAPON_SIZE};
    use crate::game::offsets::{
        ARMAMENTBOX_HIT, ARMAMENTBOX_ITEM, ARMAMENT_BOXES, COMBAT_WEAPON_CONTROL, CREW_POTENTIAL, CREW_SELECTED,
        GAME_H, GAME_W, GUI_COMBAT_CONTROL, GUI_CREW_CONTROL, GUI_SYS_CONTROL, KEY_SPACE, MOUSE_VALID, PLAYER_SHIP_ID,
        SYSBOX_LOCATION, SYSBOX_SYSTEM, SYSCTRL_BOXES, SYSCTRL_POSITION, WEAPON_POWERED,
    };
    use crate::game::zones::Zone;
    use crate::input::pad::PadFrame;
    use crate::ui::overlay::{Notice, PAD_LOST_NOTICE};
    use crate::ui::zoom::View;

    const LIMITS: Limits = Limits { long_press: 0.5, slop: 10.0, step: 10.0 };
    /// A frame step at the game's frame rate.
    const DT: f64 = 1.0 / 60.0;

    fn ev(phase: TouchPhase, id: i32, (x, y): (i32, i32), t: f64) -> TouchEvent {
        TouchEvent { phase, id, x, y, t }
    }

    /// Feeds `g` an event acting at its own window point.
    fn feed(g: &mut Gestures, phase: TouchPhase, id: i32, at: (i32, i32), t: f64) {
        g.event(&ev(phase, id, at, t), (at.0 as f32, at.1 as f32), Landing::Open, LIMITS);
    }

    #[test]
    fn a_tap_moves_the_pointer_then_clicks_there() {
        let mut g = Gestures::default();
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.0);
        assert_eq!(g.frame(0.0, LIMITS), [Act::Move(100.0, 200.0)]);
        feed(&mut g, TouchPhase::Move, 0, (105, 205), 0.05);
        feed(&mut g, TouchPhase::Up, 0, (105, 205), 0.1);
        assert_eq!(g.frame(0.1, LIMITS), [Act::Tap]);
        assert!(g.frame(0.2, LIMITS).is_empty());
    }

    #[test]
    fn a_tap_within_one_frame_clicks_on_the_next() {
        let mut g = Gestures::default();
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.0);
        feed(&mut g, TouchPhase::Up, 0, (100, 200), 0.01);
        assert_eq!(g.frame(0.01, LIMITS), [Act::Move(100.0, 200.0)]);
        assert_eq!(g.frame(0.01 + DT, LIMITS), [Act::Tap]);
    }

    #[test]
    fn a_long_press_marks_the_hold_and_right_clicks_on_release() {
        let mut g = Gestures::default();
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.0);
        assert_eq!(g.frame(0.0, LIMITS), [Act::Move(100.0, 200.0)]);
        assert!(g.frame(0.4, LIMITS).is_empty());
        assert_eq!(g.hold(0.4, LIMITS), Some(((100.0, 200.0), 0.8)));
        assert_eq!(g.frame(0.5, LIMITS), [Act::Hold]);
        assert_eq!(g.hold(0.6, LIMITS), Some(((100.0, 200.0), 1.0)));
        assert!(g.frame(0.6, LIMITS).is_empty());
        feed(&mut g, TouchPhase::Up, 0, (104, 200), 0.7);
        assert_eq!(g.frame(0.7, LIMITS), [Act::LongPress]);
        assert_eq!(g.hold(0.7, LIMITS), None);
        // An up past the long press before a frame saw it still long-presses.
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 1.0);
        g.frame(1.0, LIMITS);
        feed(&mut g, TouchPhase::Up, 0, (100, 200), 1.6);
        assert_eq!(g.frame(1.6, LIMITS), [Act::LongPress]);
    }

    #[test]
    fn a_held_finger_that_moves_drags_instead() {
        let mut g = Gestures::default();
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.0);
        g.frame(0.0, LIMITS);
        assert_eq!(g.frame(0.6, LIMITS), [Act::Hold]);
        feed(&mut g, TouchPhase::Move, 0, (150, 200), 0.7);
        assert_eq!(g.frame(0.7, LIMITS), [Act::Press, Act::Move(150.0, 200.0)]);
        assert_eq!(g.hold(0.7, LIMITS), None, "a drag shows no ring");
        feed(&mut g, TouchPhase::Up, 0, (160, 200), 0.8);
        assert_eq!(g.frame(0.8, LIMITS), [Act::Move(160.0, 200.0), Act::Release]);
    }

    #[test]
    fn a_finger_past_the_slop_drags_with_the_left_button() {
        let mut g = Gestures::default();
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.0);
        g.frame(0.0, LIMITS);
        feed(&mut g, TouchPhase::Move, 0, (108, 200), 0.05);
        assert!(g.frame(0.05, LIMITS).is_empty());
        feed(&mut g, TouchPhase::Move, 0, (120, 200), 0.1);
        assert_eq!(g.frame(0.1, LIMITS), [Act::Press, Act::Move(120.0, 200.0)]);
        feed(&mut g, TouchPhase::Move, 0, (150, 210), 0.2);
        assert_eq!(g.frame(0.2, LIMITS), [Act::Move(150.0, 210.0)]);
        assert!(g.frame(1.0, LIMITS).is_empty(), "a drag never long-presses");
        feed(&mut g, TouchPhase::Up, 0, (160, 210), 1.1);
        assert_eq!(g.frame(1.1, LIMITS), [Act::Move(160.0, 210.0), Act::Release]);
    }

    /// Feeds `g` a down on a system icon at `at`.
    fn land_on_power(g: &mut Gestures, at: (i32, i32), t: f64) {
        g.event(&ev(TouchPhase::Down, 0, at, t), (at.0 as f32, at.1 as f32), Landing::Power(PowerBox::Bars), LIMITS);
    }

    #[test]
    fn a_finger_moved_up_or_down_from_a_power_box_steps_the_power_with_it() {
        let mut g = Gestures::default();
        land_on_power(&mut g, (100, 200), 0.0);
        assert_eq!(g.frame(0.0, LIMITS), [Act::Move(100.0, 200.0)]);
        feed(&mut g, TouchPhase::Move, 0, (100, 185), 0.1);
        assert_eq!(g.frame(0.1, LIMITS), [Act::Step(1)]);
        feed(&mut g, TouchPhase::Move, 0, (103, 165), 0.2);
        assert_eq!(g.frame(0.2, LIMITS), [Act::Step(1), Act::Step(1)]);
        feed(&mut g, TouchPhase::Move, 0, (100, 195), 0.3);
        assert_eq!(g.frame(0.3, LIMITS), [Act::Step(-1), Act::Step(-1), Act::Step(-1)], "back down");
        feed(&mut g, TouchPhase::Move, 0, (100, 230), 0.4);
        assert_eq!(g.frame(0.4, LIMITS), [Act::Step(-1), Act::Step(-1), Act::Step(-1)]);
        assert_eq!(g.hold(0.4, LIMITS), None, "a swipe shows no ring");
        assert!(g.frame(1.0, LIMITS).is_empty(), "a swipe never long-presses");
        feed(&mut g, TouchPhase::Up, 0, (100, 230), 1.1);
        assert!(g.frame(1.1, LIMITS).is_empty());
    }

    #[test]
    fn a_switch_steps_one_bar_either_way_from_where_the_finger_landed() {
        let mut g = Gestures::default();
        let down = ev(TouchPhase::Down, 0, (100, 200), 0.0);
        g.event(&down, (100.0, 200.0), Landing::Power(PowerBox::Switch), LIMITS);
        g.frame(0.0, LIMITS);
        feed(&mut g, TouchPhase::Move, 0, (100, 160), 0.1);
        assert_eq!(g.frame(0.1, LIMITS), [Act::Step(1)]);
        feed(&mut g, TouchPhase::Move, 0, (100, 230), 0.2);
        assert_eq!(g.frame(0.2, LIMITS), [Act::Step(-1), Act::Step(-1)]);
    }

    #[test]
    fn a_finger_moved_sideways_from_a_power_box_drags_as_one_moved_up_elsewhere_does() {
        let mut g = Gestures::default();
        land_on_power(&mut g, (100, 200), 0.0);
        g.frame(0.0, LIMITS);
        feed(&mut g, TouchPhase::Move, 0, (120, 205), 0.1);
        assert_eq!(g.frame(0.1, LIMITS), [Act::Press, Act::Move(120.0, 205.0)]);
        feed(&mut g, TouchPhase::Up, 0, (120, 205), 0.2);
        g.frame(0.2, LIMITS);
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 1.0);
        g.frame(1.0, LIMITS);
        feed(&mut g, TouchPhase::Move, 0, (100, 170), 1.1);
        assert_eq!(g.frame(1.1, LIMITS), [Act::Press, Act::Move(100.0, 170.0)]);
        let no_step = Limits { step: 0.0, ..LIMITS };
        let mut g = Gestures::default();
        g.event(&ev(TouchPhase::Down, 0, (100, 200), 2.0), (100.0, 200.0), Landing::Power(PowerBox::Bars), no_step);
        g.frame(2.0, no_step);
        g.event(&ev(TouchPhase::Move, 0, (100, 170), 2.1), (100.0, 170.0), Landing::Open, no_step);
        assert_eq!(g.frame(2.1, no_step), [Act::Press, Act::Move(100.0, 170.0)], "no step set");
    }

    #[test]
    fn a_cancel_releases_a_drag_and_drops_a_tap() {
        let mut g = Gestures::default();
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.0);
        g.frame(0.0, LIMITS);
        feed(&mut g, TouchPhase::Cancel, 0, (100, 200), 0.1);
        assert!(g.frame(0.1, LIMITS).is_empty());
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.2);
        feed(&mut g, TouchPhase::Move, 0, (150, 200), 0.3);
        g.frame(0.3, LIMITS);
        g.frame(0.3 + DT, LIMITS);
        feed(&mut g, TouchPhase::Cancel, 0, (150, 200), 0.4);
        assert_eq!(g.frame(0.4, LIMITS), [Act::Release]);
    }

    #[test]
    fn a_new_finger_takes_over_and_the_old_one_is_ignored() {
        let mut g = Gestures::default();
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.0);
        feed(&mut g, TouchPhase::Move, 0, (150, 200), 0.05);
        g.frame(0.05, LIMITS);
        g.frame(0.05 + DT, LIMITS);
        feed(&mut g, TouchPhase::Down, 1, (300, 300), 0.1);
        assert_eq!(g.frame(0.1, LIMITS), [Act::Release, Act::Move(300.0, 300.0)]);
        feed(&mut g, TouchPhase::Move, 0, (400, 300), 0.15);
        feed(&mut g, TouchPhase::Up, 0, (400, 300), 0.2);
        assert!(g.frame(0.2, LIMITS).is_empty());
        feed(&mut g, TouchPhase::Up, 1, (300, 300), 0.3);
        assert_eq!(g.frame(0.3, LIMITS), [Act::Tap]);
    }

    #[test]
    fn two_fingers_tapped_together_pause() {
        let mut g = Gestures::default();
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.0);
        feed(&mut g, TouchPhase::Down, 1, (300, 200), 0.05);
        feed(&mut g, TouchPhase::Move, 1, (305, 203), 0.1);
        feed(&mut g, TouchPhase::Up, 0, (100, 200), 0.15);
        assert_eq!(g.frame(0.15, LIMITS), [Act::Move(100.0, 200.0)]);
        feed(&mut g, TouchPhase::Up, 1, (305, 203), 0.2);
        assert_eq!(g.frame(0.2, LIMITS), [Act::Pause]);
        assert!(g.frame(1.0, LIMITS).is_empty(), "a pair never long-presses");
    }

    #[test]
    fn a_pair_that_moves_or_stays_does_nothing() {
        let mut g = Gestures::default();
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.0);
        g.frame(0.0, LIMITS);
        feed(&mut g, TouchPhase::Down, 1, (300, 200), 0.05);
        feed(&mut g, TouchPhase::Move, 1, (350, 200), 0.1);
        feed(&mut g, TouchPhase::Up, 0, (100, 200), 0.15);
        feed(&mut g, TouchPhase::Up, 1, (350, 200), 0.2);
        assert!(g.frame(0.2, LIMITS).is_empty(), "moved");
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 1.0);
        g.frame(1.0, LIMITS);
        feed(&mut g, TouchPhase::Down, 1, (300, 200), 1.05);
        feed(&mut g, TouchPhase::Up, 1, (300, 200), 1.1);
        feed(&mut g, TouchPhase::Up, 0, (100, 200), 2.0);
        assert!(g.frame(2.0, LIMITS).is_empty(), "held");
    }

    #[test]
    fn a_pair_ignores_a_repeated_up_of_the_finger_that_lifted() {
        let mut g = Gestures::default();
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.0);
        feed(&mut g, TouchPhase::Down, 1, (300, 200), 0.05);
        feed(&mut g, TouchPhase::Up, 0, (100, 200), 0.1);
        feed(&mut g, TouchPhase::Up, 0, (100, 200), 0.15);
        assert_eq!(g.frame(0.15, LIMITS), [Act::Move(100.0, 200.0)]);
        feed(&mut g, TouchPhase::Up, 1, (300, 200), 0.2);
        assert_eq!(g.frame(0.2, LIMITS), [Act::Pause]);
    }

    #[test]
    fn a_late_second_finger_takes_over() {
        let mut g = Gestures::default();
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.0);
        g.frame(0.0, LIMITS);
        feed(&mut g, TouchPhase::Down, 1, (300, 300), 0.3);
        assert_eq!(g.frame(0.3, LIMITS), [Act::Move(300.0, 300.0)]);
        feed(&mut g, TouchPhase::Up, 1, (300, 300), 0.35);
        assert_eq!(g.frame(0.35, LIMITS), [Act::Tap]);
    }

    #[test]
    fn a_lost_up_ends_the_drag_before_the_next_down() {
        let mut g = Gestures::default();
        feed(&mut g, TouchPhase::Down, 0, (100, 200), 0.0);
        feed(&mut g, TouchPhase::Move, 0, (150, 200), 0.1);
        g.frame(0.1, LIMITS);
        g.frame(0.1 + DT, LIMITS);
        feed(&mut g, TouchPhase::Down, 0, (300, 300), 0.5);
        assert_eq!(g.frame(0.5, LIMITS), [Act::Release, Act::Move(300.0, 300.0)]);
    }

    #[test]
    fn a_touch_the_overlay_takes_does_nothing_else() {
        let mut g = Gestures::default();
        g.event(&ev(TouchPhase::Down, 0, (100, 200), 0.0), (100.0, 200.0), Landing::Overlay, LIMITS);
        feed(&mut g, TouchPhase::Move, 0, (200, 200), 0.1);
        assert!(g.frame(1.0, LIMITS).is_empty());
        feed(&mut g, TouchPhase::Up, 0, (200, 200), 1.1);
        assert!(g.frame(1.1, LIMITS).is_empty());
    }

    #[test]
    fn a_point_beside_the_targets_snaps_inside_the_nearest_within_reach() {
        let zones = [Rect { x: 100, y: 100, w: 40, h: 20 }, Rect { x: 200, y: 100, w: 6, h: 6 }];
        assert_eq!(snap(&zones, (120.0, 110.0), 24.0), (120.0, 110.0), "a point inside stays");
        assert_eq!(snap(&zones, (120.0, 90.0), 24.0), (120.0, 102.0));
        assert_eq!(snap(&zones, (150.0, 110.0), 24.0), (137.0, 110.0));
        assert_eq!(snap(&zones, (190.0, 102.0), 24.0), (202.0, 102.0), "the nearest wins");
        assert_eq!(snap(&zones, (210.0, 103.0), 24.0), (203.0, 103.0), "a small zone holds its middle");
        assert_eq!(snap(&zones, (120.0, 60.0), 24.0), (120.0, 60.0), "past the reach stays");
        assert_eq!(snap(&[], (120.0, 90.0), 24.0), (120.0, 90.0));
    }

    /// Frames a tap takes at most: the move, the game's look, the move to the nearest
    /// target, the click.
    const TAP_FRAMES: u32 = PROBE_FRAMES + 2;

    /// Runs a tap at frame point (`x`, `y`) through the layer; returns the clicks it made.
    fn tap(l: &mut Layer, x: i32, y: i32) -> Vec<Call> {
        let t = now_s();
        l.queue_touch(TouchPhase::Down, 0, x, y);
        l.queue_touch(TouchPhase::Up, 0, x, y);
        for _ in 0..TAP_FRAMES {
            l.apply_touch(t);
        }
        calls().into_iter().filter(|c| matches!(c, Call::Button(..))).collect()
    }

    #[test]
    fn a_tap_on_a_room_orders_the_selected_crew_unless_a_member_is_under_it() {
        with_game(|g| {
            let mut l = Layer { touch: Touch { hooked: true, ..Touch::default() }, ..Layer::default() };
            let room = Rect { x: 100, y: 100, w: 70, h: 35 };
            l.zones.list = vec![Zone { rect: room, id: 2, kind: ZoneKind::Room }];
            let click = |button| vec![Call::Button(button, true, 120, 110), Call::Button(button, false, 120, 110)];
            assert_eq!(tap(&mut l, 120, 110), click(MouseButton::Left));
            let member = g.crew("Ann", "human", PLAYER_SHIP_ID, false);
            g.arena.ptr_vector(g.gui, GUI_CREW_CONTROL + CREW_SELECTED, &[Some(member)]);
            assert_eq!(tap(&mut l, 120, 110), click(MouseButton::Right));
            g.arena.ptr_vector(g.gui, GUI_CREW_CONTROL + CREW_POTENTIAL, &[Some(member)]);
            assert_eq!(tap(&mut l, 120, 110), click(MouseButton::Left));
        });
    }

    #[test]
    fn a_tap_beside_a_room_orders_the_crew_into_it() {
        with_game(|g| {
            let mut l = Layer { touch: Touch { hooked: true, ..Touch::default() }, ..Layer::default() };
            let room = Rect { x: 100, y: 100, w: 70, h: 35 };
            l.zones.list = vec![Zone { rect: room, id: 2, kind: ZoneKind::Room }];
            let member = g.crew("Ann", "human", PLAYER_SHIP_ID, false);
            g.arena.ptr_vector(g.gui, GUI_CREW_CONTROL + CREW_SELECTED, &[Some(member)]);
            let click = |button, y| vec![Call::Button(button, true, 120, y), Call::Button(button, false, 120, y)];
            assert_eq!(tap(&mut l, 120, 90), click(MouseButton::Right, 102));
            g.mouse.set(MOUSE_VALID, true);
            assert_eq!(tap(&mut l, 120, 90), click(MouseButton::Left, 90), "the game takes the point");
            g.mouse.set(MOUSE_VALID, false);
            l.cfg.touch_snap = 0.0;
            assert_eq!(tap(&mut l, 120, 90), click(MouseButton::Left, 90));
        });
    }

    /// Runs the touch through the layer for `frames` frames; returns the clicks it made.
    fn clicks(l: &mut Layer, frames: u32) -> Vec<Call> {
        let t = now_s();
        for _ in 0..frames {
            l.apply_touch(t);
        }
        calls().into_iter().filter(|c| matches!(c, Call::Button(..))).collect()
    }

    #[test]
    fn a_drag_begun_before_the_probe_settles_presses_where_the_finger_landed() {
        with_game(|_| {
            let mut l = Layer { touch: Touch { hooked: true, ..Touch::default() }, ..Layer::default() };
            l.zones.list = vec![Zone { rect: Rect { x: 100, y: 100, w: 70, h: 35 }, id: 2, kind: ZoneKind::Room }];
            l.queue_touch(TouchPhase::Down, 0, 120, 90);
            l.apply_touch(now_s());
            l.queue_touch(TouchPhase::Move, 0, 160, 90);
            l.queue_touch(TouchPhase::Up, 0, 160, 90);
            let left = |down, x| Call::Button(MouseButton::Left, down, x, 90);
            assert_eq!(clicks(&mut l, TAP_FRAMES), [left(true, 120), left(false, 160)]);
        });
    }

    #[test]
    fn a_finger_landing_while_the_probe_settles_waits_for_it() {
        with_game(|_| {
            let mut l = Layer { touch: Touch { hooked: true, ..Touch::default() }, ..Layer::default() };
            l.zones.list = vec![Zone { rect: Rect { x: 100, y: 100, w: 70, h: 35 }, id: 2, kind: ZoneKind::Room }];
            l.queue_touch(TouchPhase::Down, 0, 120, 90);
            l.queue_touch(TouchPhase::Up, 0, 120, 90);
            l.apply_touch(now_s());
            l.queue_touch(TouchPhase::Down, 1, 120, 110);
            l.queue_touch(TouchPhase::Up, 1, 120, 110);
            let click =
                |y| [Call::Button(MouseButton::Left, true, 120, y), Call::Button(MouseButton::Left, false, 120, y)];
            assert_eq!(clicks(&mut l, 2 * TAP_FRAMES), [click(102), click(110)].concat());
        });
    }

    #[test]
    fn a_finger_moved_down_from_a_system_icon_right_clicks_it_once_per_step() {
        with_game(|g| {
            let mut l = Layer { touch: Touch { hooked: true, ..Touch::default() }, ..Layer::default() };
            g.player_manager();
            system_icon(g);
            // The finger lands below the icon and snaps in.
            l.queue_touch(TouchPhase::Down, 0, 60, 680);
            l.queue_touch(TouchPhase::Move, 0, 60, 690);
            l.queue_touch(TouchPhase::Move, 0, 60, 715);
            l.queue_touch(TouchPhase::Up, 0, 60, 715);
            let right = |down| Call::Button(MouseButton::Right, down, 60, 669);
            assert_eq!(clicks(&mut l, 2 * TAP_FRAMES), [right(true), right(false), right(true), right(false)]);
        });
    }

    /// Puts one system icon on the HUD.
    fn system_icon(g: &mut crate::game::fake::Game) {
        let control = g.gui.at(GUI_SYS_CONTROL);
        control.set(SYSCTRL_POSITION, Point { x: 10, y: 600 });
        let sys_box = g.arena.alloc(SYSBOX_SYSTEM + std::mem::size_of::<usize>());
        sys_box.set(SYSBOX_LOCATION, Point { x: 20, y: 30 });
        g.arena.ptr_vector(control.at(SYSCTRL_BOXES), 0, &[Some(sys_box)]);
    }

    #[test]
    fn a_swipe_from_a_game_button_beside_a_system_icon_clicks_nothing() {
        with_game(|g| {
            let mut l = Layer { touch: Touch { hooked: true, ..Touch::default() }, ..Layer::default() };
            g.player_manager();
            system_icon(g);
            // The game takes the point the finger landed on, beside the icon.
            g.mouse.set(MOUSE_VALID, true);
            l.queue_touch(TouchPhase::Down, 0, 60, 680);
            l.queue_touch(TouchPhase::Move, 0, 60, 715);
            l.queue_touch(TouchPhase::Up, 0, 60, 715);
            assert!(clicks(&mut l, 2 * TAP_FRAMES).is_empty());
        });
    }

    #[test]
    fn a_step_clicks_a_weapon_box_only_where_it_switches_the_weapon() {
        with_game(|g| {
            let mut l = Layer::default();
            g.player_manager();
            let weapons = g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL);
            let item = g.arena.alloc(WEAPON_SIZE);
            item.set(WEAPON_POWERED, true);
            let armament = g.arena.alloc(ARMAMENTBOX_ITEM + std::mem::size_of::<usize>());
            armament.set(ARMAMENTBOX_HIT, Point { x: 300, y: 640 });
            armament.set(ARMAMENTBOX_ITEM, item.addr());
            g.arena.ptr_vector(weapons, ARMAMENT_BOXES, &[Some(armament)]);
            (l.cursor.x, l.cursor.y) = (340.0, 660.0);
            l.send_cursor();
            calls();
            l.step_power(1, now_s());
            l.step_power(-1, now_s());
            let buttons: Vec<Call> = calls().into_iter().filter(|c| matches!(c, Call::Button(..))).collect();
            let right = |down| Call::Button(MouseButton::Right, down, 340, 660);
            assert_eq!(buttons, [right(true), right(false)], "powered: up arms it, so only down clicks");
        });
    }

    #[test]
    fn a_touch_closes_the_help_and_clicks_nothing() {
        with_game(|_| {
            let mut l = Layer { touch: Touch { hooked: true, ..Touch::default() }, ..Layer::default() };
            l.show_help(true);
            assert!(tap(&mut l, 120, 110).is_empty());
            assert!(!l.help.shown);
        });
    }

    #[test]
    fn a_touch_closes_a_notice_and_clicks_nothing() {
        with_game(|_| {
            let mut l = Layer { touch: Touch { hooked: true, ..Touch::default() }, ..Layer::default() };
            l.cursor.synced = true;
            l.notice = Some(Notice::until_press(PAD_LOST_NOTICE));
            assert!(tap(&mut l, 120, 110).is_empty());
            assert!(l.notice.is_none());
        });
    }

    #[test]
    fn touch_stays_in_use_until_the_pad_or_the_mouse_moves_the_pointer() {
        with_game(|g| {
            let mut l = Layer { touch: Touch { hooked: true, ..Touch::default() }, ..Layer::default() };
            l.sync_cursor();
            tap(&mut l, 120, 110);
            assert!(l.touch_in_use());
            let mut p = PadFrame::default();
            p.stick[0] = [1.0, 0.0];
            l.apply_pad(p, now_s(), DT as f32);
            assert!(!l.touch_in_use(), "a stick moved");
            tap(&mut l, 120, 110);
            l.touch.last_event = Some(now_s() - TOUCH_ECHO_S);
            g.mouse.set(0, Point { x: 300, y: 200 });
            l.sync_cursor();
            assert!(!l.touch_in_use(), "the mouse moved");
        });
    }

    #[test]
    fn a_mouse_move_right_after_a_touch_is_its_echo_and_goes_back() {
        with_game(|g| {
            let mut l = Layer { touch: Touch { hooked: true, ..Touch::default() }, ..Layer::default() };
            l.sync_cursor();
            tap(&mut l, 120, 110);
            calls();
            g.mouse.set(0, Point { x: 640, y: 360 });
            l.sync_cursor();
            assert!(l.touch_in_use());
            assert_eq!((l.cursor.last_x, l.cursor.last_y), (120, 110));
            let (wx, wy) = (l.cursor.last_wx, l.cursor.last_wy);
            assert_eq!(calls(), [Call::MouseMove(wx, wy)], "the game's mouse goes back to the touch");
            l.sync_cursor();
            assert!(calls().is_empty(), "the echo is handled once");
        });
    }

    #[test]
    fn the_pause_button_shows_on_the_ship_view_while_touch_is_in_use_and_pauses() {
        with_game(|g| {
            let mut l = Layer { touch: Touch { hooked: true, ..Touch::default() }, ..Layer::default() };
            g.player_manager();
            let button = (GAME_W - 30, GAME_H - 30);
            let at = (button.0 as f32, button.1 as f32);
            assert!(!l.pause_button_hit(at), "no touch yet");
            tap(&mut l, 120, 110);
            assert!(l.pause_button_hit(at));
            assert!(l.pause_button_hit(((GAME_W - 1) as f32, (GAME_H - 1) as f32)), "the corner's edge");
            assert!(!l.pause_button_hit(((GAME_W - 30) as f32, 25.0)), "the top right");
            l.cfg.pause_button = false;
            assert!(!l.pause_button_hit(at), "switched off");
            l.cfg.pause_button = true;
            l.queue_touch(TouchPhase::Down, 0, button.0, button.1);
            l.queue_touch(TouchPhase::Up, 0, button.0, button.1);
            for _ in 0..TAP_FRAMES {
                l.apply_touch(now_s());
            }
            let keys: Vec<Call> =
                calls().into_iter().filter(|c| matches!(c, Call::KeyDown(_) | Call::KeyUp(_))).collect();
            assert_eq!(keys, [Call::KeyDown(KEY_SPACE), Call::KeyUp(KEY_SPACE)]);
            let mut p = PadFrame::default();
            p.down[Input::Start as usize] = true;
            l.apply_pad(p, now_s(), DT as f32);
            assert!(!l.pause_button_hit(at), "a pad is in use");
        });
    }

    #[test]
    fn the_pause_button_moves_to_the_top_right_of_a_view_that_leaves_the_corner_out() {
        with_game(|g| {
            let mut l = Layer { touch: Touch { hooked: true, ..Touch::default() }, ..Layer::default() };
            g.player_manager();
            tap(&mut l, 120, 110);
            let corner = ((GAME_W - 30) as f32, (GAME_H - 30) as f32);
            l.zoom.view = Some(View { x: 640.0, y: 360.0, w: 640.0, h: 360.0, k: 2.0 });
            assert!(l.pause_button_hit(corner), "the view shows the corner");
            l.zoom.view = Some(View { x: 0.0, y: 0.0, w: 640.0, h: 360.0, k: 2.0 });
            assert!(!l.pause_button_hit(corner));
            assert!(l.pause_button_hit((610.0, 25.0)));
        });
    }

    #[test]
    fn touches_count_only_while_hooked_and_on() {
        with_game(|_| {
            let mut l = Layer::default();
            assert!(tap(&mut l, 120, 110).is_empty());
            l.touch.hooked = true;
            l.cfg.touch = false;
            assert!(tap(&mut l, 120, 110).is_empty());
        });
    }
}
