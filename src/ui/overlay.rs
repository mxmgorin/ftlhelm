//! What the layer draws over the game each frame: the open wheel, the help table and the
//! mode badge.

use super::draw::*;
use crate::config::layout::{key_name, truncate_label, Action, Binding, Builtin, SIL_BUTTONS};
use crate::core::layer::Layer;
use crate::game::offsets::*;
use crate::game::state;

const HELP_FONT: i32 = WHEEL_FONT;
const HELP_LINE: f32 = 30.0;
const HELP_W: f32 = 760.0;
const HELP_PAD: f32 = 12.0;
const HELP_COL_TAP: f32 = 130.0;
const HELP_COL_HOLD: f32 = 430.0;
const HELP_RULE_H: f32 = 1.0;
/// A rule sits this far above the top of the row it opens.
const HELP_RULE_GAP: f32 = 2.0;
const HELP_VERSION: &str = concat!("ftlhelm ", env!("CARGO_PKG_VERSION"));
const URL_SCHEME: &str = "https://";
/// A held click drags: a selection box over the ship, an item on a screen.
const CLICK_HOLD_TITLE: &str = "Drag select";
const BADGE_H: f32 = 30.0;
const BADGE_PAD: f32 = 14.0;
const BADGE_SEPARATOR: &str = " / ";
const BADGE_MARGIN: f32 = 8.0;
/// Text sits this far below a panel row's top edge.
const TEXT_DY: f32 = 5.0;

const INPUT_TITLES: [&str; SIL_BUTTONS] =
    ["Guide", "Start", "Select", "Y", "X", "B", "A", "L1", "R1", "L2", "R2", "L3", "R3"];

/// What the game's own hotkeys do, as the help names them.
#[rustfmt::skip]
const KEY_TITLES: &[(i32, &str)] = &[
    (KEY_ESCAPE, "Menu"), (KEY_SPACE, "Pause"), ('j' as i32, "Jump"), ('u' as i32, "Upgrades"),
    (KEY_AUTOFIRE, "Autofire"), ('z' as i32, "Open doors"), ('x' as i32, "Close doors"), ('q' as i32, "All crew"),
    (KEY_ENTER, "To stations"), ('/' as i32, "Save stations"),
];

fn builtin_title(b: Builtin) -> &'static str {
    match b {
        Builtin::Cancel => "Cancel",
        Builtin::ContextClick => "Click",
        Builtin::ArmNext => "Next weapon",
        Builtin::AimCrew => "Next crew",
        Builtin::RepeatAbility => "Repeat ability",
        Builtin::Precision => "Doors mode",
        Builtin::CenterSwap => "Other ship",
        Builtin::PrevTab => "Previous tab",
        Builtin::NextTab => "Next tab",
        Builtin::Help => "Help",
        Builtin::RoomDoors => "Room doors",
        Builtin::Pause => "Pause",
        Builtin::FastForward => "Fast forward",
        Builtin::DpadMode => "Cursor / zones",
    }
}

/// The top of a line of text centred between `top` and `bottom`.
fn band_text_top(top: f32, bottom: f32) -> f32 {
    (top + bottom - WHEEL_FONT_H) / 2.0
}

/// Whether an action acts while held: wheels, states, doors mode, fast forward.
fn is_hold_action(a: Action) -> bool {
    matches!(a, Action::Wheel(_) | Action::State(_) | Action::Builtin(Builtin::Precision | Builtin::FastForward))
}

impl Layer {
    /// Draws the overlay; returns whether the game's own pointer should be drawn, as it
    /// would cover the middle of an open wheel.
    pub fn draw_overlay(&self) -> bool {
        if self.wheel.open.is_some() {
            self.draw_wheel();
        }
        if self.help_shown && self.cursor.synced {
            self.draw_help();
        }
        if self.cursor.synced && self.cfg.overlay && state::gui().is_some() {
            let text = self.badge_text();
            if !text.is_empty() {
                let w = text_width(WHEEL_FONT, &text) + 2.0 * BADGE_PAD;
                let (x, y) = (GAME_W as f32 - BADGE_MARGIN - w, BADGE_MARGIN);
                draw_panel(x, y, w, BADGE_H, PLATE_COLOR, PLATE_EDGE_COLOR);
                draw_text_center(WHEEL_FONT, x + w / 2.0, y + TEXT_DY, &text);
            }
        }
        self.wheel.open.is_none()
    }

    fn describe(&self, a: Action) -> String {
        match a {
            Action::Key { code, .. } => {
                if let Some(&(_, title)) = KEY_TITLES.iter().find(|(c, _)| *c == code) {
                    title.to_string()
                } else if let Some(name) = key_name(code) {
                    name.to_string()
                } else if (KEY_F1..KEY_F1 + KEY_F_COUNT).contains(&code) {
                    format!("f{}", code - KEY_F1 + 1)
                } else {
                    u8::try_from(code).map_or_else(|_| code.to_string(), |c| (c as char).to_string())
                }
            }
            Action::LClick => "click".to_string(),
            Action::RClick => "right click".to_string(),
            Action::State(s) => format!("{} page", self.layout.states.get(s).map_or("?", |s| &s.name)),
            Action::Wheel(w) => {
                let name = self.layout.wheels.get(w).map_or("?", |w| &w.name);
                let mut chars = name.chars();
                let first: String = chars.next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
                format!("{first}{} wheel", chars.as_str())
            }
            Action::Builtin(b) => builtin_title(b).to_string(),
        }
    }

