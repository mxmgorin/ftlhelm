//! The zones the D-pad walks, rebuilt each frame for the screen on top, and the buttons the
//! game's hit-test reports between frames.

use std::sync::Mutex;

use super::api::{Point, Rect};
use super::mem::Obj;
use super::offsets::*;
use super::state;
use crate::core::layer::Layer;
use crate::ui::text;

const DOOR_HALF: i32 = 10;
/// Half the side of the square a beacon counts as.
const BEACON_HALF: i32 = 12;
const MAX_ZONES: usize = 64;
const MAX_BUTTONS: usize = 64;
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
}

/// Buttons the game hit-tested since the last frame, and whether it tested any.
struct Seen {
    buttons: Vec<Button>,
    tested: bool,
}

static SEEN: Mutex<Seen> = Mutex::new(Seen { buttons: Vec::new(), tested: false });

/// Records a button the game hit-tests, with its box moved to the screen. Touches only the
/// seen set: a hook may hold the layer meanwhile.
pub fn record_button(button: Obj) {
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    seen.tested = true;
    let gui = state::gui();
    let in_crew_control = gui.is_some_and(|g| g.at(GUI_CREW_CONTROL).contains(CREW_CONTROL_SIZE, button.addr()));
    let drawn = !button.flag(BUTTON_RENDER_OFF);
    if seen.buttons.len() >= MAX_BUTTONS
        || !button.flag(BUTTON_ACTIVE)
        || !drawn
        || in_crew_control
        || seen.buttons.iter().any(|b| b.object == button.addr())
    {
        return;
    }
    let mut r = button.rect(BUTTON_HITBOX);
    // The armament bars keep their buttons' boxes relative to the bar.
    let bars = [(state::weapon_control(), WEAPON_CONTROL_SIZE), (state::drone_control(), DRONE_CONTROL_SIZE)];
    for (bar, size) in bars {
        if let Some(bar) = bar.filter(|b| b.contains(size, button.addr())) {
            let at = bar.point(ARMAMENT_LOCATION);
            r.x += at.x;
            r.y += at.y;
        }
    }
    // So does the star map, relative to its position.
    if let Some(map) = state::star_map().filter(|m| m.contains(STARMAP_SIZE, button.addr())) {
        let at = map.point(STARMAP_POSITION);
        r.x += at.x;
        r.y += at.y;
    }
    let corner = r.x < CORNER_BUTTON_MAX && r.y < CORNER_BUTTON_MAX && r.w <= CORNER_BUTTON_SIZE;
    if r.w > 0 && r.h > 0 && !corner && r.x + r.w <= GAME_W && r.y + r.h <= GAME_H {
        seen.buttons.push(Button { rect: r, object: button.addr() });
    }
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
    boxes.extend(armament_boxes().into_iter().map(|(rect, _, powered)| HudBox { rect, powered: Some(powered) }));
    boxes
}

/// The weapon boxes, then the drone boxes, that hold an item: each box on screen, its slot
/// counted on from the weapon slots for a drone, and whether its item is powered.
fn armament_boxes() -> Vec<(Rect, usize, bool)> {
    let mut boxes = Vec::new();
    let bars =
        [(state::weapon_control(), WEAPON_POWERED, 0), (state::drone_control(), DRONE_POWERED, WEAPON_SLOTS as usize)];
    for (bar, powered, first) in bars.into_iter().filter_map(|(bar, powered, first)| Some((bar?, powered, first))) {
        let at = bar.point(ARMAMENT_LOCATION);
        for (slot, b) in bar.at(ARMAMENT_BOXES).vec_objs() {
            let Some(item) = b.obj(ARMAMENTBOX_ITEM) else {
                continue;
            };
            let hit = b.point(ARMAMENTBOX_HIT);
            let rect = open_rect(Point { x: at.x + hit.x, y: at.y + hit.y }, ARMAMENTBOX_HIT_W, ARMAMENTBOX_HIT_H);
            boxes.push((rect, first + slot, item.flag(powered)));
        }
    }
    boxes
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
        }
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
    fn add_map_zones(&mut self) -> bool {
        let Some(map) = state::star_map().filter(|m| m.flag(FOCUS_OPEN)) else {
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

    /// The open dialog's choices; false when none is open.
    fn add_choice_zones(&mut self) -> bool {
        let Some(choice) = state::open_choice() else {
            return false;
        };
        let zoom = text::box_zoom(choice.addr());
        for (i, r) in choice.at(CHOICE_RECTS).vec_items::<Rect>().enumerate() {
            self.push(zoom.map_or(r, |z| z.to_screen(r)), i as i32, ZoneKind::Choice);
        }
        true
    }

    /// The hit-tested buttons: with the map open only its own, with another screen or the
    /// menu open none from the HUD's top and bottom rows.
    /// None from the store page not shown.
    fn add_button_zones(&mut self, screen_open: bool, map: Option<Obj>) {
        let hidden = state::hidden_store_buttons();
        for i in 0..self.buttons.len() {
            let b = self.buttons[i];
            if map.is_some_and(|m| !m.contains(STARMAP_SIZE, b.object)) || hidden.contains(&b.object) {
                continue;
            }
            if screen_open && (b.rect.y < HUD_TOP_HEIGHT || b.rect.y >= HUD_BOTTOM_TOP) {
                continue;
            }
            self.push(b.rect, i as i32, ZoneKind::Button);
        }
    }

    /// Equipment and crew slots of an open ship screen, which are not buttons; the overflow
    /// boxes only while shown, then the store's sell box.
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
                let r = slot.rect(EQUIPBOX_HITBOX);
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
        zones.list.clear();
        {
            let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
            if seen.tested {
                zones.buttons = std::mem::take(&mut seen.buttons);
                seen.tested = false;
            }
        }
        let Some(gui) = state::gui() else {
            return;
        };
        let player = state::player_ship();
        let modal_open = state::modal_window(gui).is_some();
        if !modal_open && zones.add_choice_zones() {
            return;
        }
        let screen_open = state::open_tabbed().is_some() || modal_open;
        let map = state::star_map().filter(|m| m.flag(FOCUS_OPEN));
        let precision = self.bind.precision;
        if let Some(player) = player {
            if !screen_open && !zones.add_map_zones() {
                let enemy = state::enemy_ship();
                let origin = state::player_origin(gui);
                if state::aiming() {
                    if let Some(enemy) = enemy {
                        zones.add_rooms(enemy, state::enemy_origin(gui), true);
                    }
                    zones.add_rooms(player, origin, false);
                } else if precision && !state::any_crew_selected() {
                    zones.add_doors(player, origin);
                } else {
                    zones.add_rooms(player, origin, false);
                    if let Some(enemy) = enemy {
                        zones.add_rooms(enemy, state::enemy_origin(gui), true);
                    }
                    zones.add_armament_zones();
                }
            }
        }
        if screen_open {
            zones.add_slot_zones(gui);
        }
        if !precision || state::aiming() || state::any_crew_selected() {
            zones.add_button_zones(screen_open, map);
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
    use crate::game::fake::{with_game, Game};
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
            let item = g.arena.alloc(WEAPON_POWERED + size_of::<u8>());
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
            let item = g.arena.alloc(WEAPON_POWERED + size_of::<u8>());
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
