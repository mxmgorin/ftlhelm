//! What the layer draws over the game each frame: the open wheel, the help, a notice and the
//! mode badge, and the window helpers the help and the notice share.

use super::draw::*;
use super::text;
use crate::core::layer::Layer;
use crate::core::util::{clampf, now_s};
use crate::game::offsets::*;
use crate::game::state;
use crate::game::zones::ZoneKind;

pub const HELP_FONT: i32 = WHEEL_FONT;
pub const HELP_LINE: f32 = 30.0;
pub const HELP_PAD: f32 = 12.0;
/// The pause button in the bottom right corner of the ship view's columns; the margin holds its
/// glow and keeps its reach off the HUD's rightmost box, the battery's.
const PAUSE_BUTTON_SIZE: f32 = 56.0;
const PAUSE_MARGIN: f32 = 8.0;
/// A touch reaches the button this far past its edge: in the corner, up to the frame's edges.
const PAUSE_HIT_PAD: f32 = PAUSE_MARGIN;
/// Its glyph's two bars.
const PAUSE_BAR_W: f32 = 7.0;
const PAUSE_BAR_H: f32 = 22.0;
/// Keeps the glyph's width even like the button's, so it centres on whole pixels.
const PAUSE_BAR_GAP: f32 = 8.0;
/// Text sits this far below a panel row's top edge.
pub const TEXT_DY: f32 = 5.0;
/// The least room a scaled panel leaves to the edges of what is shown.
const PANEL_MARGIN: f32 = 8.0;

pub const PAD_LOST_NOTICE: &[&str] = &["Pad disconnected"];
pub const LAYOUT_UPDATED_NOTICE: &[&str] =
    &["Controls and wheels reset to the new defaults", "The old layout is in ftlhelm.old.ini"];
/// Seconds the layout notice stays on screen without a press: mouse clicks do not reach it.
pub const LAYOUT_NOTICE_S: f32 = 10.0;

/// A message over the frame, its lines centred.
pub struct Notice {
    lines: &'static [&'static str],
    /// Seconds it has left on screen; None: no limit. A press or a touch closes it either way.
    left: Option<f32>,
}

impl Notice {
    /// A notice that stays until a button is pressed.
    pub fn until_press(lines: &'static [&'static str]) -> Notice {
        Notice { lines, left: None }
    }

    /// A notice that stays `seconds` on screen, or until a button is pressed.
    pub fn for_seconds(lines: &'static [&'static str], seconds: f32) -> Notice {
        Notice { lines, left: Some(seconds) }
    }
}

/// The scale of a `w` by `h` panel shown in `area`: the text size, as far as the area holds
/// it within PANEL_MARGIN; never below the game's own.
pub fn panel_scale(text: f32, w: f32, h: f32, (area_w, area_h): (f32, f32)) -> f32 {
    let fit = ((area_w - 2.0 * PANEL_MARGIN) / w).min((area_h - 2.0 * PANEL_MARGIN) / h);
    clampf(text, 1.0, fit.max(1.0))
}

/// A focused zone's corner brackets, in frame pixels: their arms take a share of the zone's
/// shorter side within bounds, and they slide in from further out as the focus lands.
const FOCUS_ARM_SHARE: f32 = 0.25;
const FOCUS_ARM_MIN: f32 = 2.0 * FOCUS_CUT as f32;
const FOCUS_ARM_MAX: f32 = 14.0;
const FOCUS_THICK: i32 = 2;
/// The cut of the game's selection boxes, half the shortest arm.
const FOCUS_CUT: i32 = 4;
const FOCUS_GAP: f32 = 1.0;
/// An event choice's zone ends at its text: its brackets keep this much further out sideways.
const FOCUS_TEXT_PAD: f32 = 4.0;
const FOCUS_SLIDE: f32 = 6.0;
const FOCUS_SETTLE_S: f64 = 0.12;
/// The marker over a selected crew member, in frame pixels above its position; it bobs
/// CREW_MARK_BOB_PX up and down once every CREW_MARK_BOB_S.
const CREW_MARK: Marker = Marker { w: 16, h: 9 };
const CREW_MARK_RISE: f32 = 24.0;
const CREW_MARK_BOB_PX: f32 = 2.0;
const CREW_MARK_BOB_S: f64 = 1.0;
/// The most times its size the crew marker is drawn.
const CREW_MARK_SCALE_MAX: f32 = 1.5;

