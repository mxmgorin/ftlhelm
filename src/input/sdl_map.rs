//! PortMaster's per-device SDL mapping as SIL raw buttons and evdev axes. SDL numbers a pad's
//! buttons over all of its evdev key codes, SIL over the gamepad codes only, so an SDL index
//! goes through its key code. SDL builds differ in the order they number the codes in: stock
//! SDL puts the gamepad codes first, some firmware builds number every code in ascending
//! order. The order under which more of the mapping's entries land on the pad's keys is the
//! build's; a tie keeps stock SDL's. SDL numbers axes over the evdev axis codes in ascending
//! order, hats left out. The variable may hold several mappings, one per line, of which a pad
//! takes the one whose GUID carries its vendor and product, its own name breaking a tie; one
//! whose GUID carries none serves any pad.

use std::ops::RangeInclusive;

use crate::config::layout::SIL_BUTTONS;
use crate::game::offsets::{SILJOY_KEYMAP_HIGH, SILJOY_KEYMAP_LOW};

const KEY_VOLUMEDOWN: u16 = 114;
const KEY_VOLUMEUP: u16 = 115;
/// The volume keys, in VOLUME_LABELS order.
const VOLUME_KEYS: [u16; 2] = [KEY_VOLUMEDOWN, KEY_VOLUMEUP];
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_RX: u16 = 0x03;
const ABS_RY: u16 = 0x04;
/// ABS_HAT0X to ABS_HAT3Y, which SDL numbers as hats.
const ABS_HATS: RangeInclusive<u16> = 0x10..=0x17;
pub const BTN_JOYSTICK: u16 = 0x120;
pub const BTN_TRIGGER_HAPPY: u16 = 0x2c0;
/// SIL's key tables: raw buttons for the codes from BTN_JOYSTICK, then from BTN_TRIGGER_HAPPY.
pub const SIL_KEYMAP_LOW_LEN: u16 = 32;
pub const SIL_KEYMAP_HIGH_LEN: u16 = 40;

/// SIL's keymap slot of key code `code`; None for a code outside its button windows.
pub fn keymap_slot(code: u16) -> Option<usize> {
    if (BTN_JOYSTICK..BTN_JOYSTICK + SIL_KEYMAP_LOW_LEN).contains(&code) {
        Some(SILJOY_KEYMAP_LOW + (code - BTN_JOYSTICK) as usize)
    } else if (BTN_TRIGGER_HAPPY..BTN_TRIGGER_HAPPY + SIL_KEYMAP_HIGH_LEN).contains(&code) {
        Some(SILJOY_KEYMAP_HIGH + (code - BTN_TRIGGER_HAPPY) as usize)
    } else {
        None
    }
}

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
pub const AXES: usize = 6;
/// Mapping labels of the analog axes: both sticks' x and y, then the triggers.
pub const AXIS_LABELS: [&str; AXES] = ["leftx", "lefty", "rightx", "righty", "lefttrigger", "righttrigger"];
pub const STICK_AXES: usize = 4;
/// The axes SIL reads both sticks from, in AXIS_LABELS order.
const SIL_STICK_AXES: [u16; STICK_AXES] = [ABS_X, ABS_Y, ABS_RX, ABS_RY];

/// Hex digits of an SDL GUID: bus, CRC, vendor, product and version as 16-bit words, little
/// endian, a zero word after vendor and after product.
const GUID_LEN: usize = 32;
const GUID_VENDOR: usize = 8;
const GUID_PRODUCT: usize = 16;
const GUID_WORD_LEN: usize = 4;

/// The SDL button indices a mapping names, with the pad it is for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SdlMap {
    /// The mapping line as given.
    pub line: String,
    pub name: String,
    /// Vendor and product, None when the GUID does not carry them.
    pub ids: Option<(u16, u16)>,
    buttons: [Option<u16>; SIL_BUTTONS],
    dpad: [Option<u16>; 4],
    volume: [Option<u16>; 2],
    axes: [Option<AxisEntry>; AXES],
}

/// An analog axis a mapping names: SDL's axis index, and whether the mapping inverts it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct AxisEntry {
    index: u16,
    inverted: bool,
}

/// An analog axis as the pad's input device numbers it, and whether the mapping inverts it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MappedAxis {
    pub code: u16,
    pub inverted: bool,
}

/// A mapping resolved for one pad: SIL raw buttons, None where the mapping names none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resolved {
    pub buttons: [Option<i32>; SIL_BUTTONS],
    pub dpad: [Option<i32>; 4],
    pub ascending: bool,
}

