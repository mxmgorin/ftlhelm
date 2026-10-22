//! The zones the D-pad walks, rebuilt each frame for the screen on top, and the buttons the
//! game's hit-test reports between frames.

use std::sync::Mutex;

use super::api::{Point, Rect};
use super::mem::Obj;
use super::offsets::*;
use super::state;
use crate::core::layer::Layer;

const DOOR_HALF: i32 = 10;
/// Half the side of the square a beacon counts as.
const BEACON_HALF: i32 = 12;
const MAX_TARGETS: usize = 64;
const MAX_BUTTONS: usize = 64;
/// Small unplaced buttons the game parks in this corner.
const CORNER_BUTTON_MAX: i32 = 40;
const CORNER_BUTTON_SIZE: i32 = 24;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TargetKind {
    Room,
    EnemyRoom,
    Beacon,
    Sector,
    Choice,
    Button,
    Door,
    Slot,
}

impl TargetKind {
    pub fn name(self) -> &'static str {
        match self {
            TargetKind::Room => "player room",
            TargetKind::EnemyRoom => "enemy room",
            TargetKind::Beacon => "beacon",
            TargetKind::Sector => "sector",
            TargetKind::Choice => "choice",
            TargetKind::Button => "button",
            TargetKind::Door => "door",
            TargetKind::Slot => "slot",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub rect: Rect,
    /// Room id, or the index in the list the zone came from.
    pub id: i32,
    pub kind: TargetKind,
}

/// A button the game hit-tested: its box on screen and the object's address.
#[derive(Clone, Copy, Debug)]
pub struct Button {
    pub rect: Rect,
    pub object: usize,
}

#[derive(Default)]
pub struct Zones {
    pub targets: Vec<Target>,
    /// The last whole set of buttons the game hit-tested.
    pub buttons: Vec<Button>,
}

/// Buttons the game hit-tested since the last frame, and whether it tested any.
struct Seen {
    buttons: Vec<Button>,
    tested: bool,
}

static SEEN: Mutex<Seen> = Mutex::new(Seen { buttons: Vec::new(), tested: false });

/// Records a button the game hit-tests, with its box moved to the screen. Runs inside the
/// game's GenericButton::MouseMove, so it touches only the seen set.
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

/// Bounding box centre of a ship's rooms on screen.
pub fn ship_center(ship: Obj, origin: Point) -> Option<(f32, f32)> {
    let rooms = ship.at(SHIP_ROOMS);
    let (mut x0, mut y0, mut x1, mut y1) = (GAME_W, GAME_H, 0, 0);
    let mut any = false;
    for room in (0..rooms.vec_len(std::mem::size_of::<usize>())).filter_map(|i| rooms.vec_obj(i)) {
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
    fn push(&mut self, rect: Rect, id: i32, kind: TargetKind) {
        if self.targets.len() < MAX_TARGETS {
            self.targets.push(Target { rect, id, kind });
        }
    }

    fn add_door_list(&mut self, doors: Obj, origin: Point) {
        for i in 0..doors.vec_len(std::mem::size_of::<usize>()) {
            if let Some(door) = doors.vec_obj(i) {
                let (x, y) = (origin.x + door.i32(DOOR_X), origin.y + door.i32(DOOR_Y));
                let rect = Rect { x: x - DOOR_HALF, y: y - DOOR_HALF, w: 2 * DOOR_HALF, h: 2 * DOOR_HALF };
                self.push(rect, i as i32, TargetKind::Door);
            }
        }
    }

    /// Inner doors and the airlocks on the hull.
    fn add_doors(&mut self, ship: Obj, origin: Point) {
        self.add_door_list(ship.at(SHIP_DOORS), origin);
        self.add_door_list(ship.at(SHIP_AIRLOCKS), origin);
    }

    fn add_rooms(&mut self, ship: Obj, origin: Point, enemy: bool) {
        let rooms = ship.at(SHIP_ROOMS);
        for room in (0..rooms.vec_len(std::mem::size_of::<usize>())).filter_map(|i| rooms.vec_obj(i)) {
            let mut r = room.rect(ROOM_RECT);
            r.x += origin.x;
            r.y += origin.y;
            self.push(r, room.i32(ROOM_ID), if enemy { TargetKind::EnemyRoom } else { TargetKind::Room });
        }
    }

    /// Beacons, or the sector boxes while a new sector is being chosen; false when the map
    /// is closed.
    fn add_map_targets(&mut self) -> bool {
        let Some(map) = state::star_map().filter(|m| m.flag(FOCUS_OPEN)) else {
            return false;
        };
        if map.flag(STARMAP_CHOOSING_SECTOR) {
            let offset = map.point(STARMAP_SECTOR_OFFSET);
            let boxes = map.at(STARMAP_SECTOR_HITBOXES);
            for i in 0..boxes.vec_len(std::mem::size_of::<Rect>()) {
                if let Some(mut r) = boxes.vec_get::<Rect>(i) {
                    r.x += offset.x;
                    r.y += offset.y;
                    self.push(r, i as i32, TargetKind::Sector);
                }
            }
            return true;
        }
        let origin = map.point(STARMAP_POSITION);
        let (tx, ty) = (map.f32(STARMAP_TRANSLATION), map.f32(STARMAP_TRANSLATION + 4));
        let locations = map.at(STARMAP_LOCATIONS);
        for i in 0..locations.vec_len(std::mem::size_of::<usize>()) {
            if let Some(loc) = locations.vec_obj(i) {
                let cx = origin.x + (tx + loc.f32(LOCATION_LOC)).round() as i32;
                let cy = origin.y + (ty + loc.f32(LOCATION_LOC + 4)).round() as i32;
                let rect = Rect { x: cx - BEACON_HALF, y: cy - BEACON_HALF, w: 2 * BEACON_HALF, h: 2 * BEACON_HALF };
                self.push(rect, i as i32, TargetKind::Beacon);
            }
        }
        true
    }

    /// The open event box's choices; false when it is closed.
    fn add_choice_targets(&mut self, gui: Obj) -> bool {
        let choice = gui.at(GUI_CHOICE_BOX);
        if !choice.flag(FOCUS_OPEN) {
            return false;
        }
        let rects = choice.at(CHOICE_RECTS);
        for i in 0..rects.vec_len(std::mem::size_of::<Rect>()) {
            if let Some(r) = rects.vec_get::<Rect>(i) {
                self.push(r, i as i32, TargetKind::Choice);
            }
        }
        true
    }

    /// With the map open only its own buttons count, a beacon click jumps by itself; with
    /// another screen or the menu open the HUD's top and bottom rows drop out.
    fn add_button_targets(&mut self, screen_open: bool, map: Option<Obj>) {
        for i in 0..self.buttons.len() {
            let b = self.buttons[i];
            if map.is_some_and(|m| !m.contains(STARMAP_SIZE, b.object)) {
                continue;
            }
            if screen_open && (b.rect.y < HUD_TOP_HEIGHT || b.rect.y >= HUD_BOTTOM_TOP) {
                continue;
            }
            self.push(b.rect, i as i32, TargetKind::Button);
        }
    }

    /// Equipment and crew slots of an open ship screen, which are not buttons.
    fn add_slot_targets(&mut self, gui: Obj) {
        for (window, boxes) in [(GUI_EQUIP_SCREEN, EQUIP_BOXES), (GUI_CREW_SCREEN, CREWSCREEN_BOXES)] {
            let window = gui.at(window);
            if !window.flag(FOCUS_OPEN) {
                continue;
            }
            let list = window.at(boxes);
            for i in 0..list.vec_len(std::mem::size_of::<usize>()) {
                if let Some(slot) = list.vec_obj(i) {
                    let r = slot.rect(EQUIPBOX_HITBOX);
                    if r.w > 0 && r.h > 0 {
                        self.push(r, i as i32, TargetKind::Slot);
                    }
                }
            }
        }
    }
}

impl Layer {
    /// Rebuilds the zones for the current screen: an event's choices alone while it is open
    /// (it sits over any screen but the game menu), beacons or sector boxes with the map
    /// open, otherwise rooms; the buttons the game hit-tests come on top.
    pub fn build_targets(&mut self) {
        let zones = &mut self.zones;
        zones.targets.clear();
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
        let player = state::ship_of(state::player_complete());
        let modal_open = state::modal_window(gui).is_some();
        if !modal_open && zones.add_choice_targets(gui) {
            return;
        }
        let screen_open = state::open_tabbed().is_some() || modal_open;
        let map = state::star_map().filter(|m| m.flag(FOCUS_OPEN));
        let precision = self.bind.precision;
        if let Some(player) = player {
            if !screen_open && !zones.add_map_targets() {
                let enemy = state::ship_of(state::enemy_complete());
                let origin = gui.point(GUI_SHIP_POSITION);
                if state::aiming() {
                    if let Some(enemy) = enemy {
                        zones.add_rooms(enemy, state::enemy_origin(gui), true);
                    }
                    zones.add_rooms(player, origin, false);
                } else if precision && !state::crew_selected() {
                    zones.add_doors(player, origin);
                } else {
                    zones.add_rooms(player, origin, false);
                    if let Some(enemy) = enemy {
                        zones.add_rooms(enemy, state::enemy_origin(gui), true);
                    }
                }
            }
        }
        if screen_open {
            zones.add_slot_targets(gui);
        }
        if !precision || state::aiming() || state::crew_selected() {
            zones.add_button_targets(screen_open, map);
        }
    }

    /// The zone under the cursor's last reported position.
    pub fn target_under_cursor(&self) -> Option<usize> {
        self.zones.targets.iter().position(|t| t.rect.contains(self.cursor.last_x, self.cursor.last_y))
    }
}
