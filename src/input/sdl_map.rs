//! PortMaster's per-device SDL mapping as SIL raw buttons. SDL numbers a pad's buttons over
//! all of its evdev key codes, SIL over the gamepad codes only, so an SDL index goes through
//! its key code. SDL builds differ in the order they number the codes in: stock SDL puts
//! the gamepad codes first, some firmware builds number every code in ascending order. The
//! mapping's volume entries, when it has them, tell the two apart.

use crate::config::layout::SIL_BUTTONS;

const KEY_VOLUMEDOWN: u16 = 114;
const KEY_VOLUMEUP: u16 = 115;
pub const BTN_JOYSTICK: u16 = 0x120;
pub const BTN_TRIGGER_HAPPY: u16 = 0x2c0;
/// SIL's key tables: raw buttons for the codes from BTN_JOYSTICK, then from BTN_TRIGGER_HAPPY.
pub const SIL_KEYMAP_LOW_LEN: u16 = 32;
pub const SIL_KEYMAP_HIGH_LEN: u16 = 40;

/// Mapping labels of the SIL buttons, in `Input` order.
const BUTTON_LABELS: [&str; SIL_BUTTONS] = [
    "guide",
    "start",
    "back",
    "y",
    "x",
    "b",
    "a",
    "leftshoulder",
    "rightshoulder",
    "lefttrigger",
    "righttrigger",
    "leftstick",
    "rightstick",
];
/// Mapping labels of D-pad buttons, in `Input` order: left, up, right, down.
const DPAD_LABELS: [&str; 4] = ["dpleft", "dpup", "dpright", "dpdown"];
const VOLUME_LABELS: [&str; 2] = ["volumedown", "volumeup"];

/// The SDL button indices a mapping names.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SdlMap {
    buttons: [Option<u16>; SIL_BUTTONS],
    dpad: [Option<u16>; 4],
    volume: [Option<u16>; 2],
}

/// A mapping resolved for one pad: SIL raw buttons, None where the mapping names none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resolved {
    pub buttons: [Option<i32>; SIL_BUTTONS],
    pub dpad: [Option<i32>; 4],
    pub ascending: bool,
}

fn button_index(map: &str, label: &str) -> Option<u16> {
    let token = format!(",{label}:b");
    let at = map.find(&token)? + token.len();
    let digits: String = map[at..].chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Reads the button entries of a mapping string; None when it names no button.
pub fn parse(map: &str) -> Option<SdlMap> {
    let m = SdlMap {
        buttons: BUTTON_LABELS.map(|l| button_index(map, l)),
        dpad: DPAD_LABELS.map(|l| button_index(map, l)),
        volume: VOLUME_LABELS.map(|l| button_index(map, l)),
    };
    (m.buttons.iter().chain(&m.dpad).any(Option::is_some)).then_some(m)
}

/// The pad's key codes in SDL button order.
fn sdl_order(codes: &[u16], ascending: bool) -> Vec<u16> {
    let mut sorted = codes.to_vec();
    sorted.sort_unstable();
    if !ascending {
        sorted.sort_by_key(|&c| c < BTN_JOYSTICK);
    }
    sorted
}

/// Whether the mapping's volume entries land on the volume keys under the ascending order.
fn numbers_ascending(map: &SdlMap, codes: &[u16]) -> bool {
    let order = sdl_order(codes, true);
    let lands = |idx: Option<u16>, key: u16| idx.and_then(|i| order.get(i as usize)) == Some(&key);
    lands(map.volume[0], KEY_VOLUMEDOWN) && lands(map.volume[1], KEY_VOLUMEUP)
}

/// Resolves the mapping for a pad with these key codes; `sil_button` gives SIL's raw button
/// for a key code.
pub fn resolve(map: &SdlMap, codes: &[u16], sil_button: impl Fn(u16) -> Option<i32>) -> Resolved {
    let ascending = numbers_ascending(map, codes);
    let order = sdl_order(codes, ascending);
    let raw = |idx: Option<u16>| idx.and_then(|i| order.get(i as usize)).and_then(|&c| sil_button(c));
    Resolved { buttons: map.buttons.map(raw), dpad: map.dpad.map(raw), ascending }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::layout::Input;

    /// muOS on the RG35XX-SP: ESC and the volume keys sit below the gamepad codes.
    const MUOS_MAP: &str = "19000000010000000100000000010000,Deeplay-keys,a:b3,b:b4,x:b6,y:b5,\
        leftshoulder:b7,rightshoulder:b8,lefttrigger:b12,righttrigger:b13,guide:b11,start:b10,\
        back:b9,dpup:h0.1,dpleft:h0.8,dpright:h0.2,dpdown:h0.4,volumedown:b1,volumeup:b2,\
        leftx:a0,lefty:a1,platform:Linux,";
    const MUOS_CODES: [u16; 15] = [1, 114, 115, 304, 305, 306, 307, 308, 309, 310, 311, 312, 314, 315, 354];

    /// SIL numbers the codes of its low key table in ascending order.
    fn sil_button(codes: &[u16]) -> impl Fn(u16) -> Option<i32> + '_ {
        move |c| {
            let low = BTN_JOYSTICK..BTN_JOYSTICK + SIL_KEYMAP_LOW_LEN;
            let pad: Vec<u16> = codes.iter().copied().filter(|k| low.contains(k)).collect();
            pad.iter().position(|&k| k == c).map(|i| i as i32)
        }
    }

    #[test]
    fn muos_numbers_every_code_in_ascending_order() {
        let map = parse(MUOS_MAP).expect("the mapping names buttons");
        let r = resolve(&map, &MUOS_CODES, sil_button(&MUOS_CODES));
        assert!(r.ascending);
        assert_eq!(r.buttons[Input::FaceDown as usize], Some(0));
        assert_eq!(r.buttons[Input::FaceRight as usize], Some(1));
        assert_eq!(r.buttons[Input::FaceUp as usize], Some(2));
        assert_eq!(r.buttons[Input::FaceLeft as usize], Some(3));
        assert_eq!(r.buttons[Input::Start as usize], Some(7));
        assert_eq!(r.buttons[Input::R2 as usize], Some(10));
        assert_eq!(r.buttons[Input::L3 as usize], None);
        assert_eq!(r.dpad, [None; 4]);
    }

    #[test]
    fn without_volume_entries_gamepad_codes_come_first() {
        let map = parse(",a:b0,b:b1,start:b11,").expect("the mapping names buttons");
        let r = resolve(&map, &MUOS_CODES, sil_button(&MUOS_CODES));
        assert!(!r.ascending);
        assert_eq!(r.buttons[Input::FaceDown as usize], Some(0));
        assert_eq!(r.buttons[Input::Start as usize], None);
    }

    #[test]
    fn a_mapping_without_buttons_is_none() {
        assert_eq!(parse(",leftx:a0,lefty:a1,"), None);
    }
}
