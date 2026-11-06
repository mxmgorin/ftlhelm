//! The zones the D-pad walks, rebuilt each frame for the screen on top, and the buttons the
//! game's hit-test reports between frames.

use std::sync::Mutex;

use super::api::{Point, Rect};
use super::mem::Obj;
use super::offsets::*;
use super::state;
use crate::core::layer::Layer;
use crate::core::util::log;
use crate::ui::text;

const DOOR_HALF: i32 = 10;
/// Half the side of the square a beacon counts as.
const BEACON_HALF: i32 = 12;
/// The most rooms of a ship in the game: the rebel flagship's.
const MAX_SHIP_ROOMS: usize = 21;
const MAX_BUTTONS: usize = 64;
/// Every zone of the busiest screen: a fight's rooms on both ships, the armament boxes and the
/// buttons.
const MAX_ZONES: usize = 2 * MAX_SHIP_ROOMS + (WEAPON_SLOTS + DRONE_SLOTS) as usize + MAX_BUTTONS;
/// Small unplaced buttons the game parks in this corner.
const CORNER_BUTTON_MAX: i32 = 40;
const CORNER_BUTTON_SIZE: i32 = 24;
/// A box pointer field of a screen and the flag that shows it.
type ShownBox = (usize, usize);
/// Equipment's overflow boxes.
const EQUIP_OVERFLOW: [ShownBox; 2] =
    [(EQUIP_OVERCAPACITY_BOX, EQUIP_OVER_CAPACITY), (EQUIP_OVERAUG_BOX, EQUIP_OVER_AUG_CAPACITY)];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZoneKind {
    Room,
    EnemyRoom,
    Beacon,
    Sector,
    Choice,
    Button,
    Door,
    Slot,
    SellBox,
    Armament,
    Achievement,
}

impl ZoneKind {
    pub fn name(self) -> &'static str {
        match self {
            ZoneKind::Room => "player room",
            ZoneKind::EnemyRoom => "enemy room",
            ZoneKind::Beacon => "beacon",
            ZoneKind::Sector => "sector",
            ZoneKind::Choice => "choice",
            ZoneKind::Button => "button",
            ZoneKind::Door => "door",
            ZoneKind::Slot => "slot",
            ZoneKind::SellBox => "sell box",
            ZoneKind::Armament => "armament box",
            ZoneKind::Achievement => "achievement",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Zone {
    pub rect: Rect,
    /// Room id, or the index in the list the zone came from.
    pub id: i32,
    pub kind: ZoneKind,
}

/// ShipAchievementInfo as the game lays it out.
#[repr(C)]
#[derive(Clone, Copy)]
struct AchievementIcon {
    achievement: usize,
    at: Point,
    side: i32,
}

const _: () = assert!(std::mem::size_of::<AchievementIcon>() == ACHIEVEMENT_INFO_SIZE);

/// A score screen achievement as the game lays it out: the achievement and its icon's corner.
#[repr(C)]
#[derive(Clone, Copy)]
struct ScoreIcon {
    achievement: usize,
    at: Point,
}

/// A button the game hit-tested: its box on screen and the object's address.
#[derive(Clone, Copy, Debug)]
pub struct Button {
    pub rect: Rect,
    pub object: usize,
}

#[derive(Default)]
pub struct Zones {
    pub list: Vec<Zone>,
    /// The last whole set of buttons the game hit-tested.
    pub buttons: Vec<Button>,
    /// Zones the build under way left out past MAX_ZONES, and whether the last set of buttons
    /// ran out of room.
    dropped: (usize, bool),
    /// `dropped` as last logged.
    logged: (usize, bool),
}

/// Buttons the game hit-tested since the last frame, whether it tested any, whether one
/// found no room, and where they may sit, resolved at the first of them.
struct Seen {
    buttons: Vec<Button>,
    tested: bool,
    full: bool,
    containers: Option<Containers>,
}

static SEEN: Mutex<Seen> = Mutex::new(Seen { buttons: Vec::new(), tested: false, full: false, containers: None });

/// What holds hit-tested buttons, as the address and size of each: the crew control, whose
/// buttons are no zones; the armament bars and the star map, which keep their buttons' boxes
/// relative to the point beside each.
#[derive(Clone, Copy)]
struct Containers {
    crew: Option<(usize, usize)>,
    moved: [Option<(usize, usize, Point)>; 3],
    /// Whether the game draws its menu moved, as while the tutorial runs.
    menu_moved: bool,
}

impl Containers {
    fn now() -> Containers {
        let moving = |holder: Option<Obj>, size, at| holder.map(|h| (h.addr(), size, h.point(at)));
        Containers {
            crew: state::gui().map(|g| (g.at(GUI_CREW_CONTROL).addr(), CREW_CONTROL_SIZE)),
            moved: [
                moving(state::weapon_control(), WEAPON_CONTROL_SIZE, ARMAMENT_LOCATION),
                moving(state::drone_control(), DRONE_CONTROL_SIZE, ARMAMENT_LOCATION),
                moving(state::star_map(), STARMAP_SIZE, STARMAP_POSITION),
            ],
            menu_moved: state::tutorial_running(),
        }
    }
}

/// Records a button the game hit-tests, with its box moved to the screen. Touches only the
/// seen set: a hook may hold the layer meanwhile.
pub fn record_button(button: Obj) {
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    seen.tested = true;
    let c = *seen.containers.get_or_insert_with(Containers::now);
    let held = |(start, size): (usize, usize)| (start..start + size).contains(&button.addr());
    let in_crew_control = c.crew.is_some_and(held);
    let drawn = !button.flag(BUTTON_RENDER_OFF);
    if !button.flag(BUTTON_ACTIVE)
        || !drawn
        || in_crew_control
        || seen.buttons.iter().any(|b| b.object == button.addr())
    {
        return;
    }
    if seen.buttons.len() >= MAX_BUTTONS {
        seen.full = true;
        return;
    }
    let mut r = button.rect(BUTTON_HITBOX);
    for (_, _, at) in c.moved.into_iter().flatten().filter(|&(start, size, _)| held((start, size))) {
        r.x += at.x;
        r.y += at.y;
    }
    if c.menu_moved && state::gui().is_some_and(|g| in_menu(g.at(GUI_MENU_BOX), button)) {
        r.x += MENU_TUTORIAL_SHIFT;
    }
    let corner = r.x < CORNER_BUTTON_MAX && r.y < CORNER_BUTTON_MAX && r.w <= CORNER_BUTTON_SIZE;
    if r.w > 0 && r.h > 0 && !corner && r.x + r.w <= GAME_W && r.y + r.h <= GAME_H {
        seen.buttons.push(Button { rect: r, object: button.addr() });
    }
}

/// Whether `button` is the game menu's own: in it, or in its button list.
fn in_menu(menu: Obj, button: Obj) -> bool {
    menu.contains(MENU_SIZE, button.addr()) || menu.at(MENU_BUTTONS).vec_objs().any(|(_, b)| b == button)
}

/// The pixels strictly inside the `w` by `h` span from `at`, as the game's hit tests take them.
fn open_rect(at: Point, w: i32, h: i32) -> Rect {
    Rect { x: at.x + 1, y: at.y + 1, w: w - 1, h: h - 1 }
}

/// A HUD box that takes clicks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudBox {
    pub rect: Rect,
    /// An armament box's item powered on or not; None for a system icon, whose power runs in
    /// bars.
    pub powered: Option<bool>,
}

/// The ship view's HUD boxes: the system icons and the armament boxes holding an item.
pub fn hud_boxes() -> Vec<HudBox> {
    let Some(gui) = state::gui() else {
        return Vec::new();
    };
    let control = gui.at(GUI_SYS_CONTROL);
    let origin = control.point(SYSCTRL_POSITION);
    let side = SYSBOX_CLICK_TO - SYSBOX_CLICK_FROM;
    let mut boxes: Vec<HudBox> = control
        .at(SYSCTRL_BOXES)
        .vec_objs()
        .map(|(_, b)| {
            let at = b.point(SYSBOX_LOCATION);
            let from = Point { x: origin.x + at.x + SYSBOX_CLICK_FROM, y: origin.y + at.y + SYSBOX_CLICK_FROM };
            HudBox { rect: open_rect(from, side, side), powered: None }
        })
        .collect();
    boxes.extend(armament_boxes().map(|(rect, _, powered)| HudBox { rect, powered: Some(powered) }));
    boxes
}

/// The weapon boxes, then the drone boxes, that hold an item: each box on screen, its slot
/// counted on from the weapon slots for a drone, and whether its item is powered.
fn armament_boxes() -> impl Iterator<Item = (Rect, usize, bool)> {
    let bars =
        [(state::weapon_control(), WEAPON_POWERED, 0), (state::drone_control(), DRONE_POWERED, WEAPON_SLOTS as usize)];
    bars.into_iter().filter_map(|(bar, powered, first)| Some((bar?, powered, first))).flat_map(
        |(bar, powered, first)| {
            let at = bar.point(ARMAMENT_LOCATION);
            bar.at(ARMAMENT_BOXES).vec_objs().filter_map(move |(slot, b)| {
                let item = b.obj(ARMAMENTBOX_ITEM)?;
                let hit = b.point(ARMAMENTBOX_HIT);
                let rect = open_rect(Point { x: at.x + hit.x, y: at.y + hit.y }, ARMAMENTBOX_HIT_W, ARMAMENTBOX_HIT_H);
                Some((rect, first + slot, item.flag(powered)))
            })
        },
    )
}

/// Bounding box centre of a ship's rooms on screen.
pub fn ship_center(ship: Obj, origin: Point) -> Option<(f32, f32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (GAME_W, GAME_H, 0, 0);
    let mut any = false;
    for (_, room) in ship.at(SHIP_ROOMS).vec_objs() {
        let r = room.rect(ROOM_RECT);
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.x + r.w);
        y1 = y1.max(r.y + r.h);
        any = true;
    }
    (any && x1 > x0).then(|| (origin.x as f32 + (x0 + x1) as f32 / 2.0, origin.y as f32 + (y0 + y1) as f32 / 2.0))
}

