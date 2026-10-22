//! The binding engine: state stack lookup, tap and hold resolution, builtins, and where
//! wheels may open.

use crate::config::layout::{Action, Binding, Builtin, Input, Layout, Slot, SlotMode, State, INPUT_COUNT, MAX_STATES};
use crate::core::layer::Layer;
use crate::core::util::log;
use crate::game::actions::{self, key_down, key_up, set_shift, tap_key, MouseButton};
use crate::game::offsets::*;
use crate::game::state;
use crate::game::zones::ZoneKind;

/// Who an action runs for: an input, or a wheel plate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Input(Input),
    Plate,
}

impl Source {
    fn index(self) -> usize {
        match self {
            Source::Input(i) => i as usize,
            Source::Plate => INPUT_COUNT,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Source::Input(i) => i.ini_name(),
            Source::Plate => "plate",
        }
    }
}

pub struct Bindings {
    /// Pushed states above the base, with the input holding each.
    stack: Vec<(usize, Input)>,
    /// Presses waiting to turn into a tap or a hold, with the binding resolved at the press.
    pending: [Option<(Binding, f64)>; INPUT_COUNT],
    /// What each held input did when pressed, undone on release.
    active: [Option<Action>; INPUT_COUNT],
    /// The button context_click pressed for each source; None when a wheel took the press.
    context_done: [Option<MouseButton>; INPUT_COUNT + 1],
    /// Doors are the zones while precision is held.
    pub precision: bool,
    /// The abilities key repeat_ability repeats.
    pub last_ability_key: Option<i32>,
    /// dpad_mode toggled: the D-pad and the sticks move the cursor.
    pub dpad_mouse: bool,
}

impl Default for Bindings {
    fn default() -> Self {
        Bindings {
            stack: Vec::new(),
            pending: [None; INPUT_COUNT],
            active: [None; INPUT_COUNT],
            context_done: [None; INPUT_COUNT + 1],
            precision: false,
            last_ability_key: None,
            dpad_mouse: false,
        }
    }
}

impl Bindings {
    /// Drops pushed states and pending presses, as a new layout needs.
    pub fn reset(&mut self) {
        self.stack.clear();
        self.pending = [None; INPUT_COUNT];
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// The states from the top of the stack down to the base, as far as lookups reach.
    fn chain<'a>(&self, layout: &'a Layout) -> Vec<&'a State> {
        let mut chain = Vec::new();
        for level in (0..=self.stack.len()).rev() {
            let index = if level == 0 { 0 } else { self.stack[level - 1].0 };
            let Some(s) = layout.states.get(index) else {
                break;
            };
            chain.push(s);
            if level > 0 && !s.parent {
                break;
            }
        }
        chain
    }

    /// The binding an input resolves to in the active state chain.
    pub fn lookup(&self, layout: &Layout, input: Input) -> Option<Binding> {
        self.chain(layout).into_iter().find_map(|s| s.bind[input as usize])
    }

    /// What a pointer source does in the active state chain: keys once any state binds its
    /// directions, else the mode the nearest state sets.
    pub fn slot_mode(&self, layout: &Layout, slot: Slot) -> SlotMode {
        for s in self.chain(layout) {
            if (0..4).any(|d| s.bind[slot.first_input() + d].is_some()) {
                return SlotMode::Keys;
            }
            if s.slot[slot as usize] != SlotMode::Keys {
                return s.slot[slot as usize];
            }
        }
        SlotMode::Keys
    }

    /// The names of the states on the stack, base first.
    pub fn stack_names(&self, layout: &Layout) -> Vec<String> {
        std::iter::once(0)
            .chain(self.stack.iter().map(|&(s, _)| s))
            .filter_map(|i| layout.states.get(i).map(|s| s.name.clone()))
            .collect()
    }

    /// Pushes a state held by an input, while the stack has room.
    fn push_state(&mut self, state: usize, input: Input) {
        if self.stack.len() + 1 < MAX_STATES {
            self.stack.push((state, input));
        }
    }

    /// Pops the topmost state the input pushed, with every state above it.
    fn pop_states(&mut self, input: Input) {
        if let Some(level) = self.stack.iter().rposition(|&(_, by)| by == input) {
            self.stack.truncate(level);
        }
    }

    /// Takes the presses held for at least `hold_s` by `t`, in input order.
    fn take_due(&mut self, t: f64, hold_s: f64) -> Vec<(Input, Binding)> {
        let mut due = Vec::new();
        for input in Input::ALL {
            let Some((b, since)) = self.pending[input as usize] else {
                continue;
            };
            if t - since >= hold_s {
                self.pending[input as usize] = None;
                due.push((input, b));
            }
        }
        due
    }
}

impl Layer {
    pub fn stack_names(&self) -> Vec<String> {
        self.bind.stack_names(&self.layout)
    }

    pub fn slot_mode(&self, slot: Slot) -> SlotMode {
        self.bind.slot_mode(&self.layout, slot)
    }

