//! The mode badge: what the buttons do now that the game does not show, on the row where FTL
//! writes PAUSED, in the look of its pause banners.
//!
//! A short mode, a weapon waiting for a target or the doors held, shows as the game's own
//! banner for it, the art the PC build ships for touch play, with its touch hint cut off.
//! The rest goes on a line in the outlined header font, pale over a dark edge in a glow of
//! the mode's colour: under the banner, else on the pause row, above PAUSED while the game is
//! paused. A banner shown while paused stands in for PAUSED, which the pause hook then skips,
//! and the line names the pause; without the hook a paused game shows no banner. The badge
//! keeps inside the zoomed view, and stays off while a menu or an event holds the game: its
//! window reaches down to PAUSED.

use std::sync::atomic::{AtomicBool, Ordering};

use super::draw::*;
use super::zoom::View;
use crate::config::layout::truncate_label;
use crate::core::layer::Layer;
use crate::core::util::clampf;
use crate::game::api::{GlColor, Ptr};
use crate::game::offsets::*;
use crate::game::state;

/// Where CommandGui centres its pause banners, in frame pixels: mid-frame, above the weapons.
const PAUSE_CENTER: (f32, f32) = (GAME_W as f32 / 2.0, 558.0);
/// The top of PAUSED's letters, its glow left out.
const PAUSED_TOP: f32 = 537.0;
/// The line keeps this far above PAUSED's letters, and below a banner's cut.
const PAUSED_GAP: f32 = 10.0;
const BANNER_GAP: f32 = 3.0;
/// The least room the badge leaves to the edges of a zoomed view.
const VIEW_MARGIN: f32 = 8.0;
const SEPARATOR: &str = " / ";
const CREW_TITLE: &str = "Crew";
const TARGET_TITLE: &str = "Target";
/// The badge's names for a volley's aim, in place of the armed weapon's: of all weapons, and
/// after the count of picked ones.
const VOLLEY_TITLE: &str = "All weapons";
const WEAPONS_TITLE: &str = "weapons";
const DOORS_TITLE: &str = "Doors";
const FAST_TITLE: &str = "Fast";
const MOUSE_TITLE: &str = "Mouse";
const PAUSED_TITLE: &str = "Paused";
/// The glows of the game's TARGETING and CREW SELECTION banners.
const TARGET_GLOW: GlColor = GlColor::new(0.95, 0.2, 0.2, 1.0);
const CREW_GLOW: GlColor = GlColor::new(1.0, 0.6, 0.2, 1.0);
/// Rows of each banner above its touch hint, targeting then doors, per game language: up to
/// the row before the hint's first. The first entry serves a language not listed.
const BANNER_ROWS: [(&str, [f32; 2]); 10] = [
    ("en", [49.0, 50.0]),
    ("de", [50.0, 56.0]),
    ("es", [50.0, 50.0]),
    ("fr", [50.0, 58.0]),
    ("it", [50.0, 50.0]),
    ("ja", [52.0, 51.0]),
    ("pl", [49.0, 49.0]),
    ("pt", [50.0, 50.0]),
    ("ru", [49.0, 52.0]),
    ("zh-Hans", [52.0, 53.0]),
];

/// Whether CommandGui::RenderPause is hooked, so a banner can stand in for PAUSED.
static PAUSE_HOOKED: AtomicBool = AtomicBool::new(false);

pub fn set_pause_hooked(on: bool) {
    PAUSE_HOOKED.store(on, Ordering::Relaxed);
}

/// A short mode the game has a pause banner for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Banner {
    Targeting,
    Doors,
}

impl Banner {
    const ALL: [Banner; 2] = [Banner::Targeting, Banner::Doors];

    fn image(self) -> &'static str {
        match self {
            Banner::Targeting => "pause_target.png",
            Banner::Doors => "pause_doors.png",
        }
    }
}

/// The rows of `banner` above its touch hint in `language`.
fn banner_rows(language: &str, banner: Banner) -> f32 {
    let rows = BANNER_ROWS.iter().find(|(code, _)| *code == language).unwrap_or(&BANNER_ROWS[0]).1;
    rows[banner as usize]
}

/// The game's banners in its current language: each one's texture and rows above its hint.
#[derive(Default)]
pub struct BannerArt {
    language: Option<String>,
    art: [Option<(Ptr, f32)>; Banner::ALL.len()],
}