impl Zones {
    fn push(&mut self, rect: Rect, id: i32, kind: ZoneKind) {
        if self.list.len() < MAX_ZONES {
            self.list.push(Zone { rect, id, kind });
        } else {
            self.dropped.0 += 1;
        }
    }

    /// Logs what the last build left out when it changed, and starts a new count.
    fn report_dropped(&mut self) {
        if self.dropped != self.logged {
            let (zones, full) = self.dropped;
            log!("zones: {zones} left out past MAX_ZONES, buttons full {}", full as i32);
            self.logged = self.dropped;
        }
        self.dropped.0 = 0;
    }

    fn add_door_list(&mut self, doors: Obj, origin: Point) {
        for (i, door) in doors.vec_objs() {
            let (x, y) = (origin.x + door.i32(DOOR_X), origin.y + door.i32(DOOR_Y));
            let rect = Rect { x: x - DOOR_HALF, y: y - DOOR_HALF, w: 2 * DOOR_HALF, h: 2 * DOOR_HALF };
            self.push(rect, i as i32, ZoneKind::Door);
        }
    }

    /// Inner doors and the airlocks on the hull.
    fn add_doors(&mut self, ship: Obj, origin: Point) {
        self.add_door_list(ship.at(SHIP_DOORS), origin);
        self.add_door_list(ship.at(SHIP_AIRLOCKS), origin);
    }

    fn add_armament_zones(&mut self) {
        for (rect, slot, _) in armament_boxes() {
            self.push(rect, slot as i32, ZoneKind::Armament);
        }
    }

    /// The rooms of the player's ship and of the enemy's when it is in reach, the enemy's
    /// first when `enemy_first`.
    fn add_ship_rooms(&mut self, gui: Obj, player: Obj, enemy_first: bool) {
        let ships = [
            (Some(player), state::player_origin(gui), false),
            (state::enemy_ship_in_reach(), state::enemy_origin(gui), true),
        ];
        let order = if enemy_first { [1, 0] } else { [0, 1] };
        for (ship, origin, enemy) in order.map(|i| ships[i]) {
            if let Some(ship) = ship {
                self.add_rooms(ship, origin, enemy);
            }
        }
    }

    fn add_rooms(&mut self, ship: Obj, origin: Point, enemy: bool) {
        for (_, room) in ship.at(SHIP_ROOMS).vec_objs() {
            let mut r = room.rect(ROOM_RECT);
            r.x += origin.x;
            r.y += origin.y;
            self.push(r, room.i32(ROOM_ID), if enemy { ZoneKind::EnemyRoom } else { ZoneKind::Room });
        }
    }

    /// Beacons, or the sector boxes while a new sector is being chosen; false when the map
    /// is closed.
    fn add_map_zones(&mut self, map: Option<Obj>) -> bool {
        let Some(map) = map else {
            return false;
        };
        if map.flag(STARMAP_CHOOSING_SECTOR) {
            let offset = map.point(STARMAP_SECTOR_OFFSET);
            for (i, mut r) in map.at(STARMAP_SECTOR_HITBOXES).vec_items::<Rect>().enumerate() {
                r.x += offset.x;
                r.y += offset.y;
                self.push(r, i as i32, ZoneKind::Sector);
            }
            return true;
        }
        let origin = map.point(STARMAP_POSITION);
        let [tx, ty] = map.get::<[f32; 2]>(STARMAP_TRANSLATION);
        for (i, loc) in map.at(STARMAP_LOCATIONS).vec_objs() {
            let [lx, ly] = loc.get::<[f32; 2]>(LOCATION_LOC);
            let (cx, cy) = (origin.x + (tx + lx).round() as i32, origin.y + (ty + ly).round() as i32);
            let rect = Rect { x: cx - BEACON_HALF, y: cy - BEACON_HALF, w: 2 * BEACON_HALF, h: 2 * BEACON_HALF };
            self.push(rect, i as i32, ZoneKind::Beacon);
        }
        true
    }

    /// The hangar's equipment boxes that hold an item, each cut above the buttons that start
    /// inside it.
    fn add_hangar_boxes(&mut self, hangar: Obj) {
        for (i, b) in hangar.at(HANGAR_EQUIP_BOXES).vec_objs() {
            let held = b.get::<[usize; EQUIPBOX_ITEM_PTRS]>(EQUIPBOX_ITEM).iter().any(|&p| p != 0);
            let r = self.above_buttons(b.rect(EQUIPBOX_HITBOX));
            if held && r.w > 0 && r.h > 0 {
                self.push(r, i as i32, ZoneKind::Slot);
            }
        }
    }