    /// Runs a builtin on the press or release of a source.
    pub fn builtin(&mut self, id: Builtin, source: Source, down: bool) {
        match id {
            Builtin::ContextClick => {
                let slot = source.index();
                if !down {
                    if let Some(button) = self.bind.context_done[slot].take() {
                        self.click(button, false);
                    }
                } else if self.wheel.open.is_some() && source != Source::Plate {
                    self.bind.context_done[slot] = None;
                    self.act_on_plate();
                } else {
                    // With crew selected and a room under the cursor, A sends them there,
                    // which the game does on the right button.
                    let room = self
                        .zone_under_cursor()
                        .is_some_and(|i| matches!(self.zones.list[i].kind, ZoneKind::Room | ZoneKind::EnemyRoom));
                    let crew_order = state::any_crew_selected() && !state::aiming() && room;
                    let button = if crew_order { MouseButton::Right } else { MouseButton::Left };
                    self.bind.context_done[slot] = Some(button);
                    self.click(button, true);
                }
            }
            Builtin::Cancel if down => {
                if self.wheel.open.is_some() {
                    self.close_wheel();
                } else if state::gui_window_open(GUI_MENU_BOX) || state::gui_window_open(GUI_OPTIONS_BOX) {
                    tap_key(KEY_ESCAPE, false);
                } else if state::aiming() {
                    // The right button cancels targeting.
                    self.click_tap(MouseButton::Right);
                } else if state::any_crew_selected() {
                    if let Some(gui) = state::gui() {
                        // SAFETY: ClearSelectedCrew on the GUI's CrewControl.
                        unsafe { (crate::game::api::api().clear_crew)(gui.at(GUI_CREW_CONTROL).ptr()) };
                    }
                } else if state::open_tabbed().is_some() || state::choice_open() || state::star_map_open() {
                    tap_key(KEY_ESCAPE, false);
                }
            }
            Builtin::ArmNext if down => actions::arm_next(),
            // On a tabbed screen the tap walks its tabs instead.
            Builtin::AimCrew if down => {
                if state::open_tabbed().is_some() {
                    actions::tab_step(1);
                } else {
                    actions::select_next_crew();
                }
            }
            Builtin::RepeatAbility if down => {
                if let Some(key) = self.bind.last_ability_key {
                    tap_key(key, false);
                }
            }
            // Held: doors are the zones for as long as the button is down.
            Builtin::Precision => self.bind.precision = down,
            Builtin::CenterSwap if down => self.center_swap(),
            Builtin::PrevTab if down => actions::tab_step(-1),
            Builtin::NextTab if down => actions::tab_step(1),
            Builtin::Help if down => self.help_shown = !self.help_shown,
            Builtin::RoomDoors if down => self.room_doors(),
            Builtin::Pause if down => tap_key(KEY_SPACE, false),
            Builtin::DpadMode if down => self.bind.dpad_mouse = !self.bind.dpad_mouse,
            Builtin::FastForward => {
                if down && state::game_paused() {
                    tap_key(KEY_SPACE, false);
                }
                self.set_fast(down);
            }
            _ => {}
        }
    }

    /// Performs an action's press or release.
    pub fn perform(&mut self, action: Action, source: Source, down: bool) {
        match action {
            Action::Key { code, shift } => {
                if down {
                    if shift {
                        set_shift(true);
                    }
                    key_down(code);
                } else {
                    key_up(code);
                    if shift {
                        set_shift(false);
                    }
                }
            }
            Action::LClick => self.click(MouseButton::Left, down),
            Action::RClick => self.click(MouseButton::Right, down),
            Action::State(state) => {
                let Source::Input(input) = source else {
                    return;
                };
                if down {
                    self.bind.push_state(state, input);
                } else {
                    self.bind.pop_states(input);
                }
            }
            Action::Wheel(wheel) => {
                if down {
                    self.open_wheel(wheel, source);
                }
            }
            Action::Builtin(b) => self.builtin(b, source, down),
        }
        if self.debug && down {
            let what = match action {
                Action::Key { .. } => "key",
                Action::Builtin(b) => b.name(),
                Action::Wheel(_) => "wheel",
                Action::State(_) => "state",
                Action::LClick | Action::RClick => "click",
            };
            log!("{}: {what}", source.name());
        }
    }

    /// Whether a tap action would do nothing in the current context.
    fn tap_is_idle(&self, tap: Action) -> bool {
        match tap {
            Action::Builtin(Builtin::PrevTab | Builtin::NextTab) => state::open_tabbed().is_none(),
            Action::Builtin(Builtin::RepeatAbility) => self.bind.last_ability_key.is_none(),
            _ => false,
        }
    }

    /// Performs an action's press and records it for the release.
    fn start_hold(&mut self, action: Action, input: Input) {
        self.perform(action, Source::Input(input), true);
        self.bind.active[input as usize] = Some(action);
    }