/// What the badge's line names: its banner, the pause, and the modes and game objects behind
/// its words.
#[derive(Clone, Copy, Debug, PartialEq)]
struct LineKey {
    banner: Option<Banner>,
    paused: bool,
    aiming: bool,
    armed: usize,
    volley: Option<(usize, bool)>,
    crew: (usize, usize),
    ship_view: bool,
    precision: bool,
    fast_forward: bool,
    dpad_mouse: bool,
}

/// A badge line as built: what it names, its text, width and glow.
struct BuiltLine {
    key: LineKey,
    text: String,
    width: f32,
    glow: GlColor,
}

/// The badge's line as last built.
#[derive(Default)]
pub struct BadgeLine(Option<BuiltLine>);

/// The badge's crew part on the ship view: the selected crew member's name, or how many are
/// selected.
fn crew_part() -> Option<String> {
    let count = state::selected_count();
    if count == 0 || !state::ship_view() {
        return None;
    }
    if count > 1 {
        return Some(format!("{count} {CREW_TITLE}"));
    }
    let member = state::first_selected_crew()?;
    let name = truncate_label(&state::display_text(member.at(CREW_BLUEPRINT + CREWBP_NAME)));
    Some(format!("{CREW_TITLE}: {name}"))
}

/// Where the badge's parts go, in frame pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    /// The banner's top left.
    banner: Option<(f32, f32)>,
    /// The line's centre and top.
    line: Option<(f32, f32)>,
}

/// Places a banner, its width, image height and rows shown, and a line `line_w` wide on the
/// pause row, then moves both into `view` where one is given.
fn place(banner: Option<(f32, f32, f32)>, line_w: Option<f32>, paused: bool, view: Option<View>) -> Placement {
    let (cx, cy) = PAUSE_CENTER;
    let banner_at = banner.map(|(w, h, _)| (cx - (w / 2.0).floor(), cy - (h / 2.0).floor()));
    let line_top = match (banner_at, banner) {
        (Some((_, y)), Some((_, _, rows))) => y + rows + BANNER_GAP,
        _ if paused => PAUSED_TOP - PAUSED_GAP - WHEEL_FONT_H,
        _ => cy - WHEEL_FONT_H / 2.0,
    };
    let mut p = Placement { banner: banner_at, line: line_w.map(|_| (cx, line_top)) };
    let Some(v) = view else {
        return p;
    };
    let mut boxes = Vec::new();
    if let (Some((x, y)), Some((w, _, rows))) = (p.banner, banner) {
        boxes.push((x, y, x + w, y + rows));
    }
    if let (Some((x, y)), Some(w)) = (p.line, line_w) {
        boxes.push((x - w / 2.0 - GLOW_REACH, y - GLOW_REACH, x + w / 2.0 + GLOW_REACH, y + WHEEL_FONT_H + GLOW_REACH));
    }
    let span = |lo: f32, hi: f32, from: f32, to: f32| {
        let (from, to) = (from + VIEW_MARGIN, to - VIEW_MARGIN);
        if hi - lo > to - from {
            (from + to - lo - hi) / 2.0
        } else {
            clampf(0.0, from - lo, to - hi)
        }
    };
    if boxes.is_empty() {
        return p;
    }
    let lo_x = boxes.iter().map(|b| b.0).fold(f32::INFINITY, f32::min);
    let hi_x = boxes.iter().map(|b| b.2).fold(f32::NEG_INFINITY, f32::max);
    let lo_y = boxes.iter().map(|b| b.1).fold(f32::INFINITY, f32::min);
    let hi_y = boxes.iter().map(|b| b.3).fold(f32::NEG_INFINITY, f32::max);
    let (dx, dy) = (span(lo_x, hi_x, v.x, v.right()), span(lo_y, hi_y, v.y, v.bottom()));
    p.banner = p.banner.map(|(x, y)| (x + dx, y + dy));
    p.line = p.line.map(|(x, y)| (x + dx, y + dy));
    p
}

impl Layer {
    /// Looks the banners up anew when the game's language changes.
    pub fn update_banner_art(&mut self) {
        if self.banner_art.language.as_deref().is_some_and(state::language_is) {
            return;
        }
        let language = state::language();
        let mut images = Images::default();
        self.banner_art.art = Banner::ALL.map(|b| images.get(b.image()).map(|t| (t, banner_rows(&language, b))));
        self.banner_art.language = Some(language);
    }

    fn badge_on(&self) -> bool {
        let shown = self.cursor.synced && self.cfg.badge && self.wheel.open.is_none();
        shown && state::gui().is_some() && !state::menu_paused()
    }

    /// The short mode on, a weapon waiting for a target before the doors held.
    fn banner_mode(&self) -> Option<Banner> {
        if state::mouse().i32(MOUSE_AIMING) != 0 {
            Some(Banner::Targeting)
        } else if self.bind.precision {
            Some(Banner::Doors)
        } else {
            None
        }
    }