    /// The tap and hold columns of a binding.
    fn help_row(&self, b: Binding) -> (String, String) {
        match b.tap {
            Some(tap) => (self.describe(tap), self.describe(b.hold)),
            None if b.hold == Action::Builtin(Builtin::ContextClick) => {
                (self.describe(b.hold), CLICK_HOLD_TITLE.to_string())
            }
            None if is_hold_action(b.hold) => (String::new(), self.describe(b.hold)),
            None => (self.describe(b.hold), String::new()),
        }
    }

    /// Draws the base state's bindings as a table: button, tap, hold.
    fn draw_help(&self) {
        let Some(base) = self.layout.states.first() else {
            return;
        };
        let rows: Vec<(usize, Binding)> = (0..SIL_BUTTONS).filter_map(|i| base.bind[i].map(|b| (i, b))).collect();
        // A header row, the bindings, then the footer.
        let h = HELP_LINE * (rows.len() + 2) as f32 + 2.0 * HELP_PAD;
        let (x, y) = ((GAME_W as f32 - HELP_W) / 2.0, (GAME_H as f32 - h) / 2.0);
        draw_panel(x, y, HELP_W, h, PLATE_COLOR, PLATE_EDGE_COLOR);
        let mut line_y = y + HELP_PAD + HELP_LINE;
        let rule_y = line_y - HELP_RULE_GAP;
        let header_y = band_text_top(y, rule_y);
        draw_text_left(HELP_FONT, x + HELP_PAD, header_y, "BUTTON");
        draw_text_left(HELP_FONT, x + HELP_COL_TAP, header_y, "TAP");
        draw_text_left(HELP_FONT, x + HELP_COL_HOLD, header_y, "HOLD");
        draw_rect(x + HELP_PAD, rule_y, HELP_W - 2.0 * HELP_PAD, HELP_RULE_H, PLATE_EDGE_COLOR);
        for (i, b) in rows {
            let (tap, hold) = self.help_row(b);
            draw_text_left(HELP_FONT, x + HELP_PAD, line_y + TEXT_DY, INPUT_TITLES[i]);
            draw_text_left(HELP_FONT, x + HELP_COL_TAP, line_y + TEXT_DY, &tap);
            draw_text_left(HELP_FONT, x + HELP_COL_HOLD, line_y + TEXT_DY, &hold);
            line_y += HELP_LINE;
        }
        let rule_y = line_y - HELP_RULE_GAP;
        draw_rect(x + HELP_PAD, rule_y, HELP_W - 2.0 * HELP_PAD, HELP_RULE_H, PLATE_EDGE_COLOR);
        let footer_y = band_text_top(rule_y + HELP_RULE_H, y + h);
        let url = env!("CARGO_PKG_REPOSITORY").trim_start_matches(URL_SCHEME);
        set_color(TEXT_DISABLED_COLOR);
        draw_text_left(HELP_FONT, x + HELP_PAD, footer_y, HELP_VERSION);
        draw_text_left(HELP_FONT, x + HELP_W - HELP_PAD - text_width(HELP_FONT, url), footer_y, url);
        set_color(TEXT_COLOR);
    }

    /// The mode badge text, empty in the plain state: the weapon waiting for a target and
    /// the layer's own modes, which the game does not show.
    pub fn badge_text(&self) -> String {
        if self.wheel.open.is_some() {
            return String::new();
        }
        let mut parts = Vec::new();
        if state::mouse().i32(MOUSE_AIMING) != 0 {
            let weapon = state::weapon_control()
                .and_then(|c| c.obj(WEAPONCTRL_ARMED))
                .and_then(|w| w.obj(WEAPON_BLUEPRINT))
                .map(|bp| truncate_label(&state::display_text(bp.at(BLUEPRINT_SHORT_TITLE))))
                .unwrap_or_default();
            parts.push(if weapon.is_empty() { "Target".to_string() } else { format!("Target: {weapon}") });
        }
        let modes =
            [(self.bind.precision, "Doors"), (self.actions.fast_forward, "Fast"), (self.bind.dpad_mouse, "Mouse")];
        parts.extend(modes.iter().filter(|(on, _)| *on).map(|(_, name)| name.to_string()));
        parts.join(BADGE_SEPARATOR).to_ascii_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::layout::LABEL_CHARS;
    use crate::game::fake::with_game;
    use std::mem::size_of;

    #[test]
    fn the_badge_names_the_armed_weapon_and_the_layer_modes() {
        with_game(|g| {
            let mut l = Layer::default();
            assert_eq!(l.badge_text(), "");
            (l.bind.precision, l.bind.dpad_mouse) = (true, true);
            assert_eq!(l.badge_text(), "DOORS / MOUSE");
            g.mouse.set(MOUSE_AIMING, 1i32);
            assert_eq!(l.badge_text(), "TARGET / DOORS / MOUSE");
            let title = "Heavy Laser Mark Two Prototype";
            let bp = g.arena.alloc(BLUEPRINT_SHORT_TITLE + TEXTSTRING_LITERAL + size_of::<u8>());
            g.literal(bp, BLUEPRINT_SHORT_TITLE, title);
            let weapon = g.arena.alloc(WEAPON_BLUEPRINT + size_of::<usize>());
            weapon.set(WEAPON_BLUEPRINT, bp.addr());
            g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL).set(WEAPONCTRL_ARMED, weapon.addr());
            let name: String = title.chars().take(LABEL_CHARS).collect();
            assert_eq!(l.badge_text(), format!("TARGET: {} / DOORS / MOUSE", name.to_ascii_uppercase()));
        });
    }
}