    /// `r` cut above the hit-tested buttons that start inside it.
    fn above_buttons(&self, mut r: Rect) -> Rect {
        let inside = |s: Rect| s.x < r.x + r.w && r.x < s.x + s.w && s.y > r.y && s.y < r.y + r.h;
        r.h = self.buttons.iter().filter(|b| inside(b.rect)).map(|b| b.rect.y - r.y).fold(r.h, i32::min);
        r
    }

    /// The icons of a ship achievement list that hold an achievement.
    fn add_achievements(&mut self, list: Obj) {
        self.add_icons(list.vec_items::<AchievementIcon>().map(|i| (i.achievement, i.at, i.side)));
    }

    /// The icons of a score screen achievement list that hold an achievement.
    fn add_score_achievements(&mut self, list: Obj) {
        self.add_icons(list.vec_items::<ScoreIcon>().map(|i| (i.achievement, i.at, SCORE_ACHIEVEMENT_SIDE)));
    }

    /// The achievements of the score screen's tabs shown; none under its ship list.
    fn add_score_screen(&mut self, score: Obj) {
        if score.at(SCORE_SHIP_LIST).flag(SHIP_LIST_OPEN) {
            return;
        }
        for (tab, list) in
            [(SCORE_ACHIEVEMENTS_TAB, SCORE_ACHIEVEMENTS), (SCORE_SHIP_BEST_TAB, SCORE_SHIP_ACHIEVEMENTS)]
        {
            if score.flag(tab) {
                self.add_score_achievements(score.at(list));
            }
        }
    }

    /// The game over window's buttons, else the score screen's over it; none under the credits.
    fn add_game_over(&mut self, over: Obj) {
        if over.flag(GAME_OVER_CREDITS) {
            return;
        }
        let listed = |list: Obj, object: usize| list.vec_objs().any(|(_, b)| b.addr() == object);
        if let Some(score) = state::score_screen() {
            self.add_score_screen(score);
            let ships = score.at(SCORE_SHIP_LIST + SHIP_LIST_BUTTONS);
            self.add_own_buttons(|o| score.contains(SCORE_SIZE, o) || listed(ships, o));
        } else {
            self.add_own_buttons(|o| listed(over.at(GAME_OVER_BUTTONS), o));
        }
    }

    /// The hit-tested buttons whose object `own` accepts.
    fn add_own_buttons(&mut self, own: impl Fn(usize) -> bool) {
        for i in 0..self.buttons.len() {
            let b = self.buttons[i];
            if own(b.object) {
                self.push(b.rect, i as i32, ZoneKind::Button);
            }
        }
    }

    /// Achievement icons, each its achievement, corner and side, as the game hit-tests them;
    /// those without an achievement left out.
    fn add_icons(&mut self, icons: impl Iterator<Item = (usize, Point, i32)>) {
        for (i, (achievement, at, side)) in icons.enumerate() {
            if achievement != 0 && side > 0 {
                self.push(open_rect(at, side, side), i as i32, ZoneKind::Achievement);
            }
        }
    }

    /// A choice box's choices, through the transform it last drew with.
    fn add_choices(&mut self, choice: Obj) {
        let zoom = text::box_zoom(choice.addr());
        for (i, r) in choice.at(CHOICE_RECTS).vec_items::<Rect>().enumerate() {
            self.push(zoom.map_or(r, |z| z.to_screen(r)), i as i32, ZoneKind::Choice);
        }
    }

    /// The hit-tested buttons: only those of `holder` (its object and size) when given, else
    /// with another screen or the menu open none from the HUD's top and bottom rows.
    /// None from the store page not shown.
    fn add_button_zones(&mut self, screen_open: bool, holder: Option<(Obj, usize)>) {
        let hidden = state::hidden_store_buttons();
        for i in 0..self.buttons.len() {
            let b = self.buttons[i];
            let held = holder.map(|(h, size)| h.contains(size, b.object));
            let in_hud_rows = b.rect.y < HUD_TOP_HEIGHT || b.rect.y >= HUD_BOTTOM_TOP;
            if held == Some(false) || (held.is_none() && screen_open && in_hud_rows) || hidden.contains(&b.object) {
                continue;
            }
            self.push(b.rect, i as i32, ZoneKind::Button);
        }
    }

    /// Equipment and crew slots of an open ship screen, which are not buttons, each cut above
    /// the buttons that start inside it; the overflow boxes only while shown, then the store's
    /// sell box.
    fn add_slot_zones(&mut self, gui: Obj) {
        let screens: [(usize, usize, &[ShownBox]); 2] = [
            (layout().gui_equip_screen, EQUIP_BOXES, &EQUIP_OVERFLOW),
            (layout().gui_crew_screen, CREWSCREEN_BOXES, &[]),
        ];
        for (window, boxes, overflow) in screens {
            let window = gui.at(window);
            if !window.flag(FOCUS_OPEN) {
                continue;
            }
            let hidden: Vec<Obj> = overflow
                .iter()
                .filter(|&&(_, shown)| !window.flag(shown))
                .filter_map(|&(b, _)| window.obj(b))
                .collect();
            for (i, slot) in window.at(boxes).vec_objs().filter(|(_, s)| !hidden.contains(s)) {
                let r = self.above_buttons(slot.rect(EQUIPBOX_HITBOX));
                if r.w > 0 && r.h > 0 {
                    self.push(r, i as i32, ZoneKind::Slot);
                }
            }
        }
        if let Some(r) = sell_box(gui.at(layout().gui_equip_screen)) {
            self.push(r, 0, ZoneKind::SellBox);
        }
    }
}

/// The box the store's sell tab sells an item dropped on, as the game hit-tests it; None
/// off the sell tab.
fn sell_box(equip: Obj) -> Option<Rect> {
    if !equip.flag(FOCUS_OPEN) || !equip.flag(EQUIP_STORE_MODE) {
        return None;
    }
    let sell = equip.at(EQUIP_SELL_BOX);
    let image = sell.obj(DROPBOX_IMAGE)?;
    let at = sell.point(DROPBOX_POSITION);
    let h = image.i32(TEXTURE_H) + sell.i32(DROPBOX_INSERT_HEIGHT) + sell.i32(DROPBOX_TITLE_INSERT);
    Some(Rect { x: at.x, y: at.y, w: image.i32(TEXTURE_W), h })
}