    /// The banner the badge shows, with its texture and rows: none while paused without the
    /// pause hook, nor where the game lacks the art.
    fn shown_banner(&self) -> Option<(Banner, Ptr, f32)> {
        if !self.badge_on() || (state::game_paused() && !PAUSE_HOOKED.load(Ordering::Relaxed)) {
            return None;
        }
        let banner = self.banner_mode()?;
        let (texture, rows) = self.banner_art.art[banner as usize]?;
        Some((banner, texture, rows))
    }

    /// Whether a banner stands in for the game's PAUSED this frame.
    pub fn banner_hides_pause(&self) -> bool {
        state::game_paused() && self.shown_banner().is_some()
    }

    /// The banner the badge shows and its line.
    pub fn badge_parts(&self) -> (Option<Banner>, String) {
        let banner = self.shown_banner().map(|(b, ..)| b);
        (banner, self.badge_line(banner, state::game_paused()))
    }

    /// The badge's line: the modes `banner` leaves out, each named, then the pause when the
    /// banner stands in for PAUSED; empty in the plain state.
    pub fn badge_line(&self, banner: Option<Banner>, paused: bool) -> String {
        let mut parts = Vec::new();
        if state::mouse().i32(MOUSE_AIMING) != 0 {
            let weapon = match &self.actions.volley {
                Some(v) if v.all => VOLLEY_TITLE.to_string(),
                Some(v) => format!("{} {WEAPONS_TITLE}", v.slots.len()),
                None => state::weapon_control()
                    .and_then(|c| c.obj(WEAPONCTRL_ARMED))
                    .and_then(|w| w.obj(WEAPON_BLUEPRINT))
                    .map(|bp| truncate_label(&state::display_text(bp.at(BLUEPRINT_SHORT_TITLE))))
                    .unwrap_or_default(),
            };
            match (banner, weapon.is_empty()) {
                (Some(Banner::Targeting), true) => {}
                (Some(Banner::Targeting), false) => parts.push(weapon),
                (_, true) => parts.push(TARGET_TITLE.to_string()),
                (_, false) => parts.push(format!("{TARGET_TITLE}: {weapon}")),
            }
        }
        parts.extend(crew_part());
        let modes = [
            (self.bind.precision && banner != Some(Banner::Doors), DOORS_TITLE),
            (self.actions.fast_forward, FAST_TITLE),
            (self.bind.dpad_mouse, MOUSE_TITLE),
            (banner.is_some() && paused, PAUSED_TITLE),
        ];
        parts.extend(modes.iter().filter(|(on, _)| *on).map(|(_, name)| name.to_string()));
        parts.join(SEPARATOR).to_uppercase()
    }

    /// The line's glow: the colour of the game's banner for the mode it leads with.
    fn line_glow(&self) -> GlColor {
        if state::mouse().i32(MOUSE_AIMING) != 0 {
            TARGET_GLOW
        } else if self.bind.precision {
            FRAME_GLOW_COLOR
        } else if crew_part().is_some() {
            CREW_GLOW
        } else {
            FRAME_GLOW_COLOR
        }
    }

    /// What a line with `banner` shown, `paused` or not, names now.
    fn line_key(&self, banner: Option<Banner>, paused: bool) -> LineKey {
        LineKey {
            banner,
            paused,
            aiming: state::mouse().i32(MOUSE_AIMING) != 0,
            armed: state::weapon_control().and_then(|c| c.obj(WEAPONCTRL_ARMED)).map_or(0, |w| w.addr()),
            volley: self.actions.volley.as_ref().map(|v| (v.slots.len(), v.all)),
            crew: (state::selected_count(), state::first_selected_crew().map_or(0, |c| c.addr())),
            ship_view: state::ship_view(),
            precision: self.bind.precision,
            fast_forward: self.actions.fast_forward,
            dpad_mouse: self.bind.dpad_mouse,
        }
    }