/// The index after `label` of kind `kind`, `b` for a button or `a` for an axis, with the
/// text after it.
fn entry<'a>(map: &'a str, label: &str, kind: char) -> Option<(u16, &'a str)> {
    let token = format!(",{label}:{kind}");
    let at = map.find(&token)? + token.len();
    let digits = map[at..].chars().take_while(char::is_ascii_digit).count();
    Some((map[at..at + digits].parse().ok()?, &map[at + digits..]))
}

fn button_index(map: &str, label: &str) -> Option<u16> {
    entry(map, label, 'b').map(|(index, _)| index)
}

fn axis_entry(map: &str, label: &str) -> Option<AxisEntry> {
    entry(map, label, 'a').map(|(index, rest)| AxisEntry { index, inverted: rest.starts_with('~') })
}

/// The vendor and product a GUID carries, None when it does not.
fn guid_ids(guid: &str) -> Option<(u16, u16)> {
    if guid.len() != GUID_LEN || !guid.is_ascii() {
        return None;
    }
    let word = |at: usize| {
        let hex = &guid[at..at + GUID_WORD_LEN];
        u16::from_str_radix(hex, 16).ok().map(u16::swap_bytes)
    };
    let zero_after = |at: usize| word(at + GUID_WORD_LEN) == Some(0);
    (zero_after(GUID_VENDOR) && zero_after(GUID_PRODUCT)).then_some((word(GUID_VENDOR)?, word(GUID_PRODUCT)?))
}

/// Reads one mapping line; None when it names no button.
pub fn parse(map: &str) -> Option<SdlMap> {
    let mut fields = map.splitn(3, ',');
    let ids = guid_ids(fields.next().unwrap_or_default());
    let name = fields.next().unwrap_or_default().to_string();
    let m = SdlMap {
        line: map.to_string(),
        name,
        ids,
        buttons: BUTTON_LABELS.map(|l| button_index(map, l)),
        dpad: DPAD_LABELS.map(|l| button_index(map, l)),
        volume: VOLUME_LABELS.map(|l| button_index(map, l)),
        axes: AXIS_LABELS.map(|l| axis_entry(map, l)),
    };
    (m.buttons.iter().chain(&m.dpad).any(Option::is_some)).then_some(m)
}

/// Reads every mapping line of the variable that names a button.
pub fn parse_all(config: &str) -> Vec<SdlMap> {
    config.lines().filter_map(parse).collect()
}

/// The mapping for a pad: of those for its vendor and product, the one under its name, else
/// the first; with none for them, the first whose GUID carries no ids. None when none fits.
pub fn select<'a>(maps: &'a [SdlMap], name: &str, vendor: u16, product: u16) -> Option<&'a SdlMap> {
    let mut own = maps.iter().filter(|m| m.ids == Some((vendor, product))).peekable();
    let first_own = own.peek().copied();
    own.find(|m| m.name == name).or(first_own).or_else(|| maps.iter().find(|m| m.ids.is_none()))
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

/// How many of the mapping's entries land under an order: buttons and D-pad entries on a SIL
/// button, volume entries on their keys.
fn fits(map: &SdlMap, codes: &[u16], ascending: bool, sil_button: &impl Fn(u16) -> Option<i32>) -> usize {
    let order = sdl_order(codes, ascending);
    let code = |idx: &Option<u16>| idx.and_then(|i| order.get(i as usize).copied());
    let buttons = (map.buttons.iter().chain(&map.dpad)).filter(|idx| code(idx).and_then(sil_button).is_some());
    let volume = (map.volume.iter().zip(VOLUME_KEYS)).filter(|&(idx, key)| code(idx) == Some(key));
    buttons.count() + volume.count()
}

/// Resolves the mapping for a pad with these key codes; `sil_button` gives SIL's raw button
/// for a key code.
pub fn resolve(map: &SdlMap, codes: &[u16], sil_button: impl Fn(u16) -> Option<i32>) -> Resolved {
    let ascending = fits(map, codes, true, &sil_button) > fits(map, codes, false, &sil_button);
    let order = sdl_order(codes, ascending);
    let raw = |idx: Option<u16>| idx.and_then(|i| order.get(i as usize)).and_then(|&c| sil_button(c));
    Resolved { buttons: map.buttons.map(raw), dpad: map.dpad.map(raw), ascending }
}