impl Layer {
    /// Rebuilds the zones for the current screen: a dialog's choices alone while it is open
    /// (it sits over any screen but the game menu), beacons or sector boxes with the map
    /// open, otherwise rooms, then the filled armament boxes unless aiming; the buttons the
    /// game hit-tests come on top.
    pub fn build_zones(&mut self) {
        let zones = &mut self.zones;
        zones.report_dropped();
        zones.list.clear();
        {
            let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
            if seen.tested {
                std::mem::swap(&mut zones.buttons, &mut seen.buttons);
                seen.buttons.clear();
                zones.dropped.1 = std::mem::take(&mut seen.full);
                seen.containers = None;
                seen.tested = false;
            }
        }
        // The credits hit-test no buttons: those last seen are the menu's beneath.
        if state::credits_open() {
            return;
        }
        let Some(gui) = state::gui() else {
            return;
        };
        // The game over window takes every click from the GUI behind it.
        if let Some(over) = state::game_over() {
            zones.add_game_over(over);
            return;
        }
        // The GUI keeps the last game's ship behind the main menu.
        let player = state::player_ship().filter(|_| !state::main_menu_open());
        let options = state::open_options();
        let modal_open = options.is_some() || state::modal_window(gui).is_some();
        if let Some(choice) = state::open_choice().filter(|_| !modal_open) {
            zones.add_choices(choice);
            return;
        }
        if let Some((options, choices)) = options.and_then(|o| Some((o, state::options_choices(o)?))) {
            zones.add_choices(choices);
            // The restart notice takes every click.
            if choices != options {
                return;
            }
        }
        if let Some(hangar) = state::hangar().filter(|_| !state::ship_list_open()) {
            zones.add_hangar_boxes(hangar);
            zones.add_achievements(hangar.at(HANGAR_ACHIEVEMENTS));
        }
        // The game hit-tests the menu's achievements only outside the tutorial.
        if let Some(menu) = state::game_menu_alone().filter(|_| !state::tutorial_running()) {
            zones.add_achievements(menu.at(MENU_ACHIEVEMENTS));
        }
        if let Some(score) = state::score_screen() {
            zones.add_score_screen(score);
        }
        let screen_open = state::open_tabbed().is_some() || modal_open;
        let map = state::star_map().filter(|m| m.flag(FOCUS_OPEN));
        let precision = self.bind.precision;
        if let Some(player) = player {
            if !screen_open && !zones.add_map_zones(map) {
                if state::aiming() {
                    zones.add_ship_rooms(gui, player, true);
                } else if precision && !state::any_crew_selected() {
                    zones.add_doors(player, state::player_origin(gui));
                } else {
                    zones.add_ship_rooms(gui, player, false);
                    zones.add_armament_zones();
                }
            }
        }
        if screen_open {
            zones.add_slot_zones(gui);
        }
        if !precision || state::aiming() || state::any_crew_selected() {
            // The hotkey screen covers the HUD, its own buttons standing in the HUD's rows.
            let hotkeys = options.filter(|o| o.flag(OPTIONS_CUSTOMIZE_CONTROLS)).map(|o| (o, layout().options_size()));
            zones.add_button_zones(screen_open, hotkeys.or(map.map(|m| (m, STARMAP_SIZE))));
        }
    }

    /// The zone under the cursor's last reported position.
    pub fn zone_under_cursor(&self) -> Option<usize> {
        self.zones.list.iter().position(|t| t.rect.contains(self.cursor.last_x, self.cursor.last_y))
    }
}