    /// Whether wheels may open: anywhere, or with wheels_ship_only only on the ship view
    /// with nothing over it (no menu, ship or store screen, dialog or map).
    pub fn wheel_screen(&self) -> bool {
        if !self.cfg.wheels_ship_only {
            return true;
        }
        let Some(gui) = state::gui() else {
            return false;
        };
        state::player_complete().is_some()
            && !state::main_menu_open()
            && state::open_tabbed().is_none()
            && state::modal_window(gui).is_none()
            && !state::choice_open()
            && !state::star_map_open()
    }

    /// A press runs its binding at once, unless it carries a tap half that would act: then
    /// it waits for the threshold (hold) or the release (tap).
    pub fn press(&mut self, input: Input, t: f64) {
        self.bind.active[input as usize] = None;
        let Some(b) = self.bind.lookup(&self.layout, input) else {
            return;
        };
        if let Action::Wheel(w) = b.hold {
            // Off the ship view a wheel button is its tap alone, at once.
            if self.wheel.open.is_none() && !self.wheel_screen() {
                if let Some(tap) = b.tap {
                    self.perform(tap, Source::Input(input), true);
                    self.perform(tap, Source::Input(input), false);
                }
                return;
            }
            // The open wheel's own button closes it, its tap half included.
            if self.wheel.open == Some(w) {
                self.close_wheel();
                return;
            }
        }
        if b.tap.is_some_and(|tap| !self.tap_is_idle(tap)) {
            self.bind.pending[input as usize] = Some((b, t));
            return;
        }
        self.start_hold(b.hold, input);
    }

    pub fn release(&mut self, input: Input) {
        if let Some((b, _)) = self.bind.pending[input as usize].take() {
            if let Some(tap) = b.tap {
                self.perform(tap, Source::Input(input), true);
                self.perform(tap, Source::Input(input), false);
            }
            return;
        }
        if let Some(action) = self.bind.active[input as usize].take() {
            self.perform(action, Source::Input(input), false);
        }
    }

    /// Turns presses that outlived the tap threshold into holds.
    pub fn settle_pending(&mut self, t: f64) {
        for (input, b) in self.bind.take_due(t, self.cfg.tap_hold as f64) {
            self.start_hold(b.hold, input);
        }
    }

    /// Runs a plate's builtin as a tap.
    pub fn builtin_tap(&mut self, b: Builtin) {
        self.builtin(b, Source::Plate, true);
        self.builtin(b, Source::Plate, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGES: &str = "[controls]\nl1 = hold_state depower\nr1 = hold_state solo\na = space\nb = esc\n\
                         [controls:depower]\noverlay = parent\na = 1 add_shift\n\
                         [controls:solo]\nleft_analog = mouse_movement\n";

    fn key(code: char) -> Option<Action> {
        Some(Action::Key { code: code as i32, shift: false })
    }

    #[test]
    fn overlay_page_falls_through_to_the_base() {
        let layout = Layout::parse(PAGES);
        let mut b = Bindings::default();
        b.push_state(1, Input::L1);
        assert_eq!(
            b.lookup(&layout, Input::FaceDown).map(|b| b.hold),
            Some(Action::Key { code: '1' as i32, shift: true })
        );
        assert_eq!(
            b.lookup(&layout, Input::FaceRight).map(|b| b.hold),
            Some(Action::Key { code: KEY_ESCAPE, shift: false })
        );
        assert_eq!(b.stack_names(&layout), ["controls", "depower"]);
    }

    #[test]
    fn page_without_overlay_hides_the_base() {
        let layout = Layout::parse(PAGES);
        let mut b = Bindings::default();
        assert_eq!(b.lookup(&layout, Input::FaceDown).map(|b| b.hold), key(' '));
        b.push_state(2, Input::R1);
        assert_eq!(b.lookup(&layout, Input::FaceDown), None);
        assert_eq!(b.slot_mode(&layout, Slot::LStick), SlotMode::Mouse);
    }

    #[test]
    fn release_pops_its_page_and_those_above() {
        let mut b = Bindings::default();
        b.push_state(1, Input::L1);
        b.push_state(2, Input::R1);
        b.pop_states(Input::L1);
        assert_eq!(b.depth(), 0);
    }

    #[test]
    fn stack_stops_below_the_state_limit() {
        let mut b = Bindings::default();
        for _ in 0..MAX_STATES * 2 {
            b.push_state(1, Input::L1);
        }
        assert_eq!(b.depth(), MAX_STATES - 1);
    }

    #[test]
    fn a_press_turns_into_a_hold_after_the_threshold() {
        let layout = Layout::parse("[controls]\nx = wheel w | arm_next\n[wheel:w]\n");
        let mut b = Bindings::default();
        let binding = b.lookup(&layout, Input::FaceLeft).expect("x is bound");
        b.pending[Input::FaceLeft as usize] = Some((binding, 1.0));
        assert!(b.take_due(1.15, 0.2).is_empty());
        assert_eq!(b.take_due(1.25, 0.2), [(Input::FaceLeft, binding)]);
        assert!(b.take_due(2.0, 0.2).is_empty());
    }
}