/// Draws the pause button at (`x`, `y`): a game button with two bars, lit while the game is
/// paused.
fn draw_pause_button(x: f32, y: f32) {
    let s = PAUSE_BUTTON_SIZE;
    let fill = if state::game_paused() { BUTTON_SELECTED_COLOR } else { BUTTON_PALE_COLOR };
    draw_frame(x, y, s, s, &BUTTON_FRAME, fill);
    let (cx, top) = (x + s / 2.0, y + (s - PAUSE_BAR_H) / 2.0);
    draw_rect(cx - PAUSE_BAR_GAP / 2.0 - PAUSE_BAR_W, top, PAUSE_BAR_W, PAUSE_BAR_H, INK_COLOR);
    draw_rect(cx + PAUSE_BAR_GAP / 2.0, top, PAUSE_BAR_W, PAUSE_BAR_H, INK_COLOR);
}

/// Draws a pale window `w` by `h` centred on (`cx`, `cy`), unscaled: its cut stays on whole
/// pixels at any text size.
pub fn draw_window(cx: f32, cy: f32, w: f32, h: f32) {
    draw_frame(cx - w / 2.0, cy - h / 2.0, w, h, &WINDOW_FRAME, BUTTON_PALE_COLOR);
}

/// The height at which the capitals of a line of text whose top is `top` centre.
pub fn caps_middle(top: f32) -> f32 {
    top + WHEEL_FONT_H / 2.0 + WHEEL_FONT_CAPS_DY
}

/// The top of a line of text whose capitals centre between `top` and `bottom`.
pub fn band_text_top(top: f32, bottom: f32) -> f32 {
    (top + bottom) / 2.0 - caps_middle(0.0)
}

impl Layer {
    /// Draws the overlay under the game's pointer; returns whether the pointer and its tooltip
    /// should be drawn, as they would cover an open wheel or the help.
    pub fn draw_overlay(&mut self) -> bool {
        if !self.wheel_at_cursor() && !self.help.shown && self.cfg.overlay {
            if self.cfg.crew_marker {
                self.draw_crew_marks();
            }
            self.draw_focus();
        }
        if self.wheel.open.is_some() {
            self.draw_wheel();
        }
        self.draw_touch();
        if let Some((x, y)) = self.pause_button() {
            draw_pause_button(x, y);
        }
        self.draw_badge();
        if self.help.shown && self.cursor.synced {
            self.draw_help();
        }
        self.wheel.open.is_none() && !self.help.shown
    }

    /// The top right corner of what is shown.
    fn top_right(&self) -> (f32, f32) {
        self.zoom.view.map_or((GAME_W as f32, 0.0), |v| (v.right(), v.y))
    }

    /// The pause button's place in the bottom right corner of the ship view's columns.
    fn pause_corner(&self) -> (f32, f32) {
        let inset = PAUSE_MARGIN + PAUSE_BUTTON_SIZE;
        (self.zoom.span.right - inset, GAME_H as f32 - inset)
    }

    /// Whether the view shows the pause button's place in the frame's corner.
    fn corner_shown(&self) -> bool {
        let (x, y) = self.pause_corner();
        self.zoom.view.is_none_or(|v| x + PAUSE_BUTTON_SIZE <= v.right() && y + PAUSE_BUTTON_SIZE <= v.bottom())
    }

    /// Where the pause button sits while it is on and touch is in use, on the ship view with
    /// nothing over it: in the frame's corner, else in the top right of the view.
    fn pause_button(&self) -> Option<(f32, f32)> {
        let shown = self.cfg.pause_button && self.touch_in_use() && state::ship_view();
        if !shown || self.wheel.open.is_some() || self.help.shown {
            return None;
        }
        if self.corner_shown() {
            return Some(self.pause_corner());
        }
        let (right, top) = self.top_right();
        Some((right - PAUSE_MARGIN - PAUSE_BUTTON_SIZE, top + PAUSE_MARGIN))
    }

    /// Whether a touch at frame point (`x`, `y`) reaches the pause button.
    pub fn pause_button_hit(&self, (x, y): (f32, f32)) -> bool {
        self.pause_button().is_some_and(|(bx, by)| {
            let reach = |v: f32, from: f32| v >= from - PAUSE_HIT_PAD && v < from + PAUSE_BUTTON_SIZE + PAUSE_HIT_PAD;
            reach(x, bx) && reach(y, by)
        })
    }

    /// The zone focus brackets frame while the overlay shows them, and the seconds since its
    /// focus move.
    fn shown_focus(&self, now: f64) -> Option<(usize, f64)> {
        self.brackets_shown().then(|| self.focused_zone(now)).flatten()
    }

    fn brackets_shown(&self) -> bool {
        self.cfg.overlay && self.cfg.focus_brackets && !self.wheel_at_cursor() && !self.help.shown
    }

    /// Whether the game's plain pointer is left undrawn: touch or focus brackets mark its point,
    /// or a held focus waits for the first item of a screen that opened or for the jump to end.
    pub fn pointer_hidden(&self) -> bool {
        let waiting = self.first_focus_pending() || state::player_jumping();
        let awaiting = self.brackets_shown() && self.focus_held() && waiting;
        self.touch_in_use() || self.shown_focus(now_s()).is_some() || awaiting
    }