impl Zones {
    /// The armament zone one box along its bar from zone `i`, the next for a positive `dir`,
    /// else the previous; None at the bar's end.
    pub fn armament_beside(&self, i: usize, dir: i32) -> Option<usize> {
        let from = self.list[i].id;
        let to = from + dir.signum();
        if (from < WEAPON_SLOTS) != (to < WEAPON_SLOTS) {
            return None;
        }
        self.list.iter().position(|z| z.kind == ZoneKind::Armament && z.id == to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::fake::{score_keeper, set_tutorial_running, with_game, Game, WEAPON_SIZE};
    use std::mem::size_of;

    const PLAYER_AT: Point = Point { x: 100, y: 50 };
    const COMBAT_AT: Point = Point { x: 600, y: 0 };
    const TARGET_AT: Point = Point { x: 100, y: 60 };
    const ENEMY_AT: Point = Point { x: COMBAT_AT.x + TARGET_AT.x, y: COMBAT_AT.y + TARGET_AT.y };
    const SIDE: i32 = 35;

    fn rect(x: i32, y: i32) -> Rect {
        Rect { x, y, w: SIDE, h: SIDE }
    }

    fn moved(r: Rect, by: Point) -> Rect {
        Rect { x: r.x + by.x, y: r.y + by.y, ..r }
    }

    fn square(center: Point, half: i32) -> Rect {
        Rect { x: center.x - half, y: center.y - half, w: 2 * half, h: 2 * half }
    }

    fn zones(l: &Layer) -> Vec<(ZoneKind, i32, Rect)> {
        l.zones.list.iter().map(|z| (z.kind, z.id, z.rect)).collect()
    }

    /// The player ship with two rooms around a null entry, the enemy with one.
    fn two_ships(g: &mut Game) -> Obj {
        g.gui.set(GUI_SHIP_POSITION, PLAYER_AT);
        let player = g.player_manager().at(SHIPMANAGER_SHIP);
        let rooms = [Some(g.room(3, rect(0, 0))), None, Some(g.room(5, rect(SIDE, 0)))];
        g.arena.ptr_vector(player, SHIP_ROOMS, &rooms);
        let combat = g.gui.at(GUI_COMBAT_CONTROL);
        combat.set(COMBAT_POSITION, COMBAT_AT);
        combat.set(COMBAT_TARGET_POSITION, TARGET_AT);
        let enemy = g.enemy_manager().at(SHIPMANAGER_SHIP);
        let rooms = [Some(g.room(1, rect(0, 0)))];
        g.arena.ptr_vector(enemy, SHIP_ROOMS, &rooms);
        player
    }

    fn open_map(g: &mut Game) -> Obj {
        let map = g.arena.alloc(STARMAP_SIZE);
        g.gui.set(GUI_STAR_MAP, map.addr());
        map.set(FOCUS_OPEN, 1u8);
        map
    }

    fn built(l: &mut Layer) -> Vec<(ZoneKind, i32, Rect)> {
        l.build_zones();
        zones(l)
    }

    #[test]
    fn the_ship_view_lists_player_rooms_then_enemy_rooms() {
        with_game(|g| {
            two_ships(g);
            assert_eq!(
                built(&mut Layer::default()),
                [
                    (ZoneKind::Room, 3, moved(rect(0, 0), PLAYER_AT)),
                    (ZoneKind::Room, 5, moved(rect(SIDE, 0), PLAYER_AT)),
                    (ZoneKind::EnemyRoom, 1, moved(rect(0, 0), ENEMY_AT)),
                ]
            );
        });
    }

    #[test]
    fn a_surrendered_enemy_lists_its_rooms_only_with_the_players_crew_aboard() {
        with_game(|g| {
            two_ships(g);
            let target = g.gui.at(GUI_COMBAT_CONTROL).obj(COMBAT_CURRENT_TARGET);
            let manager = target.and_then(|c| c.obj(COMPLETE_SHIP_MANAGER)).expect("two_ships places an enemy");
            manager.set(SHIPMANAGER_TARGETABLE + TARGETABLE_HOSTILE, 0u8);
            let kinds = |l: &mut Layer| built(l).into_iter().map(|(k, id, _)| (k, id)).collect::<Vec<_>>();
            let mut l = Layer::default();
            assert_eq!(kinds(&mut l), [(ZoneKind::Room, 3), (ZoneKind::Room, 5)]);
            manager.set(SHIPMANAGER_PLAYER_CREW, 1u8);
            assert_eq!(kinds(&mut l), [(ZoneKind::Room, 3), (ZoneKind::Room, 5), (ZoneKind::EnemyRoom, 1)]);
        });
    }

    #[test]
    fn the_other_ship_takes_the_focus_on_its_room_nearest_the_centre() {
        with_game(|g| {
            two_ships(g);
            let mut l = Layer::default();
            l.build_zones();
            let enemy_room = moved(rect(0, 0), ENEMY_AT).center();
            (l.cursor.x, l.cursor.y) = enemy_room;
            let room_at = |l: &Layer| {
                let at = (l.cursor.x as i32, l.cursor.y as i32);
                l.zones
                    .list
                    .iter()
                    .find(|z| z.rect.contains(at.0, at.1))
                    .map(|z| (z.kind, z.rect.center() == (l.cursor.x, l.cursor.y)))
            };
            l.center_swap();
            assert_eq!(room_at(&l), Some((ZoneKind::Room, true)), "a player room, at its centre");
            assert!(l.cursor.focus.is_some());
            l.center_swap();
            assert_eq!((l.cursor.x, l.cursor.y), enemy_room, "back on the enemy's room");
            let mut bare = Layer::default();
            (bare.cursor.x, bare.cursor.y) = enemy_room;
            bare.center_swap();
            let centre = (PLAYER_AT.x as f32 + SIDE as f32, PLAYER_AT.y as f32 + SIDE as f32 / 2.0);
            assert_eq!((bare.cursor.x, bare.cursor.y), centre, "the ship's centre without room zones");
        });
    }

    #[test]
    fn aiming_lists_enemy_rooms_first() {
        with_game(|g| {
            two_ships(g);
            g.mouse.set(MOUSE_AIMING, 1i32);
            let kinds: Vec<_> = built(&mut Layer::default()).into_iter().map(|(k, id, _)| (k, id)).collect();
            assert_eq!(kinds, [(ZoneKind::EnemyRoom, 1), (ZoneKind::Room, 3), (ZoneKind::Room, 5)]);
        });
    }

    #[test]
    fn the_ship_view_lists_the_filled_armament_boxes_after_the_rooms_unless_aiming() {
        with_game(|g| {
            two_ships(g);
            let weapons = g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL);
            weapons.set(ARMAMENT_LOCATION, Point { x: 300, y: 640 });
            let item = g.arena.alloc(WEAPON_SIZE);
            let filled = g.arena.alloc(ARMAMENTBOX_ITEM + size_of::<usize>());
            filled.set(ARMAMENTBOX_ITEM, item.addr());
            let empty = g.arena.alloc(ARMAMENTBOX_ITEM + size_of::<usize>());
            g.arena.ptr_vector(weapons, ARMAMENT_BOXES, &[Some(empty), Some(filled)]);
            let mut l = Layer::default();
            let last = built(&mut l).pop();
            let rect = Rect { x: 301, y: 641, w: ARMAMENTBOX_HIT_W - 1, h: ARMAMENTBOX_HIT_H - 1 };
            assert_eq!(last, Some((ZoneKind::Armament, 1, rect)), "the filled box by its slot");
            g.mouse.set(MOUSE_AIMING, 1i32);
            assert!(built(&mut l).iter().all(|&(k, ..)| k != ZoneKind::Armament));
        });
    }

    #[test]
    fn the_biggest_fight_lists_every_room_armament_box_and_button() {
        with_game(|g| {
            let ships = [g.player_manager(), g.enemy_manager()].map(|m| m.at(SHIPMANAGER_SHIP));
            for ship in ships {
                let rooms: Vec<_> = (0..MAX_SHIP_ROOMS).map(|i| Some(g.room(i as i32, rect(0, 0)))).collect();
                g.arena.ptr_vector(ship, SHIP_ROOMS, &rooms);
            }
            for (bar, slots) in [(COMBAT_WEAPON_CONTROL, WEAPON_SLOTS), (COMBAT_DRONE_CONTROL, DRONE_SLOTS)] {
                let mut boxes = Vec::new();
                for _ in 0..slots {
                    let item = g.arena.alloc(WEAPON_SIZE);
                    let filled = g.arena.alloc(ARMAMENTBOX_ITEM + size_of::<usize>());
                    filled.set(ARMAMENTBOX_ITEM, item.addr());
                    boxes.push(Some(filled));
                }
                g.arena.ptr_vector(g.gui.at(GUI_COMBAT_CONTROL + bar), ARMAMENT_BOXES, &boxes);
            }
            let buttons = (1..=MAX_BUTTONS).map(|object| Button { rect: rect(0, 0), object }).collect();
            *SEEN.lock().unwrap_or_else(|e| e.into_inner()) =
                Seen { buttons, tested: true, full: false, containers: None };
            let mut l = Layer::default();
            let listed = built(&mut l);
            assert_eq!(listed.len(), MAX_ZONES);
            assert_eq!(listed.iter().filter(|&&(k, ..)| k == ZoneKind::Button).count(), MAX_BUTTONS);
        });
    }

    #[test]
    fn hit_tested_buttons_take_their_holders_offsets_and_a_fresh_list_each_frame() {
        // A button's offset inside its holder, clear of the holder's fields the readers use.
        const BUTTON_AT: usize = 256;
        with_game(|g| {
            *SEEN.lock().unwrap_or_else(|e| e.into_inner()) =
                Seen { buttons: Vec::new(), tested: false, full: false, containers: None };
            let bar = g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL);
            let bar_at = |x| Point { x, y: 600 };
            bar.set(ARMAMENT_LOCATION, bar_at(300));
            let button = |b: Obj, r: Rect| {
                b.set(BUTTON_HITBOX, r);
                b.set(BUTTON_ACTIVE, 1u8);
                b
            };
            let plain = button(g.arena.alloc(BUTTON_ACTIVE + size_of::<u8>()), rect(100, 100));
            let in_bar = button(bar.at(BUTTON_AT), rect(10, 5));
            let in_crew = button(g.gui.at(GUI_CREW_CONTROL + BUTTON_AT), rect(200, 200));
            for b in [plain, in_bar, in_crew, plain] {
                record_button(b);
            }
            let mut l = Layer::default();
            l.build_zones();
            let rects = |l: &Layer| l.zones.buttons.iter().map(|b| b.rect).collect::<Vec<_>>();
            assert_eq!(rects(&l), [rect(100, 100), moved(rect(10, 5), bar_at(300))]);
            bar.set(ARMAMENT_LOCATION, bar_at(320));
            record_button(in_bar);
            l.build_zones();
            assert_eq!(rects(&l), [moved(rect(10, 5), bar_at(320))], "the bar read anew");
            let menu = g.gui.at(GUI_MENU_BOX);
            let listed = button(g.arena.alloc(BUTTON_ACTIVE + size_of::<u8>()), rect(431, 231));
            g.arena.ptr_vector(menu, MENU_BUTTONS, &[Some(listed)]);
            let inline = button(menu.at(BUTTON_AT), rect(500, 300));
            let record = |l: &mut Layer| {
                record_button(listed);
                record_button(inline);
                l.build_zones();
            };
            record(&mut l);
            assert_eq!(rects(&l), [rect(431, 231), rect(500, 300)]);
            set_tutorial_running(true);
            record(&mut l);
            let tutorial = Point { x: MENU_TUTORIAL_SHIFT, y: 0 };
            assert_eq!(
                rects(&l),
                [moved(rect(431, 231), tutorial), moved(rect(500, 300), tutorial)],
                "the menu drawn moved"
            );
        });
    }

