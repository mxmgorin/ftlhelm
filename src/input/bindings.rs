//! The binding engine: state stack lookup, tap and hold resolution, builtins, and where
//! wheels may open.

use crate::config::layout::{Action, Binding, Builtin, Input, Slot, SlotMode, State, INPUT_COUNT, MAX_STATES};
use crate::core::layer::Layer;
use crate::core::util::{log, now_s};
use crate::game::actions::{self, key_down, key_up, set_shift, tap_key, Button};
use crate::game::offsets::*;
use crate::game::state;
use crate::game::zones::TargetKind;

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
    context_done: [Option<Button>; INPUT_COUNT + 1],
    /// Doors are the zones while precision is held.
    pub precision: bool,
    /// The abilities key repeat_ability repeats.
    pub last_ability_key: Option<i32>,
    /// dpad_mode toggled: the D-pad and the sticks move the cursor.
    pub dpad_mouse: bool,
    /// When pause_ff went down, while it is held.
    pub pause_ff_since: Option<f64>,
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
            pause_ff_since: None,
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
}

impl Layer {
    /// The states from the top of the stack down to the base, as far as lookups reach.
    fn state_chain(&self) -> Vec<&State> {
        let mut chain = Vec::new();
        for level in (0..=self.bind.stack.len()).rev() {
            let index = if level == 0 { 0 } else { self.bind.stack[level - 1].0 };
            let Some(s) = self.layout.states.get(index) else {
                break;
            };
            chain.push(s);
            if level > 0 && !s.parent {
                break;
            }
        }
        chain
    }

    /// The names of the states on the stack, base first.
    pub fn stack_names(&self) -> Vec<String> {
        std::iter::once(0)
            .chain(self.bind.stack.iter().map(|&(s, _)| s))
            .filter_map(|i| self.layout.states.get(i).map(|s| s.name.clone()))
            .collect()
    }

    /// The binding an input resolves to in the active state chain.
    fn lookup(&self, input: Input) -> Option<Binding> {
        self.state_chain().into_iter().find_map(|s| s.bind[input as usize])
    }

    /// What a pointer source does in the active state chain: keys once any state binds its
    /// directions, else the mode the nearest state sets.
    pub fn slot_mode(&self, slot: Slot) -> SlotMode {
        for s in self.state_chain() {
            if (0..4).any(|d| s.bind[slot.first_input() + d].is_some()) {
                return SlotMode::Keys;
            }
            if s.slot[slot as usize] != SlotMode::Keys {
                return s.slot[slot as usize];
            }
        }
        SlotMode::Keys
    }

