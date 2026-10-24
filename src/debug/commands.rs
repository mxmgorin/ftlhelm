//! The FIFO test channel: dumps of zones and state, and commands that drive a run.

use std::fs::File;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;

use crate::config::layout::{Input, Slot, SlotMode};
use crate::core::layer::Layer;
use crate::core::util::log;
use crate::game::actions::{key_down, key_up, shift_count, MouseButton};
use crate::game::api::api;
use crate::game::offsets::*;
use crate::game::state;
use crate::game::zones::ZoneKind;
use crate::input::bindings::Source;
use crate::ui::draw::{text_width, WHEEL_FONT};

const CMD_ENV: &str = "FTLPAD_CMD";
const CMD_BUF: usize = 512;

/// The command FIFO and the text read from it short of a newline.
#[derive(Default)]
pub struct Commands {
    fifo: Option<File>,
    partial: Vec<u8>,
}

impl Commands {
    /// Opens the FIFO the environment names, without waiting for a writer.
    pub fn open() -> Commands {
        let fifo = std::env::var(CMD_ENV).ok().map(|path| {
            let f = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(path);
            log!("command channel {}", if f.is_ok() { "open" } else { "unavailable" });
            f.ok()
        });
        Commands { fifo: fifo.flatten(), partial: Vec::new() }
    }
}

fn mode_name(m: SlotMode) -> &'static str {
    match m {
        SlotMode::Keys => "keys",
        SlotMode::Mouse => "mouse",
        SlotMode::Flick => "flick",
    }
}

impl Layer {
    fn dump_zones(&self) {
        let under = self.zone_under_cursor().map_or(-1, |i| i as i64);
        log!(
            "dump: aiming={} targets={} cursor={},{} under={under}",
            state::aiming() as i32,
            self.zones.list.len(),
            self.cursor.last_x,
            self.cursor.last_y
        );
        let gui = state::gui();
        for t in &self.zones.list {
            let r = t.rect;
            if matches!(t.kind, ZoneKind::Room | ZoneKind::EnemyRoom) {
                let enemy = t.kind == ZoneKind::EnemyRoom;
                let ship = if enemy { state::enemy_ship() } else { state::player_ship() };
                let check = match (ship, gui) {
                    (Some(ship), Some(gui)) => {
                        let origin = if enemy { state::enemy_origin(gui) } else { state::player_origin(gui) };
                        let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
                        // SAFETY: Ship::GetSelectedRoomId on the ship the zone came from.
                        unsafe { (api().selected_room)(ship.ptr(), cx - origin.x, cy - origin.y, false) }
                    }
                    _ => -2,
                };
                log!("  {} {} at {},{} {}x{}, game says {check}", t.kind.name(), t.id, r.x, r.y, r.w, r.h);
            } else {
                log!("  {} {} at {},{} {}x{}", t.kind.name(), t.id, r.x, r.y, r.w, r.h);
            }
        }
    }

    fn dump_state(&self) {
        let wheel = self.wheel.open.and_then(|w| self.layout.wheels.get(w)).map_or("-", |w| &w.name);
        log!("state: {}; wheel {wheel} sel {}; shift {}; dpad {}, left {}, right {}; aiming {} crew {} paused {} precision {}",
             self.stack_names().join(" "), self.wheel.sel.map_or(-1, |s| s as i64), shift_count(),
             mode_name(self.slot_mode(Slot::Dpad)), mode_name(self.slot_mode(Slot::LStick)),
             mode_name(self.slot_mode(Slot::RStick)), state::aiming() as i32, state::any_crew_selected() as i32,
             state::game_paused() as i32, self.bind.precision as i32);
        let fps = state::fps_control();
        // SAFETY: the step cap float placed by redirect_step_cap.
        let cap = self.actions.step_cap.map_or(-1.0, |c| unsafe { (c as *const f32).read() });
        log!("speed: level {}, step {:.3}, cap {cap:.2}", fps.i32(FPS_LEVEL), fps.f32(FPS_STEP));
    }

    fn run_command(&mut self, line: &str) {
        let mut words = line.split_whitespace();
        let cmd = words.next().unwrap_or("");
        let nums: Vec<i32> = line.split_whitespace().skip(1).filter_map(|w| w.parse().ok()).collect();
        match (cmd, nums.as_slice()) {
            ("move", &[x, y]) => {
                (self.cursor.x, self.cursor.y) = (x as f32, y as f32);
                self.send_cursor();
            }
            ("click", _) => self.click_tap(MouseButton::Left),
            ("rclick", _) => self.click_tap(MouseButton::Right),
            ("key", &[code]) => {
                key_down(code);
                key_up(code);
            }
            ("dump", _) => self.dump_zones(),
            ("state", _) => self.dump_state(),
            ("font", &[font]) => self.plate_font = font,
            ("doors", _) => {
                let under = self.zone_under_cursor().map(|i| self.zones.list[i].id);
                let Some(ship) = state::player_ship() else {
                    return;
                };
                for (i, d) in ship.at(SHIP_DOORS).vec_objs() {
                    let (r1, r2) = (d.i32(DOOR_ROOM1), d.i32(DOOR_ROOM2));
                    if under.is_some_and(|u| u == r1 || u == r2) {
                        log!(
                            "door {i} rooms {r1},{r2} at {},{} open={}",
                            d.i32(DOOR_X),
                            d.i32(DOOR_Y),
                            d.flag(DOOR_OPEN) as i32
                        );
                    }
                }
            }
            ("roomdoors", _) => self.room_doors(),
            ("buttons", _) => {
                let gui = state::gui().map_or(0, |g| g.addr()) as i64;
                let map = state::star_map().map_or(0, |m| m.addr()) as i64;
                for (i, b) in self.zones.buttons.iter().enumerate() {
                    let r = b.rect;
                    let off = |base: i64| if base == 0 { -1 } else { b.object as i64 - base };
                    log!("button {i} at {},{} {}x{} gui+{} map+{}", r.x, r.y, r.w, r.h, off(gui), off(map));
                }
            }
            ("wheel", _) => {
                let name = line["wheel".len()..].trim();
                self.close_wheel();
                if let Some(i) = self.layout.wheels.iter().position(|w| w.name == name) {
                    self.open_wheel(i, Source::Input(Input::Home));
                }
            }
            ("pick", &[slot]) => {
                if self.wheel.open.is_some() {
                    self.wheel.sel = usize::try_from(slot).ok().filter(|&s| s < self.wheel.plates.len());
                }
            }
            ("badge", _) if state::gui().is_some() => {
                let text = self.badge_text();
                let width = if text.is_empty() { 0.0 } else { text_width(WHEEL_FONT, &text) };
                log!("badge: '{text}' width {width}");
            }
            _ => log!("unknown command: {line}"),
        }
    }

    /// Runs the whole lines waiting in the command FIFO.
    pub fn poll_commands(&mut self) {
        let Some(fifo) = self.cmd.fifo.as_mut() else {
            return;
        };
        let mut buf = [0u8; CMD_BUF];
        let n = match fifo.read(&mut buf) {
            Ok(n) if n > 0 => n,
            _ => return,
        };
        self.cmd.partial.extend_from_slice(&buf[..n]);
        while let Some(nl) = self.cmd.partial.iter().position(|&c| c == b'\n') {
            let line: Vec<u8> = self.cmd.partial.drain(..=nl).collect();
            let line = String::from_utf8_lossy(&line[..nl]).trim().to_string();
            if !line.is_empty() {
                self.run_command(&line);
            }
        }
        if self.cmd.partial.len() >= CMD_BUF {
            log!("command too long, dropped");
            self.cmd.partial.clear();
        }
    }
}