    #[test]
    fn precision_lists_doors_then_airlocks_by_list_index() {
        with_game(|g| {
            let player = two_ships(g);
            let (d0, d2) = (Point { x: 10, y: 20 }, Point { x: 30, y: 40 });
            let a0 = Point { x: 50, y: 0 };
            let doors = [Some(g.door((3, 5), d0, false)), None, Some(g.door((5, 1), d2, true))];
            g.arena.ptr_vector(player, SHIP_DOORS, &doors);
            let airlocks = [Some(g.door((3, -1), a0, false))];
            g.arena.ptr_vector(player, SHIP_AIRLOCKS, &airlocks);
            let mut l = Layer::default();
            l.bind.precision = true;
            let at = |p: Point| square(Point { x: PLAYER_AT.x + p.x, y: PLAYER_AT.y + p.y }, DOOR_HALF);
            assert_eq!(
                built(&mut l),
                [(ZoneKind::Door, 0, at(d0)), (ZoneKind::Door, 2, at(d2)), (ZoneKind::Door, 0, at(a0))]
            );
        });
    }

    #[test]
    fn the_star_map_lists_its_beacons_and_its_own_buttons() {
        with_game(|g| {
            two_ships(g);
            let map = open_map(g);
            let origin = Point { x: 200, y: 100 };
            map.set(STARMAP_POSITION, origin);
            map.set(STARMAP_TRANSLATION, [10.0f32, 20.0]);
            let locations: Vec<Option<Obj>> = [[30.0f32, 40.0], [100.4, 7.6]]
                .into_iter()
                .map(|xy| {
                    let loc = g.arena.alloc(LOCATION_LOC + size_of::<[f32; 2]>());
                    loc.set(LOCATION_LOC, xy);
                    Some(loc)
                })
                .collect();
            g.arena.ptr_vector(map, STARMAP_LOCATIONS, &locations);
            let mut l = Layer::default();
            let on_map = Button { rect: rect(400, 400), object: map.addr() + size_of::<usize>() };
            let off_map = Button { rect: rect(500, 500), object: g.gui.addr() };
            l.zones.buttons = vec![off_map, on_map];
            let beacon = |x: i32, y: i32| square(Point { x: origin.x + x, y: origin.y + y }, BEACON_HALF);
            assert_eq!(
                built(&mut l),
                [
                    (ZoneKind::Beacon, 0, beacon(40, 60)),
                    (ZoneKind::Beacon, 1, beacon(110, 28)),
                    (ZoneKind::Button, 1, on_map.rect),
                ]
            );
        });
    }

    #[test]
    fn a_sector_choice_lists_the_sector_boxes() {
        with_game(|g| {
            two_ships(g);
            let map = open_map(g);
            map.set(STARMAP_CHOOSING_SECTOR, 1u8);
            let offset = Point { x: 300, y: 200 };
            map.set(STARMAP_SECTOR_OFFSET, offset);
            g.arena.vector(map, STARMAP_SECTOR_HITBOXES, &[rect(0, 0), rect(50, 70)]);
            assert_eq!(
                built(&mut Layer::default()),
                [(ZoneKind::Sector, 0, moved(rect(0, 0), offset)), (ZoneKind::Sector, 1, moved(rect(50, 70), offset))]
            );
        });
    }

    #[test]
    fn an_event_lists_its_choices_alone() {
        with_game(|g| {
            two_ships(g);
            let choice = g.gui.at(GUI_CHOICE_BOX);
            choice.set(FOCUS_OPEN, 1u8);
            g.arena.vector(choice, CHOICE_RECTS, &[rect(400, 300), rect(400, 340)]);
            let mut l = Layer::default();
            l.zones.buttons = vec![Button { rect: rect(10, 200), object: g.gui.addr() }];
            assert_eq!(built(&mut l), [(ZoneKind::Choice, 0, rect(400, 300)), (ZoneKind::Choice, 1, rect(400, 340))]);
        });
    }

    #[test]
    fn the_main_menu_lists_its_own_choices() {
        with_game(|g| {
            let menu = g.app.at(CAPP_MAIN_MENU);
            menu.set(MAIN_MENU_OPEN, 1u8);
            let choice = menu.at(layout().main_menu_choice_box);
            choice.set(FOCUS_OPEN, 1u8);
            g.arena.vector(choice, CHOICE_RECTS, &[rect(500, 300)]);
            assert_eq!(built(&mut Layer::default()), [(ZoneKind::Choice, 0, rect(500, 300))]);
        });
    }

    #[test]
    fn the_options_list_their_choices_then_buttons_unless_a_window_covers_them() {
        with_game(|g| {
            two_ships(g);
            let options = g.gui.at(GUI_OPTIONS_BOX);
            options.set(FOCUS_OPEN, 1u8);
            g.arena.vector(options, CHOICE_RECTS, &[rect(300, 200), rect(300, 240)]);
            let mut l = Layer::default();
            l.zones.buttons = vec![Button { rect: rect(700, 500), object: g.gui.addr() }];
            let kinds = |l: &mut Layer| built(l).into_iter().map(|(k, id, _)| (k, id)).collect::<Vec<_>>();
            assert_eq!(kinds(&mut l), [(ZoneKind::Choice, 0), (ZoneKind::Choice, 1), (ZoneKind::Button, 0)]);
            let notice = options.at(layout().options_restart_dialog);
            notice.set(FOCUS_OPEN, 1u8);
            g.arena.vector(notice, CHOICE_RECTS, &[rect(600, 400)]);
            assert_eq!(built(&mut l), [(ZoneKind::Choice, 0, rect(600, 400))], "the restart notice alone");
            options.at(layout().options_language).set(FOCUS_OPEN, 1u8);
            assert_eq!(kinds(&mut l), [(ZoneKind::Button, 0)], "the language picker's buttons");
        });
    }

    #[test]
    fn the_hotkey_screen_lists_the_options_buttons_alone_hud_rows_included() {
        with_game(|g| {
            let options = g.gui.at(GUI_OPTIONS_BOX);
            options.set(FOCUS_OPEN, 1u8);
            options.set(OPTIONS_CUSTOMIZE_CONTROLS, 1u8);
            g.arena.vector(options, CHOICE_RECTS, &[rect(300, 200)]);
            let mut l = Layer::default();
            let own = Button { rect: rect(1087, HUD_BOTTOM_TOP), object: options.addr() + layout().options_size() - 1 };
            let hud = Button { rect: rect(631, 0), object: options.addr() + layout().options_size() };
            l.zones.buttons = vec![hud, own];
            assert_eq!(built(&mut l), [(ZoneKind::Button, 1, own.rect)]);
        });
    }

    #[test]
    fn the_main_menu_leaves_out_the_last_games_ship() {
        with_game(|g| {
            two_ships(g);
            g.app.at(CAPP_MAIN_MENU).set(MAIN_MENU_OPEN, 1u8);
            let mut l = Layer::default();
            let start = Button { rect: rect(1078, 364), object: g.app.addr() };
            l.zones.buttons = vec![start];
            assert_eq!(built(&mut l), [(ZoneKind::Button, 0, start.rect)]);
        });
    }

    #[test]
    fn the_credits_list_no_zones() {
        with_game(|g| {
            let menu = g.app.at(CAPP_MAIN_MENU);
            menu.set(MAIN_MENU_OPEN, 1u8);
            menu.set(layout().main_menu_credits, 1u8);
            let mut l = Layer::default();
            l.zones.buttons = vec![Button { rect: rect(1078, 364), object: g.app.addr() }];
            assert_eq!(built(&mut l), []);
        });
    }