    /// Draws the badge, its line built anew only when what it names changes.
    pub fn draw_badge(&mut self) {
        if !self.badge_on() {
            return;
        }
        let paused = state::game_paused();
        let shown = self.shown_banner();
        let banner = shown.map(|(b, ..)| b);
        let key = self.line_key(banner, paused);
        let line = match self.badge.0.take() {
            Some(line) if line.key == key => line,
            _ => {
                let text = self.badge_line(banner, paused);
                let width = if text.is_empty() { 0.0 } else { text_width(WHEEL_FONT, &text) };
                BuiltLine { key, text, width, glow: self.line_glow() }
            }
        };
        let line_w = (!line.text.is_empty()).then_some(line.width);
        let size = shown.map(|(_, t, rows)| {
            let (w, h) = texture_size(t);
            (w, h, rows)
        });
        let p = place(size, line_w, paused, self.zoom.view);
        if let (Some((_, texture, rows)), Some((x, y))) = (shown, p.banner) {
            draw_image_top(texture, x, y, rows);
        }
        if let Some((x, y)) = p.line {
            draw_text_glow(WHEEL_FONT, x, y, &line.text, line.glow);
        }
        self.badge.0 = Some(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::layout::LABEL_CHARS;
    use crate::game::actions::Volley;
    use crate::game::fake::{with_game, WEAPON_SIZE};

    const BANNER: (f32, f32, f32) = (264.0, 72.0, 49.0);
    const LINE_W: f32 = 200.0;

    fn view(x: f32, y: f32, w: f32, h: f32) -> View {
        View { x, y, w, h, k: 1.0 }
    }

    #[test]
    fn banners_cut_at_their_language_hint_and_an_unknown_one_takes_englishs() {
        assert_eq!(banner_rows("en", Banner::Targeting), 49.0);
        assert_eq!(banner_rows("fr", Banner::Doors), 58.0);
        assert_eq!(banner_rows("zh-Hans", Banner::Targeting), 52.0);
        assert_eq!(banner_rows("", Banner::Doors), banner_rows("en", Banner::Doors));
        assert_eq!(banner_rows("xx", Banner::Targeting), banner_rows("en", Banner::Targeting));
    }

    #[test]
    fn the_badge_sits_on_the_pause_row_and_under_its_banner() {
        let (cx, cy) = PAUSE_CENTER;
        let p = place(Some(BANNER), Some(LINE_W), false, None);
        let top = cy - BANNER.1 / 2.0;
        assert_eq!(p.banner, Some((cx - BANNER.0 / 2.0, top)), "centred as the game centres it");
        assert_eq!(p.line, Some((cx, top + BANNER.2 + BANNER_GAP)), "under the banner's cut");
        let p = place(None, Some(LINE_W), false, None);
        assert_eq!(p.line, Some((cx, cy - WHEEL_FONT_H / 2.0)), "on the pause row");
        let p = place(None, Some(LINE_W), true, None);
        assert_eq!(p.line, Some((cx, PAUSED_TOP - PAUSED_GAP - WHEEL_FONT_H)), "above PAUSED");
        assert_eq!(place(None, None, true, None), Placement { banner: None, line: None });
    }

    #[test]
    fn the_badge_keeps_inside_a_zoomed_view() {
        let shown = view(0.0, 0.0, 600.0, 450.0);
        let p = place(Some(BANNER), Some(LINE_W), false, Some(shown));
        let ((bx, by), (lx, ly)) = (p.banner.expect("a banner was given"), p.line.expect("a line was given"));
        assert!(bx + BANNER.0 <= shown.right() - VIEW_MARGIN, "banner {bx}");
        assert!(ly + WHEEL_FONT_H + GLOW_REACH <= shown.bottom() - VIEW_MARGIN, "line {ly}");
        assert_eq!(lx - bx, BANNER.0 / 2.0, "the parts move together");
        assert_eq!(ly - by, BANNER.2 + BANNER_GAP);
        let whole = view(0.0, 0.0, GAME_W as f32, GAME_H as f32);
        assert_eq!(
            place(Some(BANNER), Some(LINE_W), false, Some(whole)),
            place(Some(BANNER), Some(LINE_W), false, None)
        );
    }

    #[test]
    fn the_line_names_the_selected_crew_on_the_ship_view() {
        with_game(|g| {
            let l = Layer::default();
            let ann = g.crew("Ann", "human", PLAYER_SHIP_ID, false);
            let bob = g.crew("Bob", "human", PLAYER_SHIP_ID, false);
            g.arena.ptr_vector(g.gui, GUI_CREW_CONTROL + CREW_SELECTED, &[Some(ann)]);
            assert_eq!(l.badge_line(None, false), "", "not on the ship view");
            g.player_manager();
            assert_eq!(l.badge_line(None, false), "CREW: ANN");
            g.arena.ptr_vector(g.gui, GUI_CREW_CONTROL + CREW_SELECTED, &[Some(ann), Some(bob)]);
            assert_eq!(l.badge_line(None, false), "2 CREW");
        });
    }

    #[test]
    fn the_line_names_the_armed_weapon_and_the_modes_its_banner_leaves_out() {
        with_game(|g| {
            let mut l = Layer::default();
            assert_eq!(l.badge_line(None, false), "");
            assert_eq!(l.banner_mode(), None);
            (l.bind.precision, l.bind.dpad_mouse) = (true, true);
            assert_eq!(l.banner_mode(), Some(Banner::Doors));
            assert_eq!(l.badge_line(None, false), "DOORS / MOUSE");
            assert_eq!(l.badge_line(Some(Banner::Doors), false), "MOUSE");
            assert_eq!(l.badge_line(Some(Banner::Doors), true), "MOUSE / PAUSED", "the banner hides PAUSED");
            g.mouse.set(MOUSE_AIMING, 1i32);
            assert_eq!(l.banner_mode(), Some(Banner::Targeting), "a target before the doors");
            assert_eq!(l.badge_line(None, false), "TARGET / DOORS / MOUSE");
            assert_eq!(l.badge_line(Some(Banner::Targeting), false), "DOORS / MOUSE");
            let title = "Heavy Laser Mark Two Prototype";
            let bp = g.arena.alloc(BLUEPRINT_SHORT_TITLE + TEXTSTRING_LITERAL + size_of::<u8>());
            g.literal(bp, BLUEPRINT_SHORT_TITLE, title);
            let weapon = g.arena.alloc(WEAPON_SIZE);
            weapon.set(WEAPON_BLUEPRINT, bp.addr());
            g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL).set(WEAPONCTRL_ARMED, weapon.addr());
            let name = title.chars().take(LABEL_CHARS).collect::<String>().to_uppercase();
            assert_eq!(l.badge_line(None, false), format!("TARGET: {name} / DOORS / MOUSE"));
            assert_eq!(l.badge_line(Some(Banner::Targeting), true), format!("{name} / DOORS / MOUSE / PAUSED"));
            let mut volley = Volley::default();
            volley.slots = vec![0, 2];
            l.actions.volley = Some(volley);
            assert_eq!(l.badge_line(None, false), "TARGET: 2 WEAPONS / DOORS / MOUSE", "a volley of picked weapons");
            l.actions.volley.as_mut().expect("set above").all = true;
            assert_eq!(l.badge_line(None, false), "TARGET: ALL WEAPONS / DOORS / MOUSE");
        });
    }

    #[test]
    fn the_badge_builds_its_line_anew_only_when_what_it_names_changes() {
        with_game(|g| {
            let mut l = Layer::default();
            l.cursor.synced = true;
            g.player_manager();
            let text = |l: &Layer| l.badge.0.as_ref().map(|b| b.text.clone());
            let ann = g.crew("Ann", "human", PLAYER_SHIP_ID, false);
            let bob = g.crew("Bob", "human", PLAYER_SHIP_ID, false);
            g.arena.ptr_vector(g.gui, GUI_CREW_CONTROL + CREW_SELECTED, &[Some(ann)]);
            l.draw_badge();
            assert_eq!(text(&l).as_deref(), Some("CREW: ANN"));
            g.literal(ann, CREW_BLUEPRINT + CREWBP_NAME, "Zed");
            l.draw_badge();
            assert_eq!(text(&l).as_deref(), Some("CREW: ANN"), "the same member");
            g.arena.ptr_vector(g.gui, GUI_CREW_CONTROL + CREW_SELECTED, &[Some(bob)]);
            l.draw_badge();
            assert_eq!(text(&l).as_deref(), Some("CREW: BOB"));
            l.bind.dpad_mouse = true;
            l.draw_badge();
            assert_eq!(text(&l).as_deref(), Some("CREW: BOB / MOUSE"));
        });
    }

    #[test]
    fn a_banner_stands_in_for_pause_only_with_the_pause_hook() {
        with_game(|g| {
            let mut l = Layer::default();
            (l.cursor.synced, l.bind.precision) = (true, true);
            l.banner_art.art = [None, Some((std::ptr::dangling_mut(), 50.0))];
            assert!(l.shown_banner().is_some());
            assert!(!l.banner_hides_pause(), "running");
            g.gui.set(layout().gui_paused, true);
            set_pause_hooked(false);
            assert!(l.shown_banner().is_none(), "PAUSED stays, so no banner over it");
            set_pause_hooked(true);
            assert!(l.banner_hides_pause());
            g.gui.set(layout().gui_menu_paused, true);
            assert!(l.shown_banner().is_none(), "an event holds the game");
            assert!(!l.banner_hides_pause());
            l.cfg.badge = false;
            assert!(!l.banner_hides_pause(), "badge off");
        });
    }
}