    /// Runs a builtin on the press or release of a source.
    pub fn builtin(&mut self, id: Builtin, source: Source, down: bool, t: f64) {
        match id {
            Builtin::ContextClick => {
                let slot = source.index();
                if !down {
                    if let Some(button) = self.bind.context_done[slot].take() {
                        self.click(button, false);
                    }
                } else if self.wheel.open.is_some() && source != Source::Plate {
                    self.bind.context_done[slot] = None;
                    self.wheel_act();
                } else {
                    // With crew selected and a room under the cursor, A sends them there,
                    // which the game does on the right button.
                    let room = self
                        .target_under_cursor()
                        .is_some_and(|i| matches!(self.zones.targets[i].kind, TargetKind::Room | TargetKind::EnemyRoom));
                    let crew_order = state::crew_selected() && !state::aiming() && room;
                    let button = if crew_order { Button::Right } else { Button::Left };
                    self.bind.context_done[slot] = Some(button);
                    self.click(button, true);
                }
            }
            Builtin::Cancel if down => {
                if self.wheel.open.is_some() {
                    self.wheel_close();
                } else if state::gui_window_open(GUI_MENU_BOX) || state::gui_window_open(GUI_OPTIONS_BOX) {
                    tap_key(KEY_ESCAPE, false);
                } else if state::aiming() {
                    // The right button cancels targeting.
                    self.click_tap(Button::Right);
                } else if state::crew_selected() {
                    if let Some(gui) = state::gui() {
                        // SAFETY: ClearSelectedCrew on the GUI's CrewControl.
                        unsafe { (crate::game::api::api().clear_crew)(gui.at(GUI_CREW_CONTROL).ptr()) };
                    }
                } else if state::open_tabbed().is_some() || state::choice_open() || state::star_map_open() {
                    tap_key(KEY_ESCAPE, false);
                }
            }
            Builtin::PauseFf => {
                if down {
                    tap_key(KEY_SPACE, false);
                    self.bind.pause_ff_since = Some(t);
                } else {
                    if self.actions.fast_forward {
                        self.set_fast(false);
                    }
                    self.bind.pause_ff_since = None;
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
    pub fn perform(&mut self, action: Action, source: Source, down: bool, t: f64) {
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
            Action::LClick => self.click(Button::Left, down),
            Action::RClick => self.click(Button::Right, down),
            Action::State(state) => {
                let Source::Input(input) = source else {
                    return;
                };
                if down {
                    if self.bind.stack.len() + 1 < MAX_STATES {
                        self.bind.stack.push((state, input));
                    }
                } else if let Some(level) = self.bind.stack.iter().rposition(|&(_, by)| by == input) {
                    self.bind.stack.truncate(level);
                }
            }
            Action::Wheel(wheel) => {
                if down {
                    self.wheel_show(wheel, source);
                }
            }
            Action::Builtin(b) => self.builtin(b, source, down, t),
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
    fn tap_idle(&self, tap: Action) -> bool {
        match tap {
            Action::Builtin(Builtin::PrevTab | Builtin::NextTab) => state::open_tabbed().is_none(),
            Action::Builtin(Builtin::RepeatAbility) => self.bind.last_ability_key.is_none(),
            _ => false,
        }
    }

    /// Performs an action's press and records it for the release.
    fn start(&mut self, action: Action, input: Input, t: f64) {
        self.perform(action, Source::Input(input), true, t);
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
        let Some(b) = self.lookup(input) else {
            return;
        };
        if let Action::Wheel(w) = b.hold {
            // Off the ship view a wheel button is its tap alone, at once.
            if self.wheel.open.is_none() && !self.wheel_screen() {
                if let Some(tap) = b.tap {
                    self.perform(tap, Source::Input(input), true, t);
                    self.perform(tap, Source::Input(input), false, t);
                }
                return;
            }
            // The open wheel's own button closes it, its tap half included.
            if self.wheel.open == Some(w) {
                self.wheel_close();
                return;
            }
        }
        if b.tap.is_some_and(|tap| !self.tap_idle(tap)) {
            self.bind.pending[input as usize] = Some((b, t));
            return;
        }
        self.start(b.hold, input, t);
    }

    pub fn release(&mut self, input: Input, t: f64) {
        if let Some((b, _)) = self.bind.pending[input as usize].take() {
            if let Some(tap) = b.tap {
                self.perform(tap, Source::Input(input), true, t);
                self.perform(tap, Source::Input(input), false, t);
            }
            return;
        }
        if let Some(action) = self.bind.active[input as usize].take() {
            self.perform(action, Source::Input(input), false, t);
        }
    }

    /// Turns presses that outlived the tap threshold into holds.
    pub fn settle_pending(&mut self, t: f64) {
        for input in Input::ALL {
            let Some((b, since)) = self.bind.pending[input as usize] else {
                continue;
            };
            if t - since < self.cfg.tap_hold as f64 {
                continue;
            }
            self.bind.pending[input as usize] = None;
            self.start(b.hold, input, t);
        }
    }

    /// Runs a plate's builtin as a tap.
    pub fn builtin_tap(&mut self, b: Builtin) {
        self.builtin(b, Source::Plate, true, now_s());
        self.builtin(b, Source::Plate, false, now_s());
    }
}