    /// The focused zone whose brackets are drawn: none on the player's ship while it jumps
    /// away, though the pointer stays hidden.
    fn framed_focus(&self, now: f64) -> Option<(usize, f64)> {
        let on_ship = |i: usize| matches!(self.zones.list[i].kind, ZoneKind::Room | ZoneKind::Door);
        self.shown_focus(now).filter(|&(i, _)| !(on_ship(i) && state::player_jumping()))
    }

    /// Frames the zone a focus move left the cursor on with corner brackets.
    fn draw_focus(&self) {
        let now = now_s();
        let Some((i, age)) = self.framed_focus(now) else {
            return;
        };
        let zone = &self.zones.list[i];
        let r = zone.rect;
        let (w, h) = (r.w as f32, r.h as f32);
        let k = self.mark_scale();
        let arm = clampf(w.min(h) * FOCUS_ARM_SHARE, FOCUS_ARM_MIN, FOCUS_ARM_MAX).round() as i32;
        let bracket = Bracket { arm, thick: FOCUS_THICK, cut: FOCUS_CUT };
        let settle = (age / FOCUS_SETTLE_S).min(1.0) as f32;
        let out = (FOCUS_GAP + FOCUS_THICK as f32 + FOCUS_SLIDE * (1.0 - settle)) * k;
        let pad = if zone.kind == ZoneKind::Choice { FOCUS_TEXT_PAD * k } else { 0.0 };
        let (x, y) = (r.x as f32 - out - pad, r.y as f32 - out);
        let around = (x, y, w + 2.0 * (out + pad), h + 2.0 * out);
        draw_brackets(around, bracket, k, settle < 1.0, BUTTON_SELECTED_COLOR);
    }

    /// Marks each selected crew member on the ship view with a triangle over it.
    fn draw_crew_marks(&self) {
        let (Some(gui), Some(selected)) = (state::gui(), state::selected_crew()) else {
            return;
        };
        if !state::ship_view() || state::player_jumping() {
            return;
        }
        let k = self.mark_scale().min(CREW_MARK_SCALE_MAX);
        let phase = (now_s() / CREW_MARK_BOB_S).fract() as f32 * std::f32::consts::TAU;
        let bob = CREW_MARK_BOB_PX * k * phase.sin();
        for (_, member) in selected.vec_objs().filter(|(_, m)| !state::crew_dead(*m)) {
            let player = member.i32(CREW_SHIP) == PLAYER_SHIP_ID;
            let origin = if player { state::player_origin(gui) } else { state::enemy_origin(gui) };
            let x = origin.x as f32 + member.f32(CREW_X);
            let tip = origin.y as f32 + member.f32(CREW_Y) - CREW_MARK_RISE + bob;
            draw_marker(x, tip, CREW_MARK, k, CREW_SELECTED_COLOR);
        }
    }

    /// Draws what lies over the game's pointer and tooltip: the notice.
    pub fn draw_overlay_top(&self) {
        if let Some(notice) = self.notice.as_ref().filter(|_| self.notice_shown()) {
            self.draw_notice(notice.lines);
        }
    }

    /// Whether the notice is on screen: the help covers it.
    pub fn notice_shown(&self) -> bool {
        self.cursor.synced && !self.help.shown
    }

    /// Counts the notice's time down while it is on screen; drops it on a press or when its
    /// time is up.
    pub fn age_notice(&mut self, dt: f32, pressed: bool) {
        let shown = self.notice_shown();
        let Some(notice) = &mut self.notice else {
            return;
        };
        let done = match &mut notice.left {
            Some(left) => {
                if shown {
                    *left -= dt;
                }
                pressed || *left <= 0.0
            }
            None => pressed,
        };
        if done {
            self.notice = None;
        }
    }