    #[test]
    fn the_main_menu_options_list_their_choices() {
        with_game(|g| {
            let menu = g.app.at(CAPP_MAIN_MENU);
            menu.set(MAIN_MENU_OPEN, 1u8);
            let options = menu.at(MAIN_MENU_OPTIONS);
            options.set(FOCUS_OPEN, 1u8);
            g.arena.vector(options, CHOICE_RECTS, &[rect(300, 200)]);
            assert_eq!(built(&mut Layer::default()), [(ZoneKind::Choice, 0, rect(300, 200))]);
        });
    }

    #[test]
    fn the_hangar_lists_its_filled_equipment_boxes_above_their_buttons_unless_the_ship_list_covers_them() {
        with_game(|g| {
            let menu = g.app.at(CAPP_MAIN_MENU);
            menu.set(MAIN_MENU_OPEN, 1u8);
            let hangar = menu.at(MAIN_MENU_HANGAR);
            hangar.set(HANGAR_OPEN, 1u8);
            let mut equipment_box = |r: Rect, item: [usize; EQUIPBOX_ITEM_PTRS]| {
                let b = g.arena.alloc(EQUIPBOX_ITEM + EQUIPBOX_ITEM_PTRS * size_of::<usize>());
                b.set(EQUIPBOX_HITBOX, r);
                b.set(EQUIPBOX_ITEM, item);
                b
            };
            let empty = equipment_box(rect(445, 525), [0; EQUIPBOX_ITEM_PTRS]);
            let weapon = equipment_box(rect(563, 525), [1, 0, 0, 0]);
            let crew = equipment_box(rect(70, 519), [0, 0, 1, 0]);
            g.arena.ptr_vector(hangar, HANGAR_EQUIP_BOXES, &[Some(empty), Some(weapon), Some(crew)]);
            let mut l = Layer::default();
            let customize = Button { rect: rect(70, 519 + SIDE / 2), object: g.app.addr() };
            l.zones.buttons = vec![customize];
            assert_eq!(
                built(&mut l),
                [
                    (ZoneKind::Slot, 1, rect(563, 525)),
                    (ZoneKind::Slot, 2, Rect { h: SIDE / 2, ..rect(70, 519) }),
                    (ZoneKind::Button, 0, customize.rect),
                ]
            );
            hangar.at(HANGAR_SHIP_LIST).set(SHIP_LIST_OPEN, 1u8);
            l.zones.buttons.clear();
            assert_eq!(built(&mut l), [], "no boxes under the ship list");
        });
    }

    #[test]
    fn the_hangar_and_the_game_menu_alone_list_their_ship_achievements() {
        with_game(|g| {
            let icon = |achievement, x| AchievementIcon { achievement, at: Point { x, y: 400 }, side: SIDE };
            let icons = [icon(1, 20), icon(0, 80)];
            let inside = |x| open_rect(Point { x, y: 400 }, SIDE, SIDE);
            let menu = g.app.at(CAPP_MAIN_MENU);
            menu.set(MAIN_MENU_OPEN, 1u8);
            let hangar = menu.at(MAIN_MENU_HANGAR);
            hangar.set(HANGAR_OPEN, 1u8);
            g.arena.vector(hangar, HANGAR_ACHIEVEMENTS, &icons);
            let mut l = Layer::default();
            assert_eq!(built(&mut l), [(ZoneKind::Achievement, 0, inside(20))], "an icon with an achievement");
            menu.set(MAIN_MENU_OPEN, 0u8);
            let game_menu = g.gui.at(GUI_MENU_BOX);
            game_menu.set(FOCUS_OPEN, 1u8);
            g.arena.vector(game_menu, MENU_ACHIEVEMENTS, &icons);
            assert_eq!(built(&mut l), [(ZoneKind::Achievement, 0, inside(20))]);
            set_tutorial_running(true);
            assert_eq!(built(&mut l), [], "none in the tutorial");
            set_tutorial_running(false);
            game_menu.at(MENU_CONFIRM).set(FOCUS_OPEN, 1u8);
            assert_eq!(built(&mut l), [], "none under the confirmation");
        });
    }

    #[test]
    fn the_score_screen_lists_the_achievements_of_the_tab_shown() {
        with_game(|g| {
            let menu = g.app.at(CAPP_MAIN_MENU);
            menu.set(MAIN_MENU_OPEN, 1u8);
            menu.set(MAIN_MENU_SCORE_SCREEN, 1u8);
            let score = score_keeper();
            let icon = |x| ScoreIcon { achievement: 1, at: Point { x, y: 150 } };
            g.arena.vector(score, SCORE_ACHIEVEMENTS, &[icon(700)]);
            g.arena.vector(score, SCORE_SHIP_ACHIEVEMENTS, &[icon(400)]);
            let side = SCORE_ACHIEVEMENT_SIDE;
            let at = |x| (ZoneKind::Achievement, 0, open_rect(Point { x, y: 150 }, side, side));
            let mut l = Layer::default();
            assert_eq!(built(&mut l), [], "neither tab shown");
            score.set(SCORE_ACHIEVEMENTS_TAB, 1u8);
            score.set(SCORE_SHIP_BEST_TAB, 1u8);
            assert_eq!(built(&mut l), [at(700), at(400)]);
            score.at(SCORE_SHIP_LIST).set(SHIP_LIST_OPEN, 1u8);
            assert_eq!(built(&mut l), [], "none under the ship list");
        });
    }

    #[test]
    fn the_game_over_window_lists_its_own_buttons_the_score_screens_over_it_and_none_under_the_credits() {
        with_game(|g| {
            two_ships(g);
            let over = g.gui.at(GUI_GAME_OVER);
            over.set(FOCUS_OPEN, 1u8);
            let score = score_keeper();
            let past = score.addr() + SCORE_SIZE;
            let (hud, own, ship) = (past, past + 1, past + 2);
            g.arena.vector(over, GAME_OVER_BUTTONS, &[own]);
            g.arena.vector(score, SCORE_SHIP_LIST + SHIP_LIST_BUTTONS, &[ship]);
            let mut l = Layer::default();
            let objects = [hud, own, score.addr(), ship];
            l.zones.buttons = objects.iter().map(|&object| Button { rect: rect(500, 400), object }).collect();
            let ids = |l: &mut Layer| built(l).into_iter().map(|(k, id, _)| (k, id)).collect::<Vec<_>>();
            assert_eq!(ids(&mut l), [(ZoneKind::Button, 1)], "its own button, no rooms or HUD");
            over.set(GAME_OVER_STATS, 1u8);
            assert_eq!(
                ids(&mut l),
                [(ZoneKind::Button, 2), (ZoneKind::Button, 3)],
                "the score screen's and its ships'"
            );
            over.set(GAME_OVER_CREDITS, 1u8);
            assert_eq!(built(&mut l), [], "none under the credits");
        });
    }

