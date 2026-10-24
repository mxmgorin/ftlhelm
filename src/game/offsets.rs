//! The game's object layout, ids and key codes, from the binary's DWARF and symbol table.

/// The game's logical screen.
pub const GAME_W: i32 = 1280;
pub const GAME_H: i32 = 720;

pub const CAPP_GUI: usize = 16;
pub const CAPP_MAIN_MENU: usize = 32;
/// The Shift flag CApp::OnLoop samples once a frame and hands to the hotkeys.
pub const CAPP_SHIFT_HELD: usize = 9;
pub const CAPP_MODIFIER_X: usize = 14648;
pub const CAPP_MODIFIER_Y: usize = 14652;
pub const CAPP_MOUSE_MODIFIER_X: usize = 14668;
pub const CAPP_MOUSE_MODIFIER_Y: usize = 14672;
pub const CAPP_X_BAR: usize = 14692;
pub const CAPP_Y_BAR: usize = 14696;

pub const GUI_CREW_CONTROL: usize = 616;
pub const CREW_CONTROL_SIZE: usize = 552;
/// The HUD's top row; with a screen open, buttons above it belong to the background.
pub const HUD_TOP_HEIGHT: i32 = 90;
/// The HUD's bottom bar starts here; the screens' own buttons end above it.
pub const HUD_BOTTOM_TOP: i32 = 610;
pub const GUI_SYS_CONTROL: usize = 1168;
pub const GUI_COMBAT_CONTROL: usize = 1416;
pub const GUI_STAR_MAP: usize = 6504;
pub const GUI_SHIP_COMPLETE: usize = 6512;
pub const GUI_SHIP_POSITION: usize = 6656;
pub const GUI_CHOICE_BOX: usize = 6688;
pub const GUI_MENU_BOX: usize = 7232;
pub const MENU_BOX_SIZE: usize = 1136;
pub const GUI_OPTIONS_BOX: usize = 8592;
pub const OPTIONS_BOX_SIZE: usize = 4352;
pub const GUI_PAUSED: usize = 12944;
pub const GUI_SHIP_SCREENS: usize = 13064;
pub const GUI_STORE_SCREENS: usize = 13448;
pub const GUI_CREW_SCREEN: usize = 14552;
pub const GUI_EQUIP_SCREEN: usize = 15624;

pub const FOCUS_OPEN: usize = 8;

pub const MAIN_MENU_OPEN: usize = 0;
pub const MAIN_MENU_CHOICE_BOX: usize = 13864;

pub const STARMAP_SIZE: usize = 3280;
pub const STARMAP_LOCATIONS: usize = 40;
pub const STARMAP_CURRENT_LOC: usize = 136;
pub const STARMAP_POSITION: usize = 160;
pub const STARMAP_TRANSLATION: usize = 184;
pub const STARMAP_CHOOSING_SECTOR: usize = 2232;
pub const STARMAP_SECTOR_OFFSET: usize = 3240;
pub const STARMAP_SECTOR_HITBOXES: usize = 3256;
pub const LOCATION_LOC: usize = 0;

pub const CHOICE_TEXT: usize = 48;
pub const CHOICE_RECTS: usize = 88;
pub const CHOICE_OPEN_TIME: usize = 132;

pub const BUTTON_HITBOX: usize = 16;
pub const BUTTON_RENDER_OFF: usize = 34;
pub const BUTTON_ACTIVE: usize = 72;

pub const SYSCTRL_BOXES: usize = 16;
pub const SYSBOX_SYSTEM: usize = 144;
pub const SYSTEM_TYPE: usize = 64;
/// ShipSystem::powerState is a pair: the power in use, then the capacity.
pub const SYSTEM_POWER: usize = 80;
pub const SYSTEM_POWER_MAX: usize = SYSTEM_POWER + std::mem::size_of::<i32>();
pub const SYSTEM_ICON: usize = 96;

pub const COMBAT_WEAPON_CONTROL: usize = 32;
pub const COMBAT_DRONE_CONTROL: usize = 1056;
pub const COMBAT_CURRENT_TARGET: usize = 1984;
pub const COMBAT_POSITION: usize = 2000;
pub const COMBAT_TARGET_POSITION: usize = 2016;
pub const WEAPONCTRL_ARMED: usize = 216;
pub const WEAPONCTRL_AUTOFIRE: usize = 224;
pub const ARMAMENT_BOXES: usize = 32;
pub const ARMAMENT_LOCATION: usize = 56;
pub const WEAPON_CONTROL_SIZE: usize = 1024;
pub const DRONE_CONTROL_SIZE: usize = 880;
pub const ARMAMENTBOX_ITEM: usize = 496;
pub const WEAPON_BLUEPRINT: usize = 32;
pub const WEAPON_POWERED: usize = 242;
pub const WEAPON_POWER: usize = 244;
pub const WEAPONBP_ICON: usize = 320;
pub const DRONE_BLUEPRINT: usize = 32;
pub const DRONE_POWERED: usize = 16;
pub const DRONE_POWER: usize = 20;
pub const DRONEBP_ICON: usize = 168;
pub const BLUEPRINT_SHORT_TITLE: usize = 32;
pub const TEXTSTRING_TEXT: usize = 0;
pub const TEXTSTRING_LITERAL: usize = 8;
pub const TEXTLIB_LANGUAGE: usize = 0x60;
pub const TEXTURE_W: usize = 4;
pub const TEXTURE_H: usize = 8;