    /// Draws a notice in the middle of what is shown, at the text size as far as it fits.
    fn draw_notice(&self, lines: &[&str]) {
        let w = lines.iter().map(|l| text_width(HELP_FONT, l)).fold(0.0, f32::max) + 2.0 * HELP_PAD;
        let h = HELP_LINE * lines.len() as f32 + 2.0 * HELP_PAD;
        let (x, y, area_w, area_h) =
            self.zoom.view.map_or((0.0, 0.0, GAME_W as f32, GAME_H as f32), |v| (v.x, v.y, v.w, v.h));
        let k = panel_scale(text::scale(), w, h, (area_w, area_h));
        let (cx, cy) = (x + area_w / 2.0, y + area_h / 2.0);
        draw_window(cx, cy, w * k, h * k);
        with_scale(cx, cy, k, || {
            set_color(INK_COLOR);
            for (i, line) in lines.iter().enumerate() {
                draw_text_center(HELP_FONT, 0.0, -h / 2.0 + HELP_PAD + i as f32 * HELP_LINE + TEXT_DY, line);
            }
            set_color(TEXT_COLOR);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::api::Rect;
    use crate::game::fake::with_game;
    use crate::game::zones::{Zone, ZoneKind};
    use crate::ui::text::TEXT_SCALE_MAX;

    #[test]
    fn a_panel_grows_with_the_text_as_far_as_it_fits() {
        let frame = (GAME_W as f32, GAME_H as f32);
        let h = 400.0;
        let w = 760.0;
        let fit = ((frame.0 - 2.0 * PANEL_MARGIN) / w).min((frame.1 - 2.0 * PANEL_MARGIN) / h);
        assert_eq!(panel_scale(1.0, w, h, frame), 1.0);
        assert_eq!(panel_scale(TEXT_SCALE_MAX, w, h, frame), fit);
        assert_eq!(panel_scale(TEXT_SCALE_MAX, 2.0 * frame.0, h, frame), 1.0);
    }

    #[test]
    fn a_timed_notice_closes_when_its_time_is_up_or_on_a_press() {
        let mut l = Layer::default();
        l.notice = Some(Notice::for_seconds(PAD_LOST_NOTICE, 1.0));
        l.age_notice(2.0, false);
        assert!(l.notice.is_some(), "no time passes while it is not shown");
        l.cursor.synced = true;
        l.age_notice(0.6, false);
        assert!(l.notice.is_some());
        l.age_notice(0.6, false);
        assert!(l.notice.is_none());
        l.notice = Some(Notice::for_seconds(PAD_LOST_NOTICE, 1.0));
        l.age_notice(0.1, true);
        assert!(l.notice.is_none());
    }

    #[test]
    fn focus_brackets_hide_the_plain_pointer_until_the_cursor_moves() {
        with_game(|_| {
            let mut l = Layer::default();
            let zone = |x| Zone { rect: Rect { x, y: 100, w: 40, h: 40 }, id: 0, kind: ZoneKind::Button };
            l.zones.list = vec![zone(100), zone(200)];
            (l.cursor.x, l.cursor.y) = (120.0, 120.0);
            l.send_cursor();
            assert!(!l.pointer_hidden(), "no focus move yet");
            l.focus_move(1.0, 0.0);
            assert!(l.pointer_hidden());
            l.cfg.overlay = false;
            assert!(!l.pointer_hidden(), "no brackets drawn");
            l.cfg.overlay = true;
            l.cfg.focus_brackets = false;
            assert!(!l.pointer_hidden(), "brackets switched off");
            l.cfg.focus_brackets = true;
            (l.cursor.x, l.cursor.y) = (300.0, 300.0);
            l.send_cursor();
            assert!(!l.pointer_hidden(), "the cursor moved on");
        });
    }

    #[test]
    fn a_held_focus_keeps_the_pointer_hidden_while_an_opening_screen_has_no_items() {
        with_game(|g| {
            let mut l = Layer::default();
            (l.cursor.x, l.cursor.y) = (120.0, 120.0);
            l.send_cursor();
            l.mark_focus();
            assert!(!l.pointer_hidden(), "nothing under the focus");
            g.gui.at(GUI_MENU_BOX).set(FOCUS_OPEN, 1u8);
            l.focus_first_on_open();
            assert!(l.pointer_hidden(), "the menu's items awaited");
            (l.cursor.x, l.cursor.y) = (300.0, 300.0);
            l.send_cursor();
            l.focus_first_on_open();
            assert!(!l.pointer_hidden(), "the cursor moved on");
        });
    }

    #[test]
    fn a_jump_away_hides_the_brackets_on_the_ship_and_keeps_the_pointer_hidden() {
        with_game(|g| {
            let mut l = Layer::default();
            let zone = |x, kind| Zone { rect: Rect { x, y: 100, w: 40, h: 40 }, id: 0, kind };
            l.zones.list = vec![zone(100, ZoneKind::Room), zone(200, ZoneKind::Button)];
            (l.cursor.x, l.cursor.y) = (120.0, 120.0);
            l.send_cursor();
            l.mark_focus();
            let manager = g.player_manager();
            manager.set(SHIPMANAGER_JUMPING, 1u8);
            assert!(l.framed_focus(now_s()).is_none() && l.pointer_hidden(), "a room of the jumping ship");
            l.focus_move(1.0, 0.0);
            assert!(l.framed_focus(now_s()).is_some(), "a button stays framed");
            l.focus_move(-1.0, 0.0);
            l.zones.list.clear();
            assert!(l.pointer_hidden(), "a held focus over nothing mid-jump");
            l.zones.list = vec![zone(100, ZoneKind::Room), zone(200, ZoneKind::Button)];
            manager.set(SHIPMANAGER_JUMPING, 0u8);
            assert!(l.framed_focus(now_s()).is_some(), "the room framed again after the jump");
        });
    }
}