/// The mapping's analog axes on a device with axis codes `abs_codes`, ascending, in
/// AXIS_LABELS order; None where the mapping names none or the device lacks it.
pub fn resolve_axes(map: &SdlMap, abs_codes: &[u16]) -> [Option<MappedAxis>; AXES] {
    let numbered: Vec<u16> = abs_codes.iter().copied().filter(|c| !ABS_HATS.contains(c)).collect();
    let mapped = |e: AxisEntry| numbered.get(e.index as usize).map(|&code| MappedAxis { code, inverted: e.inverted });
    map.axes.map(|e| e.and_then(mapped))
}

/// Whether the mapping names an analog axis.
pub fn names_axes(map: &SdlMap) -> bool {
    map.axes.iter().any(Option::is_some)
}

/// Whether mapped axes `axes` put a stick axis off the one SIL reads.
pub fn moves_sticks(axes: &[Option<MappedAxis>; AXES]) -> bool {
    let off = |(a, sil): (&Option<MappedAxis>, u16)| a.is_some_and(|a| a.code != sil || a.inverted);
    axes[..STICK_AXES].iter().zip(SIL_STICK_AXES).any(off)
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
    fn gamepad_codes_come_first_when_more_entries_land_so() {
        let map = parse(",a:b0,b:b1,start:b11,").expect("the mapping names buttons");
        let r = resolve(&map, &MUOS_CODES, sil_button(&MUOS_CODES));
        assert!(!r.ascending);
        assert_eq!(r.buttons[Input::FaceDown as usize], Some(0));
        assert_eq!(r.buttons[Input::Start as usize], None);
    }

    #[test]
    fn a_pad_with_gamepad_codes_only_keeps_them_first() {
        let codes: Vec<u16> = (BTN_JOYSTICK..BTN_JOYSTICK + 4).collect();
        let map = parse(",a:b0,b:b1,x:b2,y:b3,").expect("the mapping names buttons");
        assert!(!resolve(&map, &codes, sil_button(&codes)).ascending);
    }

    /// Knulli on the RG35XX-H: no volume entries, the triggers and R3 past the gamepad codes
    /// under stock SDL's order.
    const RG35XXH_MAP: &str = "19000000010000000100000000010000,Anbernic RG35XX-H Controller,platform:Linux,\
        b:b3,a:b4,dpdown:h0.4,leftx:a0,lefty:a1,rightx:a2,righty:a3,lefttrigger:b13,leftstick:b12,dpleft:h0.8,\
        rightshoulder:b8,leftshoulder:b7,righttrigger:b14,rightstick:b15,dpright:h0.2,back:b9,start:b10,\
        dpup:h0.1,y:b6,x:b5,guide:b11,";
    const RG35XXH_CODES: [u16; 17] =
        [1, 114, 115, 304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 315, 316, 354];
    const RG35XXH_AXES: [u16; 6] = [2, 3, 4, 5, 16, 17];

    #[test]
    fn knulli_numbers_every_code_in_ascending_order_without_volume_entries() {
        let map = parse(RG35XXH_MAP).expect("the mapping names buttons");
        let r = resolve(&map, &RG35XXH_CODES, sil_button(&RG35XXH_CODES));
        assert!(r.ascending);
        let raw = |i: Input| r.buttons[i as usize];
        assert_eq!((raw(Input::FaceRight), raw(Input::FaceDown)), (Some(0), Some(1)));
        assert_eq!((raw(Input::FaceLeft), raw(Input::FaceUp)), (Some(2), Some(3)));
        assert_eq!((raw(Input::L1), raw(Input::R1)), (Some(4), Some(5)));
        assert_eq!((raw(Input::Select), raw(Input::Start), raw(Input::Home)), (Some(6), Some(7), Some(8)));
        assert_eq!(
            (raw(Input::L3), raw(Input::L2), raw(Input::R2), raw(Input::R3)),
            (Some(9), Some(10), Some(11), Some(12))
        );
    }

    #[test]
    fn axes_number_in_ascending_order_without_hats() {
        let map = parse(RG35XXH_MAP).expect("the mapping names buttons");
        let axes = resolve_axes(&map, &RG35XXH_AXES);
        let codes: Vec<Option<u16>> = axes.iter().map(|a| a.map(|a| a.code)).collect();
        assert_eq!(codes, [Some(2), Some(3), Some(4), Some(5), None, None], "the triggers are buttons");
        assert!(moves_sticks(&axes));
    }

    #[test]
    fn an_xbox_pad_keeps_its_sticks_on_sils_axes() {
        let map = parse(
            "030000005e0400008e02000014010000,Xbox,a:b0,leftx:a0,lefty:a1,lefttrigger:a2,\
            rightx:a3,righty:a4,righttrigger:a5,",
        )
        .expect("the mapping names buttons");
        let axes = resolve_axes(&map, &[0, 1, 2, 3, 4, 5, 16, 17]);
        assert_eq!(axes[4], Some(MappedAxis { code: 2, inverted: false }));
        assert!(!moves_sticks(&axes));
    }

    #[test]
    fn an_inverted_axis_moves_its_stick() {
        let map = parse(",a:b0,leftx:a0,lefty:a1~,").expect("the mapping names buttons");
        let axes = resolve_axes(&map, &[0, 1]);
        assert_eq!(axes[1], Some(MappedAxis { code: 1, inverted: true }));
        assert!(moves_sticks(&axes));
    }

    #[test]
    fn an_axis_the_device_lacks_is_none() {
        let map = parse(",a:b0,leftx:a0,lefty:a1,rightx:a2,").expect("the mapping names buttons");
        let axes = resolve_axes(&map, &[0, 1, 16, 17]);
        assert_eq!(axes[2], None);
        assert!(!moves_sticks(&axes));
    }

    /// Knulli on the TrimUI Smart Pro S: PortMaster puts an Xbox 360 mapping with the pad's
    /// vendor and product, another version, above the pad's own.
    const KNULLI_CONFIG: &str = "030000005e0400008e02000004010000,Microsoft Xbox 360,a:b0,b:b1,\
        back:b6,guide:b8,start:b7,x:b2,y:b3,platform:Linux,\n\
        030000005e0400008e02000014010000,TRIMUI Smart Pro S Controller,platform:Linux,a:b1,b:b0,\
        back:b8,start:b9,x:b3,y:b2,guide:b10,";
    const XBOX_IDS: (u16, u16) = (0x045e, 0x028e);

    #[test]
    fn a_pad_takes_the_mapping_under_its_name() {
        let maps = parse_all(KNULLI_CONFIG);
        assert_eq!(maps.len(), 2);
        let m = select(&maps, "TRIMUI Smart Pro S Controller", XBOX_IDS.0, XBOX_IDS.1).expect("two mappings");
        assert_eq!(m.name, "TRIMUI Smart Pro S Controller");
        assert_eq!(m.buttons[Input::FaceDown as usize], Some(1));
        assert_eq!(m.buttons[Input::FaceLeft as usize], Some(3));
    }

    #[test]
    fn an_unnamed_pad_takes_the_first_mapping_for_its_ids() {
        let maps = parse_all(KNULLI_CONFIG);
        let m = select(&maps, "Xbox 360 Wireless Receiver", XBOX_IDS.0, XBOX_IDS.1).expect("two mappings");
        assert_eq!(m.name, "Microsoft Xbox 360");
    }

    #[test]
    fn muos_keys_take_the_mapping_for_their_ids_under_another_name() {
        let maps = parse_all(MUOS_MAP);
        let m = select(&maps, "muOS-Keys", 0x0001, 0x0001).expect("the muOS mapping");
        assert_eq!(m.name, "Deeplay-keys");
    }

    #[test]
    fn a_pad_takes_no_mapping_made_for_other_ids() {
        let maps = parse_all(&format!("{MUOS_MAP}\n{KNULLI_CONFIG}"));
        assert_eq!(select(&maps, "8BitDo Pro 2", 0x2dc8, 0x6003), None);
    }

    #[test]
    fn a_mapping_without_ids_serves_any_pad() {
        let config = format!("{KNULLI_CONFIG}\n0000000058626f782047616d65706100,Name hashed,a:b0,b:b1,");
        let maps = parse_all(&config);
        let m = select(&maps, "8BitDo Pro 2", 0x2dc8, 0x6003).expect("the mapping without ids");
        assert_eq!(m.name, "Name hashed");
    }

    #[test]
    fn a_guid_carries_vendor_and_product_only_beside_zero_words() {
        assert_eq!(guid_ids("030000005e0400008e02000014010000"), Some(XBOX_IDS));
        assert_eq!(guid_ids("05000000c82d00000161000000010000"), Some((0x2dc8, 0x6101)));
        assert_eq!(guid_ids("030000005e0401008e02000014010000"), None);
        assert_eq!(guid_ids("xinput"), None);
    }

    #[test]
    fn a_mapping_without_buttons_is_none() {
        assert_eq!(parse(",leftx:a0,lefty:a1,"), None);
    }
}