    #[test]
    fn the_equipment_screen_lists_shown_slots_and_buttons_off_the_hud() {
        with_game(|g| {
            two_ships(g);
            g.gui.at(layout().gui_ship_screens).set(FOCUS_OPEN, 1u8);
            let equip = g.gui.at(layout().gui_equip_screen);
            equip.set(FOCUS_OPEN, 1u8);
            let mut slot = |r: Rect| {
                let b = g.arena.alloc(EQUIPBOX_HITBOX + size_of::<Rect>());
                b.set(EQUIPBOX_HITBOX, r);
                b
            };
            let (first, overflow, last) = (slot(rect(100, 200)), slot(rect(150, 200)), slot(rect(200, 200)));
            equip.set(EQUIP_OVERCAPACITY_BOX, overflow.addr());
            g.arena.ptr_vector(equip, EQUIP_BOXES, &[Some(first), Some(overflow), None, Some(last)]);
            let mut l = Layer::default();
            let button = |y: i32| Button { rect: rect(600, y), object: g.gui.addr() };
            l.zones.buttons = vec![button(HUD_TOP_HEIGHT - 1), button(300), button(HUD_BOTTOM_TOP)];
            assert_eq!(
                built(&mut l),
                [
                    (ZoneKind::Slot, 0, rect(100, 200)),
                    (ZoneKind::Slot, 3, rect(200, 200)),
                    (ZoneKind::Button, 1, rect(600, 300)),
                ]
            );
        });
    }

    #[test]
    fn the_crew_screen_cuts_its_slots_above_their_buttons() {
        with_game(|g| {
            two_ships(g);
            g.gui.at(layout().gui_ship_screens).set(FOCUS_OPEN, 1u8);
            let crew = g.gui.at(layout().gui_crew_screen);
            crew.set(FOCUS_OPEN, 1u8);
            let slot = g.arena.alloc(EQUIPBOX_HITBOX + size_of::<Rect>());
            slot.set(EQUIPBOX_HITBOX, rect(100, 200));
            g.arena.ptr_vector(crew, CREWSCREEN_BOXES, &[Some(slot)]);
            let mut l = Layer::default();
            let name = Button { rect: rect(100, 200 + SIDE / 2), object: g.gui.addr() };
            l.zones.buttons = vec![name];
            assert_eq!(
                built(&mut l),
                [(ZoneKind::Slot, 0, Rect { h: SIDE / 2, ..rect(100, 200) }), (ZoneKind::Button, 0, name.rect)]
            );
        });
    }

    #[test]
    fn the_store_sell_tab_lists_the_sell_box_after_the_slots() {
        with_game(|g| {
            g.gui.at(layout().gui_store_screens).set(FOCUS_OPEN, 1u8);
            let equip = g.gui.at(layout().gui_equip_screen);
            equip.set(FOCUS_OPEN, 1u8);
            let slot = g.arena.alloc(EQUIPBOX_HITBOX + size_of::<Rect>());
            slot.set(EQUIPBOX_HITBOX, rect(400, 200));
            g.arena.ptr_vector(equip, EQUIP_BOXES, &[Some(slot)]);
            let image = g.arena.alloc(TEXTURE_H + size_of::<i32>());
            image.set(TEXTURE_W, SIDE);
            image.set(TEXTURE_H, SIDE);
            let sell = equip.at(EQUIP_SELL_BOX);
            sell.set(DROPBOX_POSITION, Point { x: 70, y: 250 });
            sell.set(DROPBOX_IMAGE, image.addr());
            sell.set(DROPBOX_INSERT_HEIGHT, 7i32);
            sell.set(DROPBOX_TITLE_INSERT, 3i32);
            let mut l = Layer::default();
            assert_eq!(built(&mut l).len(), 1, "no sell box out of store mode");
            equip.set(EQUIP_STORE_MODE, 1u8);
            let tall = Rect { h: SIDE + 7 + 3, ..rect(70, 250) };
            assert_eq!(built(&mut l), [(ZoneKind::Slot, 0, rect(400, 200)), (ZoneKind::SellBox, 0, tall)]);
        });
    }

    #[test]
    fn the_store_lists_the_buttons_of_its_shown_page_and_items() {
        with_game(|g| {
            let screens = g.gui.at(layout().gui_store_screens);
            screens.set(FOCUS_OPEN, 1u8);
            let store = g.arena.alloc(STORE_SHOW_PAGE2 + 1);
            store.set(STORE_SECTION_COUNT, 3i32);
            g.arena.ptr_vector(screens, TABBED_WINDOWS, &[Some(store)]);
            let item_box = 3 * STORE_SECTION_BOXES;
            let mut boxes = vec![None; item_box + 1];
            let mut l = Layer::default();
            for (n, i) in [0, STORE_PAGE_SECTIONS * STORE_SECTION_BOXES, item_box].into_iter().enumerate() {
                let b = g.arena.alloc(STOREBOX_BUTTON + size_of::<usize>());
                boxes[i] = Some(b);
                let r = rect(400 + 2 * SIDE * n as i32, 300);
                l.zones.buttons.push(Button { rect: r, object: b.addr() + STOREBOX_BUTTON });
            }
            g.arena.ptr_vector(store, STORE_BOXES, &boxes);
            let ids = |l: &mut Layer| built(l).into_iter().map(|(_, id, _)| id).collect::<Vec<_>>();
            assert_eq!(ids(&mut l), [0, 2], "page 1 and the items");
            store.set(STORE_SHOW_PAGE2, 1u8);
            assert_eq!(ids(&mut l), [1, 2], "page 2 and the items");
        });
    }

    #[test]
    fn the_hud_boxes_are_the_system_icons_and_the_filled_armament_boxes() {
        with_game(|g| {
            let control = g.gui.at(GUI_SYS_CONTROL);
            control.set(SYSCTRL_POSITION, Point { x: 10, y: 600 });
            let sys_box = g.arena.alloc(SYSBOX_SYSTEM + size_of::<usize>());
            sys_box.set(SYSBOX_LOCATION, Point { x: 20, y: 30 });
            g.arena.ptr_vector(control.at(SYSCTRL_BOXES), 0, &[Some(sys_box)]);
            let weapons = g.gui.at(GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL);
            weapons.set(ARMAMENT_LOCATION, Point { x: 300, y: 640 });
            let item = g.arena.alloc(WEAPON_SIZE);
            item.set(WEAPON_POWERED, true);
            let filled = g.arena.alloc(ARMAMENTBOX_ITEM + size_of::<usize>());
            filled.set(ARMAMENTBOX_HIT, Point { x: 5, y: 6 });
            filled.set(ARMAMENTBOX_ITEM, item.addr());
            let empty = g.arena.alloc(ARMAMENTBOX_ITEM + size_of::<usize>());
            g.arena.ptr_vector(weapons, ARMAMENT_BOXES, &[Some(filled), Some(empty)]);
            let icon = HudBox { rect: Rect { x: 52, y: 652, w: 20, h: 20 }, powered: None };
            let weapon = HudBox { rect: Rect { x: 306, y: 647, w: 94, h: 38 }, powered: Some(true) };
            assert_eq!(hud_boxes(), [icon, weapon]);
        });
    }

    #[test]
    fn a_ship_centre_spans_its_rooms() {
        with_game(|g| {
            let player = two_ships(g);
            let centre = (PLAYER_AT.x as f32 + SIDE as f32, PLAYER_AT.y as f32 + SIDE as f32 / 2.0);
            assert_eq!(ship_center(player, PLAYER_AT), Some(centre));
            let bare = g.player_manager().at(SHIPMANAGER_SHIP);
            assert_eq!(ship_center(bare, PLAYER_AT), None);
        });
    }
}