/// Armament icons as the game's weapon boxes build them: prefix, blueprint icon, suffix.
pub const WEAPON_ICON_PREFIX: &str = "systemUI/weapbox_icon_W_";
pub const DRONE_ICON_PREFIX: &str = "systemUI/weapbox_icon_D_";
pub const ICON_SUFFIX: &str = ".png";
pub const WEAPON_ICON_FALLBACK: &str = "icons/s_weapons_overlay.png";
pub const DRONE_ICON_FALLBACK: &str = "icons/s_drones_overlay.png";

pub const SHIPMANAGER_CREW: usize = 192;
pub const SHIPMANAGER_SHIP: usize = 280;
pub const COMPLETE_SHIP_MANAGER: usize = 16;
pub const CREW_ANIM: usize = 192;
pub const CREW_ROOM: usize = 156;
pub const CREW_SHIP: usize = 396;
pub const PLAYER_SHIP_ID: i32 = 0;
pub const CREW_DEAD: usize = 544;
pub const CREW_BLUEPRINT: usize = 576;
pub const CREWBP_NAME: usize = 120;
pub const CREWBP_RACE: usize = 8;
pub const CRYSTAL_RACE: &str = "crystal";
pub const CREW_SELECTED: usize = 8;
/// Crew hotkeys run F1 up to this many.
pub const CREW_HOTKEYS: usize = 8;

pub const EQUIP_BOXES: usize = 472;
/// The overflow boxes Equipment keeps in its box list, and the flags that show them.
pub const EQUIP_OVERCAPACITY_BOX: usize = 520;
pub const EQUIP_OVERAUG_BOX: usize = 528;
pub const EQUIP_OVER_CAPACITY: usize = 808;
pub const EQUIP_OVER_AUG_CAPACITY: usize = 809;
pub const CREWSCREEN_BOXES: usize = 184;
pub const EQUIPBOX_HITBOX: usize = 96;
pub const TABBED_BUTTONS: usize = 32;
pub const TABBED_CURRENT: usize = 104;

pub const SHIP_ROOMS: usize = 16;
pub const SHIP_DOORS: usize = 40;
pub const SHIP_AIRLOCKS: usize = 88;
pub const ROOM_RECT: usize = 28;
pub const ROOM_ID: usize = 44;
pub const DOOR_ROOM1: usize = 28;
pub const DOOR_ROOM2: usize = 32;
pub const DOOR_OPEN: usize = 36;
pub const DOOR_X: usize = 592;
pub const DOOR_Y: usize = 596;

pub const MOUSE_AIMING: usize = 16;
pub const MOUSE_TELEPORTING: usize = 20;
pub const MOUSE_MIND: usize = 24;
pub const MOUSE_HACKING: usize = 312;

/// CFPS::FPSControl's speed level, unused by the game itself.
pub const FPS_LEVEL: usize = 0x24;
/// The step CFPS::OnLoop last computed, in the game's frame units.
pub const FPS_STEP: usize = 0x18;

pub const VECTOR_BEGIN: usize = 0;
pub const VECTOR_END: usize = 8;

/// Game system types.
pub const SYS_SHIELDS: i32 = 0;
pub const SYS_ENGINES: i32 = 1;
pub const SYS_OXYGEN: i32 = 2;
pub const SYS_WEAPONS: i32 = 3;
pub const SYS_DRONES: i32 = 4;
pub const SYS_MEDBAY: i32 = 5;
pub const SYS_PILOT: i32 = 6;
pub const SYS_SENSORS: i32 = 7;
pub const SYS_DOORS: i32 = 8;
pub const SYS_TELEPORTER: i32 = 9;
pub const SYS_CLOAKING: i32 = 10;
pub const SYS_ARTILLERY: i32 = 11;
pub const SYS_BATTERY: i32 = 12;
pub const SYS_CLONEBAY: i32 = 13;
pub const SYS_MIND: i32 = 14;
pub const SYS_HACKING: i32 = 15;

pub const WEAPON_SLOTS: i32 = 4;
pub const DRONE_SLOTS: i32 = 3;

/// SIL's Linux tables, one entry per pad and per input device.
pub const SILJOY_SIZE: usize = 832;
pub const SILJOY_EVENT_DEV: usize = 0;
pub const SILJOY_KEYMAP_LOW: usize = 664;
pub const SILJOY_KEYMAP_HIGH: usize = 696;
pub const EVDEV_INFO_SIZE: usize = 408;
/// The device's EV_KEY bitmap, one bit per key code.
pub const EVDEV_KEY_BITS: usize = 297;
pub const EVDEV_KEY_CODES: usize = 768;

/// Key codes the game's key handlers take: SDL1 keysyms, ASCII for letters and digits.
pub const KEY_LSHIFT: i32 = 0x130;
pub const KEY_F1: i32 = 0x11a;
pub const KEY_F_COUNT: i32 = 15;
pub const KEY_SPACE: i32 = 0x20;
pub const KEY_ESCAPE: i32 = 0x1b;
pub const KEY_ENTER: i32 = 0x0d;
