/* ftlpad: a gamepad layer for FTL, loaded into the game by box64.
 *
 * The game's SIL engine scans /dev/input for joysticks and polls them every frame, but
 * nothing in the game reads the result. This library resolves the engine's joystick API
 * and the game's entry points from the executable's symbol table, hooks the game loop,
 * and drives the game from the pad: a virtual cursor fed to the mouse handlers, clicks,
 * key presses, and a Shift state the hotkey code reads. An overlay is drawn where the
 * game draws its cursor. Hook sites are diverted through a trampoline page mapped within
 * a rel32 jump of the executable, since the library itself loads far away.
 *
 * The layout comes from a gptokeyb2-style ini: [controls] is the base state, each
 * [controls:NAME] a state a hold_state button pushes while held, overlay = parent letting
 * lookups fall through to the state below. A press records what it did so its release
 * undoes exactly that, whatever state is active by then. A stick or the D-pad bound to
 * mouse_movement moves the cursor unless the active state binds its directions.
 *
 * The cursor lives in the game's logical screen; the mouse handlers take window pixels,
 * so positions pass through the inverse of the game's own transform, read from CApp at
 * offsets taken from the binary's DWARF. Key codes are the SDL1 keysyms the game's key
 * handlers expect. Everything logs to stderr under one prefix.
 *
 * Snapping: while the game waits for a target or has crew selected, the rooms of the
 * ships on screen become targets and the D-pad, in the base state, moves the cursor
 * between them, nearest in the pressed direction. Room rectangles are ship-local; the
 * player ship sits at CommandGui::shipPosition, the enemy at CombatControl's position plus
 * targetPosition, as CommandGui::GetWorldCoordinates inverts them. With the star map open
 * the targets are its beacons, at the map's position plus translation plus the location,
 * as StarMap::MouseMove inverts them, or the sector boxes while a new sector is chosen.
 *
 * Wheels: a button bound to "wheel NAME" shows, while held, eight labelled plates around
 * the cursor; either stick picks one (D-pad left and right step), A performs it, B
 * closes, D-pad up and down add and take power on power wheels, and the release closes
 * without acting. Every wheel stays open after the release; the same button or B closes
 * it, as does A on a plate that is not about power. A power wheel stays open after A and the power keys;
 * any wheel closes once the game starts aiming. Opening a wheel pauses the game and the
 * close unpauses only what the wheel paused. Crew and system wheels fill themselves from
 * the game: the living crew in list order, the installed systems in power-bar order.
 * Labels print through the game's own font code, which takes its bundled copy-on-write
 * std::string, built and destroyed through the constructors in the binary.
 *
 * A binding may carry two actions, "HOLD | TAP": a press shorter than the tap threshold
 * performs the tap, a longer one the hold. Built-in actions cover what keys cannot:
 * pause_ff, cancel, context_click, arm_next, aim_crew, repeat_ability, precision,
 * center_swap, prev_tab, next_tab. Modes are read from the game, never tracked: aiming
 * from MouseControl, crew selection from CrewControl, screens from their open flags.
 *
 * Zones for the D-pad come from every screen: rooms of the ships, event choices, the
 * star map, and the buttons the game hit-tests on mouse moves, collected by a hook on
 * GenericButton::MouseMove and refreshed with a synthetic move.
 *
 * A command FIFO named by the environment takes test commands (move, click, key, dump,
 * state) so a run can be driven without a pad.
 */
#define _GNU_SOURCE
#include <ctype.h>
#include <elf.h>
#include <fcntl.h>
#include <math.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#define EXE_ENV "FTLPAD_EXE"
#define DEBUG_ENV "FTLPAD_DEBUG"
#define CONFIG_ENV "FTLPAD_CONFIG"
#define LAYOUT_ENV "FTLPAD_LAYOUT"
#define CMD_ENV "FTLPAD_CMD"
/* PortMaster's per-device SDL mapping; its button indices match SIL's for pads whose
 * buttons all sit in the gamepad key range. */
#define SDL_MAP_ENV "SDL_GAMECONTROLLERCONFIG"
#define SDL_MAP_UNSET -1
#define FILE_POLL_S 1.0
#define EXE_FALLBACK "/proc/self/exe"
#define MAPS "/proc/self/maps"
#define LOG_PREFIX "ftlpad: "
#define TRAMPOLINE_HINT ((void *)0x70000000)
#define TRAMPOLINE_PAGE 4096
#define JMP_LEN 5
#define FAR_JMP_LEN 12
#define NOP 0x90
#define HEARTBEAT_S 10.0

/* The game's logical screen. */
#define GAME_W 1280
#define GAME_H 720

/* Game object layout, from the binary's DWARF. */
#define CAPP_GUI 16
#define CAPP_MAIN_MENU_OPEN 32
#define CAPP_MODIFIER_X 14648
#define CAPP_MODIFIER_Y 14652
#define CAPP_MOUSE_MODIFIER_X 14668
#define CAPP_MOUSE_MODIFIER_Y 14672
#define CAPP_X_BAR 14692
#define CAPP_Y_BAR 14696
#define GUI_CREW_CONTROL 616
#define CREW_CONTROL_SIZE 552
/* Small unplaced buttons the game parks in this corner. */
#define CORNER_BUTTON_MAX 40
#define CORNER_BUTTON_SIZE 24
/* The HUD's top row; with a screen open, buttons above it belong to the background. */
#define HUD_TOP_HEIGHT 90
/* The HUD's bottom bar starts here; the screens' own buttons end above it. */
#define HUD_BOTTOM_TOP 610
#define GUI_COMBAT_CONTROL 1416
#define GUI_SHIP_COMPLETE 6512
#define GUI_SHIP_POSITION 6656
#define GUI_STAR_MAP 6504
#define GUI_FTL_BUTTON 5928
#define FTL_BUTTON_SIZE 416
#define STARMAP_SIZE 3280
/* StarMap::closeButton stays active while hidden. */
#define STARMAP_CLOSE_BUTTON 1656
#define FOCUS_OPEN 8
#define STARMAP_LOCATIONS 40
#define STARMAP_POSITION 160
#define STARMAP_TRANSLATION 184
#define STARMAP_CHOOSING_SECTOR 2232
#define STARMAP_SECTOR_OFFSET 3240
#define STARMAP_SECTOR_HITBOXES 3256
#define LOCATION_LOC 0
#define GUI_CHOICE_BOX 6688
#define CHOICE_RECTS 88
#define CHOICE_TEXT 48
#define CHOICE_OPEN_TIME 132
#define BUTTON_HITBOX 16
#define BUTTON_RENDER_OFF 34
#define BUTTON_ACTIVE 72
#define GUI_SYS_CONTROL 1168
#define SYSCTRL_BOXES 16
#define SYSBOX_SYSTEM 144
#define SYSTEM_TYPE 64
#define SYSTEM_POWER 80
#define COMBAT_WEAPON_CONTROL 32
#define COMBAT_DRONE_CONTROL 1056
#define WEAPONCTRL_ARMED 216
#define WEAPONCTRL_AUTOFIRE 224
#define ARMAMENT_BOXES 32
#define ARMAMENT_LOCATION 56
#define WEAPON_CONTROL_SIZE 1024
#define DRONE_CONTROL_SIZE 880
#define ARMAMENTBOX_ITEM 496
#define WEAPON_POWERED 242
#define WEAPON_BLUEPRINT 32
#define WEAPON_POWER 244
#define WEAPONBP_ICON 320
#define SYSTEM_ICON 96
#define TEXTURE_W 4
#define TEXTURE_H 8
#define DRONE_BLUEPRINT 32
#define DRONE_POWER 20
#define DRONEBP_ICON 168
/* Armament icons as the game's weapon boxes build them: prefix, blueprint icon, suffix. */
#define WEAPON_ICON_PREFIX "systemUI/weapbox_icon_W_"
#define DRONE_ICON_PREFIX "systemUI/weapbox_icon_D_"
#define ICON_SUFFIX ".png"
/* Weapon art sits right of its texture's centre by this many texture pixels. */
#define WEAPON_ICON_SHIFT 3.0f
#define WEAPON_ICON_FALLBACK "icons/s_weapons_overlay.png"
#define DRONE_ICON_FALLBACK "icons/s_drones_overlay.png"
#define CREW_ANIM 192
/* CrewAnimation::RenderIcon draws the face small, centred on the origin. */
#define CREW_ICON_SCALE 2.0f
#define BLUEPRINT_TITLE 16
#define BLUEPRINT_SHORT_TITLE 32
#define TEXTSTRING_LITERAL 8
#define TEXTLIB_LANGUAGE 0x60
#define DRONE_POWERED 16
#define SHIPMANAGER_CREW 192
#define CREW_ROOM 156
#define CREW_SHIP 396
#define PLAYER_SHIP_ID 0
#define CREW_DEAD 544
#define CREW_BLUEPRINT 576
#define CREWBP_NAME 120
#define GUI_PAUSED 12944
#define GUI_MENU_BOX 7232
#define MENU_BOX_SIZE 1136
#define GUI_OPTIONS_BOX 8592
#define OPTIONS_BOX_SIZE 4352
#define GUI_SHIP_SCREENS 13064
#define GUI_CREW_SCREEN 14552
#define GUI_EQUIP_SCREEN 15624
#define EQUIP_BOXES 472
#define CREWSCREEN_BOXES 184
#define EQUIPBOX_HITBOX 96
#define GUI_STORE_SCREENS 13448
#define TABBED_BUTTONS 32
#define TABBED_CURRENT 104
#define SHIP_DOORS 40
#define SHIP_AIRLOCKS 88
#define DOOR_X 592
#define DOOR_Y 596
#define DOOR_VERTICAL 600
#define DOOR_ROOM1 28
#define DOOR_ROOM2 32
#define DOOR_OPEN 36
/* Game system types with a power hotkey. */
#define SYS_SHIELDS 0
#define SYS_ENGINES 1
#define SYS_OXYGEN 2
#define SYS_WEAPONS 3
#define SYS_DRONES 4
#define SYS_MEDBAY 5
#define SYS_PILOT 6
#define SYS_SENSORS 7
#define SYS_DOORS 8
#define SYS_TELEPORTER 9
#define SYS_CLOAKING 10
#define SYS_ARTILLERY 11
#define SYS_BATTERY 12
#define SYS_CLONEBAY 13
#define SYS_MIND 14
#define SYS_HACKING 15
#define WEAPON_SLOTS 4
#define DRONE_SLOTS 3
#define DOOR_HALF 10
/* CFPS::FPSControl's speed level, the field speed_patch.py raises while F12 is held. */
#define FPS_LEVEL 0x24
#define SPEED_LEVEL_MIN 1
#define SPEED_LEVEL_MAX 6
#define RUMBLE_STRENGTH 0.4f
#define RUMBLE_S 0.05f
#define HELP_FONT WHEEL_FONT
#define HELP_LINE 30.0f
#define HELP_W 760.0f
#define HELP_PAD 12.0f
#define HELP_COL_TAP 130.0f
#define HELP_COL_HOLD 430.0f
#define HELP_RULE_H 1.0f
#define COMBAT_CURRENT_TARGET 1984
#define COMBAT_POSITION 2000
#define COMBAT_TARGET_POSITION 2016
#define CREW_SELECTED 8
#define COMPLETE_SHIP_MANAGER 16
#define SHIPMANAGER_SHIP 280
#define SHIP_ROOMS 16
#define ROOM_RECT 28
#define ROOM_ID 44
#define MOUSE_AIMING 16
#define MOUSE_TELEPORTING 20
#define MOUSE_MIND 24
#define MOUSE_HACKING 312
#define VECTOR_BEGIN 0
#define VECTOR_END 8

/* Key codes the game's key handlers take: SDL1 keysyms, ASCII for letters and digits. */
#define KEY_LSHIFT 0x130
#define KEY_F1 0x11a
#define KEY_F12 0x125
#define KEY_F_COUNT 15
#define KEY_SPACE 0x20
#define KEY_ESCAPE 0x1b
#define KEY_ENTER 0x0d

/* Cursor defaults, overridden by the config file. */
#define STICK_DEADZONE 0.12f
#define STICK_DEADZONE_MAX 0.9f
/* Logical pixels per second at full deflection. */
#define CURSOR_SPEED 800.0f
#define CURSOR_CURVE 2.0f
#define CURSOR_CURVE_MIN 0.5f
#define CURSOR_CURVE_MAX 4.0f
#define STICK_KEY_ON 0.5f
#define STICK_KEY_OFF 0.3f
/* SIL reports an analog trigger as a stick axis resting at -1. */
#define TRIGGER_ON 0.0f
#define EXIT_HOLD_S 0.6
#define MAX_DT 0.05
#define OVERLAY_LINE 1.0f

#define MAX_TARGETS 64
/* Half the side of the square a beacon counts as. */
#define BEACON_HALF 12
/* Direction scoring: sideways distance counts this many times the forward distance. */
#define FOCUS_SIDE_WEIGHT 2.0f
#define MAX_STATES 8
#define MAX_WHEELS 8
#define WHEEL_SLOTS 8
#define NAME_LEN 32
#define LABEL_LEN 24
#define ICON_PATH_LEN 64
#define IMAGE_CACHE_SIZE 128
#define MAX_BUTTONS 64
/* Frames between synthetic mouse moves that refresh the button zones. */
#define BUTTON_REFRESH_FRAMES 6
#define WHEEL_ON 0.5f
#define TAP_HOLD_S 0.2
#define FF_HOLD_S 0.25
#define FLICK_ON 0.8f
#define FLICK_OFF 0.25f
/* Zero: one flick per deflection, the stick must return to centre. */
#define FLICK_REPEAT_S 0.0
/* Magnet cone half-angles: the narrow one first, the wide one when it finds nothing. */
#define CONE_NARROW_DEG 30.0f
#define CONE_WIDE_DEG 60.0f
#define BADGE_FONT 12
#define BADGE_H 30.0f
#define BADGE_PAD 14.0f
#define BADGE_SEPARATOR " / "
#define BADGE_MARGIN 8.0f
/* Square plates on a circle, leaving the middle to the names. */
#define WHEEL_RADIUS 125.0f
#define WHEEL_TILE 60.0f
/* A plate holds an icon box, and under it a row of power pips when the item takes power. */
#define TILE_ICON 40.0f
#define TILE_PAD 5.0f
#define PIP_H 5.0f
#define PIP_GAP 2.0f
#define PIP_MAX_W 9.0f
#define CORNER_ICON 22.0f
#define CORNER_INSET 2.0f
#define WHEEL_MARGIN 8.0f
/* Lines of the wheel's middle: title, then the pick's name and detail. */
#define WHEEL_LINE 22.0f
#define WHEEL_TEXT_H 18.0f
#define WHEEL_TEXT_PAD 6.0f
/* The disc under the plates reaches this far past them, drawn as scanlines this tall. */
#define WHEEL_DISC_PAD 14.0f
#define DISC_SCANLINE 4.0f
/* The game's outlined header font, as on its own buttons. */
#define WHEEL_FONT 62
#define WHEEL_BEVEL 2
/* Text baseline sits this far below a plate's top edge. */
#define WHEEL_TEXT_DY 5.0f
#define LINE_LEN 256
#define CMD_BUF 512

typedef struct {
    float r, g, b, a;
} GL_Color;

typedef struct {
    int x, y;
} Point;

typedef struct {
    int x, y, w, h;
} Rect;

enum target_kind { T_ROOM, T_ENEMY_ROOM, T_BEACON, T_SECTOR, T_CHOICE, T_BUTTON, T_DOOR, T_SLOT };
static const char *const target_kind_names[] = {"player room", "enemy room", "beacon", "sector", "choice", "button", "door", "slot"};

struct target {
    Rect rect;
    int id;
    enum target_kind kind;
};

/* SIL joystick API, resolved from the symbol table. */
static struct {
    int (*count)(void);
    int (*connected)(int);
    int (*num_sticks)(int);
    int (*button_mapping)(int, int);
    int (*button_state)(int, int);
    void (*read_stick)(int, int, float *, float *);
    void (*read_dpad)(int, int *, int *);
    void (*enable_unfocused)(int);
    void (*rumble)(int, float, float, float);
} joy;

/* Game entry points, resolved the same way. Hooked ones are kept as plain addresses. */
static struct {
    void *onloop;
    void *mouse_render;
    void *get_shift;
    void (*mouse_move)(void *, int, int, int, int, bool, bool, bool);
    void (*lbutton_down)(void *, int, int);
    void (*lbutton_up)(void *, int, int);
    void (*rbutton_down)(void *, int, int);
    void (*rbutton_up)(void *, int, int);
    void (*key_down)(void *, int);
    void (*key_up)(void *, int);
    void (*request_exit)(void *);
    void (*draw_rect_outline)(int, int, int, int, GL_Color, float);
    void (*draw_rect)(float, float, float, float, GL_Color);
    void (*set_color)(GL_Color);
    void (*blit)(void *, float, float, float, float, float, GL_Color, bool);
    void (*blit_partial)(void *, float, float, float, float, float, float, float, float, float, GL_Color, bool);
    void (*push_matrix)(void);
    void (*pop_matrix)(void);
    void (*translate)(float, float, float);
    void (*scale)(float, float, float);
    void *(*image_id)(void *, const void *);
    void *resources;
    void (*crew_icon)(void *, bool);
    int (*selected_room)(void *, int, int, bool);
    void (*string_ctor)(void *, const char *, const void *);
    void (*string_dtor)(void *);
    void (*print_center)(int, float, float, const void *);
    void (*print_left)(int, float, float, const void *);
    int (*measure_width)(int, const void *);
    void *button_move;
    int (*system_room)(void *, int);
    void *(*system_in_room)(void *, int);
    void (*set_tab)(void *, unsigned);
    void (*clear_crew)(void *);
    void (*deselect_crew)(void *, void *);
    void *fps_control;
    void (*get_text)(void *, void *, const void *, const void *);
    void *text_library;
    Point *mouse_position;
} game;

static const struct {
    const char *name;
    void **slot;
} symbols[] = {
    {"input_joystick_count", (void **)&joy.count},
    {"input_joystick_connected", (void **)&joy.connected},
    {"input_joystick_num_sticks", (void **)&joy.num_sticks},
    {"input_joystick_button_mapping", (void **)&joy.button_mapping},
    {"input_joystick_button_state", (void **)&joy.button_state},
    {"input_joystick_read_stick", (void **)&joy.read_stick},
    {"input_joystick_read_dpad", (void **)&joy.read_dpad},
    {"input_enable_unfocused_joystick", (void **)&joy.enable_unfocused},
    {"input_joystick_rumble", (void **)&joy.rumble},
    {"_ZN4CFPS10FPSControlE", (void **)&game.fps_control},
    {"_ZN11TextLibrary7GetTextERKSsS1_", (void **)&game.get_text},
    {"_ZN7Globals7LibraryE", (void **)&game.text_library},
    {"_ZN4CApp6OnLoopEv", &game.onloop},
    {"_ZN12MouseControl8OnRenderEv", &game.mouse_render},
    {"_ZN6CEvent13GetShiftStateEv", &game.get_shift},
    {"_ZN4CApp11OnMouseMoveEiiiibbb", (void **)&game.mouse_move},
    {"_ZN4CApp13OnLButtonDownEii", (void **)&game.lbutton_down},
    {"_ZN4CApp11OnLButtonUpEii", (void **)&game.lbutton_up},
    {"_ZN4CApp13OnRButtonDownEii", (void **)&game.rbutton_down},
    {"_ZN4CApp11OnRButtonUpEii", (void **)&game.rbutton_up},
    {"_ZN4CApp9OnKeyDownEi", (void **)&game.key_down},
    {"_ZN4CApp7OnKeyUpEi", (void **)&game.key_up},
    {"_ZN4CApp13OnRequestExitEv", (void **)&game.request_exit},
    {"_ZN8CSurface18GL_DrawRectOutlineEiiii8GL_Colorf", (void **)&game.draw_rect_outline},
    {"_ZN4Ship17GetSelectedRoomIdEiib", (void **)&game.selected_room},
    {"_ZN8CSurface11GL_DrawRectEffff8GL_Color", (void **)&game.draw_rect},
    {"_ZN8CSurface11GL_SetColorE8GL_Color", (void **)&game.set_color},
    {"_ZN8CSurface12GL_BlitImageEP10GL_Texturefffff8GL_Colorb", (void **)&game.blit},
    {"_ZN8CSurface19GL_BlitImagePartialEP10GL_Texturefffffffff8GL_Colorb", (void **)&game.blit_partial},
    {"_ZN8CSurface13GL_PushMatrixEv", (void **)&game.push_matrix},
    {"_ZN8CSurface12GL_PopMatrixEv", (void **)&game.pop_matrix},
    {"_ZN8CSurface12GL_TranslateEfff", (void **)&game.translate},
    {"_ZN8CSurface8GL_ScaleEfff", (void **)&game.scale},
    {"_ZN15ResourceControl10GetImageIdERKSs", (void **)&game.image_id},
    {"_ZN15ResourceControl15GlobalResourcesE", (void **)&game.resources},
    {"_ZN13CrewAnimation10RenderIconEb", (void **)&game.crew_icon},
    {"_ZNSsC1EPKcRKSaIcE", (void **)&game.string_ctor},
    {"_ZNSsD1Ev", (void **)&game.string_dtor},
    {"_ZN8freetype16easy_printCenterEiffRKSs", (void **)&game.print_center},
    {"_ZN8freetype10easy_printEiffRKSs", (void **)&game.print_left},
    {"_ZN8freetype17easy_measureWidthEiRKSs", (void **)&game.measure_width},
    {"_ZN13GenericButton9MouseMoveEiib", &game.button_move},
    {"_ZN11ShipManager13GetSystemRoomEi", (void **)&game.system_room},
    {"_ZN11ShipManager15GetSystemInRoomEi", (void **)&game.system_in_room},
    {"_ZN12TabbedWindow6SetTabEj", (void **)&game.set_tab},
    {"_ZN11CrewControl17ClearSelectedCrewEv", (void **)&game.clear_crew},
    {"_ZN11CrewControl12DeselectCrewEP10CrewMember", (void **)&game.deselect_crew},
    {"_ZN12MouseControl5MouseE", (void **)&game.mouse_position},
};

/* Digital inputs: SIL's named buttons in enum order, then the D-pad and both sticks read
 * as four directions each. */
enum input {
    IN_HOME, IN_START, IN_SELECT, IN_FACE_UP, IN_FACE_LEFT, IN_FACE_RIGHT, IN_FACE_DOWN,
    IN_L1, IN_R1, IN_L2, IN_R2, IN_L3, IN_R3,
    IN_DPAD_LEFT, IN_DPAD_UP, IN_DPAD_RIGHT, IN_DPAD_DOWN,
    IN_LSTICK_LEFT, IN_LSTICK_UP, IN_LSTICK_RIGHT, IN_LSTICK_DOWN,
    IN_RSTICK_LEFT, IN_RSTICK_UP, IN_RSTICK_RIGHT, IN_RSTICK_DOWN,
    IN_COUNT
};
#define SIL_BUTTONS (IN_R3 + 1)
#define LEFT_STICK 0
#define RIGHT_STICK 1
#define TRIGGER_STICK 2

/* Whole-pointer sources a state can bind to mouse_movement. */
enum slot { SLOT_DPAD, SLOT_LSTICK, SLOT_RSTICK, SLOT_COUNT };
static const enum input slot_first[SLOT_COUNT] = {IN_DPAD_LEFT, IN_LSTICK_LEFT, IN_RSTICK_LEFT};
static const char *const slot_ini_names[SLOT_COUNT] = {"dpad", "left_analog", "right_analog"};

/* Input names as the ini spells them; face buttons by label, A being FACE_DOWN. */
static const char *const input_ini_names[IN_COUNT] = {
    "guide", "start", "back", "y", "x", "b", "a", "l1", "r1", "l2", "r2", "l3", "r3",
    "left", "up", "right", "down",
    "left_analog_left", "left_analog_up", "left_analog_right", "left_analog_down",
    "right_analog_left", "right_analog_up", "right_analog_right", "right_analog_down",
};

enum act_kind { ACT_NONE, ACT_KEY, ACT_LCLICK, ACT_RCLICK, ACT_STATE, ACT_WHEEL, ACT_BUILTIN };

enum builtin {
    BI_PAUSE_FF, BI_CANCEL, BI_CONTEXT_CLICK, BI_ARM_NEXT, BI_AIM_CREW, BI_REPEAT_ABILITY,
    BI_PRECISION, BI_CENTER_SWAP, BI_PREV_TAB, BI_NEXT_TAB, BI_HELP, BI_ROOM_DOORS, BI_PAUSE,
    BI_FAST_FORWARD, BI_DPAD_MODE, BI_COUNT
};
static const char *const builtin_names[BI_COUNT] = {
    "pause_ff", "cancel", "context_click", "arm_next", "aim_crew", "repeat_ability",
    "precision", "center_swap", "prev_tab", "next_tab", "help", "room_doors", "pause",
    "fast_forward", "dpad_mode",
};
static const char *const builtin_titles[BI_COUNT] = {
    "Pause, hold: fast", "Cancel", "Click", "Next weapon", "Next crew / tab",
    "Repeat ability", "Doors mode", "Other ship", "Previous tab", "Next tab", "Help",
    "Room doors", "Pause", "Fast forward", "Cursor / zones",
};
static const char *const input_titles[IN_COUNT] = {
    "Guide", "Start", "Select", "Y", "X", "B", "A", "L1", "R1", "L2", "R2", "L3", "R3",
    "Left", "Up", "Right", "Down", "LS left", "LS up", "LS right", "LS down",
    "RS left", "RS up", "RS right", "RS down",
};

struct binding {
    enum act_kind kind;
    int key;
    bool shift;
    /* State or wheel index, or the builtin id. */
    int state;
    /* The tap half of "HOLD | TAP"; kind ACT_NONE when there is none. */
    struct binding *tap;
};

/* Whole-pointer sources: mouse_movement moves the cursor, flick is the magnet. */
enum slot_mode { SLOT_KEYS, SLOT_MOUSE, SLOT_FLICK };

struct state {
    char name[NAME_LEN];
    bool parent;
    bool bound[IN_COUNT];
    struct binding bind[IN_COUNT];
    struct binding taps[IN_COUNT];
    enum slot_mode slot[SLOT_COUNT];
};

/* Eight plates clockwise from the top. */
static const char *const wheel_slot_names[WHEEL_SLOTS] = {
    "up", "up_right", "right", "down_right", "down", "down_left", "left", "up_left",
};

/* power: A sends the key, B the key with Shift, the wheel stays open; action and crew
 * close after A; systems and crew fill their plates from the game. */
enum wheel_kind { WK_ACTION, WK_POWER, WK_CREW, WK_SYSTEMS, WK_SETTINGS };
static const char *const wheel_kind_names[] = {"action", "power", "crew", "systems", "settings"};

struct plate {
    bool bound;
    bool available;
    bool powered;
    /* Icon texture, with the region in texture pixels when partial; a layout may name
     * the path and region. */
    char icon_path[ICON_PATH_LEN];
    void *icon;
    bool icon_partial;
    float icon_x, icon_y, icon_w, icon_h;
    /* Dark art that needs a light box under it, as in the game's weapon boxes. */
    bool icon_well;
    /* How far the art sits right of the texture's centre, in texture pixels. */
    float icon_shift;
    /* A crew member's animation, drawn as the game's portrait in place of an icon. */
    void *portrait;
    /* A small icon in the plate's corner, over its main icon. */
    void *corner_icon;
    /* A slot with nothing in it, drawn as a faint frame. */
    bool empty;
    /* Power pips: how many, and how many lit. */
    int pips, pips_on;
    struct binding bind;
    char label[LABEL_LEN];
    char badge[LABEL_LEN];
};

struct wheel {
    char name[NAME_LEN];
    enum wheel_kind kind;
    struct plate plates[WHEEL_SLOTS];
};

static const struct {
    const char *name;
    int code;
} key_names[] = {
    {"space", 32}, {"enter", 13}, {"return", 13}, {"esc", 27}, {"escape", 27}, {"tab", 9},
    {"backspace", 8}, {"delete", 127}, {"up", 273}, {"down", 274}, {"right", 275},
    {"left", 276}, {"insert", 277}, {"home", 278}, {"end", 279}, {"pageup", 280},
    {"pagedown", 281}, {"shift", KEY_LSHIFT}, {"lshift", KEY_LSHIFT}, {"rshift", 303},
    {"ctrl", 306}, {"lctrl", 306}, {"alt", 308}, {"lalt", 308}, {"/", 47}, {"-", 45},
    {"=", 61}, {",", 44}, {".", 46}, {";", 59}, {"'", 39}, {"[", 91}, {"]", 93}, {"\\", 92},
    {"`", 96}, {"+", 43},
};

/* Used when no layout file is named. */
static const char default_layout[] =
    "[controls]\n"
    "a = space\n"
    "back = esc\n"
    "start = j\n"
    "l2 = mouse_right\n"
    "r2 = mouse_left\n"
    "right_analog = mouse_movement\n";

/* The game's palette: dark teal panels, pale green frames, yellow for the selection,
 * green for powered items. */
static const GL_Color plate_color = {0.05f, 0.11f, 0.1f, 0.9f};
static const GL_Color plate_selected_color = {0.42f, 0.38f, 0.1f, 0.95f};
static const GL_Color plate_edge_color = {0.63f, 0.9f, 0.76f, 0.95f};
static const GL_Color plate_selected_edge_color = {0.96f, 0.84f, 0.28f, 1.0f};
static const GL_Color plate_bevel_color = {0.0f, 0.05f, 0.05f, 0.9f};
/* Wheel plates: the pick solid, the rest see-through. */
static const GL_Color middle_color = {0.0f, 0.03f, 0.03f, 0.6f};
static const GL_Color disc_color = {0.02f, 0.06f, 0.06f, 0.5f};
static const GL_Color plate_disabled_color = {0.05f, 0.07f, 0.07f, 0.6f};
static const GL_Color plate_empty_edge_color = {0.5f, 0.56f, 0.54f, 0.7f};
static const GL_Color corner_color = {0.0f, 0.03f, 0.03f, 0.8f};
static const GL_Color well_color = {0.7f, 0.76f, 0.72f, 1.0f};
static const GL_Color well_powered_color = {0.55f, 0.88f, 0.55f, 1.0f};
static const GL_Color plate_idle_color = {0.05f, 0.11f, 0.1f, 0.75f};
static const GL_Color plate_powered_color = {0.12f, 0.36f, 0.2f, 0.8f};
static const GL_Color plate_disabled_edge_color = {0.3f, 0.34f, 0.33f, 0.8f};
static const GL_Color text_color = {1.0f, 1.0f, 1.0f, 1.0f};
static const GL_Color text_disabled_color = {0.42f, 0.46f, 0.45f, 1.0f};
static const GL_Color pip_on_color = {0.47f, 0.95f, 0.47f, 1.0f};

/* mov 0x10(%rdi),%eax; movzbl 0x49(%rdi),%r8d */
static const unsigned char button_move_prologue[] = {0x8b, 0x47, 0x10, 0x44, 0x0f, 0xb6, 0x47, 0x49};
/* push %rbp; push %rbx; mov %rdi,%rbx */
static const unsigned char onloop_prologue[] = {0x55, 0x53, 0x48, 0x89, 0xfb};
/* push %r15; mov %rdi,%r15 */
static const unsigned char mouse_render_prologue[] = {0x41, 0x57, 0x49, 0x89, 0xff};
/* sub $0x8,%rsp; mov $KEY_LEFTSHIFT,%edi */
static const unsigned char get_shift_prologue[] = {0x48, 0x83, 0xec, 0x08, 0xbf, 0x4b, 0x00, 0x00, 0x00};

static void (*onloop_orig)(void *);
static void (*mouse_render_orig)(void *);
static int (*get_shift_orig)(void);
static void (*button_move_orig)(void *, int, int, bool);

static struct {
    float cursor_speed;
    float cursor_deadzone;
    float cursor_curve;
    bool overlay;
    bool snap_dpad;
    bool autopause_wheel;
    bool autopause_aim;
    bool precision;
    int speed_level;
    bool rumble;
    bool help;
    float flick_on;
    float flick_repeat;
    /* Face buttons by label: -1 from the SDL mapping, with the swaps below when there is
     * none; 1 forces the swaps, 0 forces none. */
    int nintendo;
    /* Which raw pairs to swap for the labels: A/B (east/south) and X/Y (north/west). */
    bool swap_ab;
    bool swap_xy;
} cfg = {CURSOR_SPEED, STICK_DEADZONE, CURSOR_CURVE, true, true, true, false, false, 3, true, false, -1, true, false, FLICK_ON, FLICK_REPEAT_S};

/* SIL button index per face label from the SDL mapping, or SDL_MAP_UNSET. */
static int sdl_face[4] = {SDL_MAP_UNSET, SDL_MAP_UNSET, SDL_MAP_UNSET, SDL_MAP_UNSET};
static const char *const sdl_face_names[4] = {"a", "b", "x", "y"};
static const enum input sdl_face_inputs[4] = {IN_FACE_DOWN, IN_FACE_RIGHT, IN_FACE_LEFT, IN_FACE_UP};

static bool debug;
static void *app;
static int cmd_fd = -1;
/* Fonts and colours of the overlay; the plate font is tunable from the command channel. */
static int plate_font = WHEEL_FONT;

static struct state states[MAX_STATES];
static int nstates;
static struct wheel wheels[MAX_WHEELS];
static int nwheels;
/* The open wheel, the input holding it and the plate the left stick points at. */
static int wheel_open = -1;
static enum input wheel_by;
static int wheel_sel = -1;
/* Wheels stay open after the release and close on A's action, B or their own button. */
static bool wheel_sticky;
/* Whether the open wheel paused the game, so its close unpauses. */
static bool wheel_paused;
/* Whether the game was already aiming when the wheel opened; only a new aim closes it. */
static bool wheel_opened_aiming;
/* The abilities key X repeats on a tap. */
static int last_ability_key;
static bool fast_forward;
/* dpad_mode toggled: the D-pad and the sticks move the cursor instead of walking zones. */
static bool dpad_mouse;
static double pause_ff_since;
static bool prev_aiming;
static const void *prev_crew;
static double flick_last[2];
static bool flick_armed[2] = {true, true};
/* Presses waiting to turn into a tap or a hold. */
static struct {
    bool pending;
    double since;
} pending[IN_COUNT];
/* Buttons the game hit-tested since the last frame, and the last full set. */
static Rect buttons_seen[MAX_BUTTONS], buttons[MAX_BUTTONS];
static const void *button_objects_seen[MAX_BUTTONS], *button_objects[MAX_BUTTONS];
static int nbuttons_seen, nbuttons;
/* Pushed states above the base, with the input holding each. */
static struct {
    int state;
    enum input by;
} stack[MAX_STATES];
static int depth;

static bool held[IN_COUNT];
/* What each held input did when pressed, undone on release. */
static struct {
    enum act_kind kind;
    int key;
    bool shift;
    int state;
} active[IN_COUNT];
static int shift_count;
static double chord_since;
static bool exit_requested;

static struct target targets[MAX_TARGETS];
static int ntargets;

static struct {
    float x, y;
    int last_x, last_y;
    int last_wx, last_wy;
    Point last_set;
    bool synced, l_held, r_held;
} cursor;

static double now_s(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec / 1e9;
}

static void logmsg(const char *fmt, ...)
{
    static double start;
    if (start == 0)
        start = now_s();
    va_list ap;
    va_start(ap, fmt);
    fprintf(stderr, LOG_PREFIX "%7.2f ", now_s() - start);
    vfprintf(stderr, fmt, ap);
    fputc('\n', stderr);
    fflush(stderr);
    va_end(ap);
}

static float clampf(float v, float lo, float hi)
{
    return fminf(fmaxf(v, lo), hi);
}

/* ---- symbol resolution and hooking ---- */

static int read_at(int fd, void *buf, size_t len, off_t at)
{
    return pread(fd, buf, len, at) == (ssize_t)len;
}

/* Fills the symbol slots from the executable's .symtab; returns the number left unresolved. */
static int resolve_symbols(void)
{
    const char *path = getenv(EXE_ENV);
    int fd = open(path ? path : EXE_FALLBACK, O_RDONLY);
    size_t wanted = sizeof symbols / sizeof *symbols;
    if (fd < 0) {
        logmsg("cannot open the executable");
        return (int)wanted;
    }
    Elf64_Ehdr eh;
    Elf64_Shdr *sh = NULL;
    if (!read_at(fd, &eh, sizeof eh, 0) || eh.e_shentsize != sizeof *sh)
        goto out;
    sh = calloc(eh.e_shnum, sizeof *sh);
    if (!sh || !read_at(fd, sh, eh.e_shnum * sizeof *sh, eh.e_shoff))
        goto out;
    for (int i = 0; i < eh.e_shnum; i++) {
        if (sh[i].sh_type != SHT_SYMTAB)
            continue;
        Elf64_Shdr *st = &sh[sh[i].sh_link];
        char *strtab = malloc(st->sh_size);
        Elf64_Sym *syms = malloc(sh[i].sh_size);
        if (strtab && syms && read_at(fd, strtab, st->sh_size, st->sh_offset)
                && read_at(fd, syms, sh[i].sh_size, sh[i].sh_offset)) {
            for (size_t s = 0; s < sh[i].sh_size / sizeof *syms; s++) {
                if (!syms[s].st_value || syms[s].st_name >= st->sh_size)
                    continue;
                for (size_t w = 0; w < wanted; w++)
                    if (!*symbols[w].slot && strcmp(strtab + syms[s].st_name, symbols[w].name) == 0)
                        *symbols[w].slot = (void *)syms[s].st_value;
            }
        }
        free(strtab);
        free(syms);
    }
out:
    free(sh);
    close(fd);
    int missing = 0;
    for (size_t w = 0; w < wanted; w++)
        if (!*symbols[w].slot) {
            logmsg("symbol missing: %s", symbols[w].name);
            missing++;
        }
    return missing;
}

/* Checks that addr lies in an executable mapping of this process; a preloaded library
 * also lands in the shell or wrapper that launches the game. */
static int mapped_executable(const void *addr)
{
    FILE *f = fopen(MAPS, "r");
    if (!f)
        return 0;
    int found = 0;
    char line[512];
    while (!found && fgets(line, sizeof line, f)) {
        unsigned long lo, hi;
        char perms[8];
        if (sscanf(line, "%lx-%lx %7s", &lo, &hi, perms) == 3 && perms[2] == 'x')
            found = lo <= (uintptr_t)addr && (uintptr_t)addr < hi;
    }
    fclose(f);
    return found;
}

static int protect(void *at, size_t len, int prot)
{
    uintptr_t page = (uintptr_t)sysconf(_SC_PAGESIZE);
    uintptr_t start = (uintptr_t)at & ~(page - 1);
    uintptr_t end = ((uintptr_t)at + len + page - 1) & ~(page - 1);
    return mprotect((void *)start, end - start, prot);
}

static int fits_rel32(const void *from, const void *to)
{
    intptr_t d = (intptr_t)to - (intptr_t)from;
    return d >= INT32_MIN && d <= INT32_MAX;
}

static void put_jmp(unsigned char *at, const void *to)
{
    int32_t rel = (int32_t)((intptr_t)to - (intptr_t)(at + JMP_LEN));
    at[0] = 0xe9;
    memcpy(at + 1, &rel, sizeof rel);
}

/* mov $to,%rax; jmp *%rax */
static void put_far_jmp(unsigned char *at, const void *to)
{
    uint64_t abs = (uint64_t)(uintptr_t)to;
    at[0] = 0x48;
    at[1] = 0xb8;
    memcpy(at + 2, &abs, sizeof abs);
    at[10] = 0xff;
    at[11] = 0xe0;
}

static unsigned char *trampolines;
static size_t trampolines_used;

/* Diverts fn to repl after checking its first bytes; returns an entry that runs the
 * original, or NULL. The prologue must be whole instructions with no relative operands. */
static void *hook(void *fn, void *repl, const unsigned char *prologue, size_t len)
{
    if (len < JMP_LEN || memcmp(fn, prologue, len) != 0)
        return NULL;
    if (!trampolines) {
        trampolines = mmap(TRAMPOLINE_HINT, TRAMPOLINE_PAGE, PROT_READ | PROT_WRITE | PROT_EXEC,
                           MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        if (trampolines == MAP_FAILED) {
            trampolines = NULL;
            return NULL;
        }
    }
    size_t need = FAR_JMP_LEN + len + JMP_LEN;
    if (trampolines_used + need > TRAMPOLINE_PAGE || !fits_rel32(fn, trampolines))
        return NULL;
    unsigned char *entry = trampolines + trampolines_used;
    unsigned char *orig = entry + FAR_JMP_LEN;
    put_far_jmp(entry, repl);
    memcpy(orig, prologue, len);
    put_jmp(orig + len, (unsigned char *)fn + len);
    trampolines_used += need;

    if (protect(fn, len, PROT_READ | PROT_WRITE | PROT_EXEC) != 0)
        return NULL;
    put_jmp(fn, entry);
    memset((unsigned char *)fn + JMP_LEN, NOP, len - JMP_LEN);
    protect(fn, len, PROT_READ | PROT_EXEC);
    return orig;
}

/* ---- files: config and layout ---- */

/* Reads the config file named by the environment: "key = value" lines, '#' comments. */
static void load_config(const char *path)
{
    FILE *f = fopen(path, "r");
    if (!f)
        return;
    char line[LINE_LEN];
    while (fgets(line, sizeof line, f)) {
        char *hash = strchr(line, '#');
        if (hash)
            *hash = '\0';
        char key[NAME_LEN];
        float value;
        if (sscanf(line, " %31[a-z_] = %f", key, &value) != 2)
            continue;
        if (strcmp(key, "cursor_speed") == 0)
            cfg.cursor_speed = fmaxf(value, 1.0f);
        else if (strcmp(key, "cursor_deadzone") == 0)
            cfg.cursor_deadzone = clampf(value, 0.0f, STICK_DEADZONE_MAX);
        else if (strcmp(key, "cursor_curve") == 0)
            cfg.cursor_curve = clampf(value, CURSOR_CURVE_MIN, CURSOR_CURVE_MAX);
        else if (strcmp(key, "overlay") == 0)
            cfg.overlay = value != 0;
        else if (strcmp(key, "snap_dpad") == 0)
            cfg.snap_dpad = value != 0;
        else if (strcmp(key, "autopause_wheel") == 0)
            cfg.autopause_wheel = value != 0;
        else if (strcmp(key, "autopause_aim") == 0)
            cfg.autopause_aim = value != 0;
        else if (strcmp(key, "speed_level") == 0)
            cfg.speed_level = (int)clampf(value, SPEED_LEVEL_MIN, SPEED_LEVEL_MAX);
        else if (strcmp(key, "rumble") == 0)
            cfg.rumble = value != 0;
        else if (strcmp(key, "nintendo_labels") == 0)
            cfg.nintendo = (int)value;
        else if (strcmp(key, "swap_ab") == 0)
            cfg.swap_ab = value != 0;
        else if (strcmp(key, "swap_xy") == 0)
            cfg.swap_xy = value != 0;
        else if (strcmp(key, "flick_threshold") == 0)
            cfg.flick_on = clampf(value, FLICK_OFF + 0.1f, 1.0f);
        else if (strcmp(key, "flick_repeat") == 0)
            cfg.flick_repeat = fmaxf(value, 0.0f);
        else
            logmsg("config: unknown key %s", key);
    }
    fclose(f);
    logmsg("config: cursor_speed=%.0f cursor_deadzone=%.2f cursor_curve=%.1f overlay=%d snap_dpad=%d autopause_wheel=%d autopause_aim=%d speed_level=%d rumble=%d nintendo_labels=%d",
           cfg.cursor_speed, cfg.cursor_deadzone, cfg.cursor_curve, cfg.overlay, cfg.snap_dpad,
           cfg.autopause_wheel, cfg.autopause_aim, cfg.speed_level, cfg.rumble, cfg.nintendo);
}

static int find_wheel(struct wheel *set, int *count, const char *name)
{
    for (int i = 0; i < *count; i++)
        if (strcmp(set[i].name, name) == 0)
            return i;
    if (*count >= MAX_WHEELS)
        return -1;
    struct wheel *w = &set[(*count)++];
    memset(w, 0, sizeof *w);
    strncpy(w->name, name, NAME_LEN - 1);
    return *count - 1;
}

static int find_state(struct state *set, int *count, const char *name)
{
    for (int i = 0; i < *count; i++)
        if (strcmp(set[i].name, name) == 0)
            return i;
    if (*count >= MAX_STATES)
        return -1;
    struct state *s = &set[(*count)++];
    memset(s, 0, sizeof *s);
    strncpy(s->name, name, NAME_LEN - 1);
    return *count - 1;
}

static int lookup_name(const char *const *names, int n, const char *name)
{
    for (int i = 0; i < n; i++)
        if (strcmp(names[i], name) == 0)
            return i;
    return -1;
}

/* Maps an ini key name to the game's key code; returns 0 for an unknown name. */
static int key_code(const char *name)
{
    size_t len = strlen(name);
    if (len == 1 && (islower((unsigned char)name[0]) || isdigit((unsigned char)name[0])))
        return name[0];
    if (name[0] == 'f' && len <= 3 && isdigit((unsigned char)name[1])) {
        int n = atoi(name + 1);
        if (n >= 1 && n <= KEY_F_COUNT)
            return KEY_F1 + n - 1;
    }
    for (size_t i = 0; i < sizeof key_names / sizeof *key_names; i++)
        if (strcmp(key_names[i].name, name) == 0)
            return key_names[i].code;
    return 0;
}

static char *trim(char *s)
{
    while (isspace((unsigned char)*s))
        s++;
    char *end = s + strlen(s);
    while (end > s && isspace((unsigned char)end[-1]))
        *--end = '\0';
    return s;
}

struct layout_set {
    struct state states[MAX_STATES];
    int nstates;
    struct wheel wheels[MAX_WHEELS];
    int nwheels;
};

/* Parses one action ("key [add_shift]", mouse_left, mouse_right, hold_state NAME, wheel
 * NAME, a builtin name) for the named input; returns false when nothing usable was found. */
static bool parse_action(struct layout_set *set, const char *key, char *value, struct binding *out)
{
    struct binding b = {ACT_NONE, 0, false, 0, NULL};
    char *save = NULL;
    char *tok = strtok_r(value, " \t", &save);
    if (!tok)
        return false;
    int bi = lookup_name(builtin_names, BI_COUNT, tok);
    if (bi >= 0) {
        b.kind = ACT_BUILTIN;
        b.state = bi;
    } else if (strcmp(tok, "mouse_left") == 0) {
        b.kind = ACT_LCLICK;
    } else if (strcmp(tok, "mouse_right") == 0) {
        b.kind = ACT_RCLICK;
    } else if (strcmp(tok, "hold_state") == 0 || strcmp(tok, "wheel") == 0) {
        bool wheel = tok[0] == 'w';
        char *name = strtok_r(NULL, " \t", &save);
        int idx = !name ? -1 : wheel ? find_wheel(set->wheels, &set->nwheels, name)
                                     : find_state(set->states, &set->nstates, name);
        if (idx < 0) {
            logmsg("layout: bad %s for %s", tok, key);
            return false;
        }
        b.kind = wheel ? ACT_WHEEL : ACT_STATE;
        b.state = idx;
    } else {
        b.key = key_code(tok);
        if (!b.key) {
            logmsg("layout: unknown key %s for %s", tok, key);
            return false;
        }
        b.kind = ACT_KEY;
    }
    while ((tok = strtok_r(NULL, " \t", &save))) {
        if (strcmp(tok, "add_shift") == 0)
            b.shift = true;
        else
            logmsg("layout: %s ignored for %s", tok, key);
    }
    *out = b;
    return true;
}

/* Parses "HOLD | TAP" or a single action into the hold binding and its tap half. */
static bool parse_value(struct layout_set *set, const char *key, char *value, struct binding *out,
                        struct binding *tap)
{
    char *bar = strchr(value, '|');
    tap->kind = ACT_NONE;
    if (bar) {
        *bar = '\0';
        if (!parse_action(set, key, trim(bar + 1), tap))
            tap->kind = ACT_NONE;
    }
    bool ok = parse_action(set, key, trim(value), out);
    out->tap = tap->kind != ACT_NONE ? tap : NULL;
    return ok;
}

/* Parses one "input = value" line of a controls section into the state. */
static void parse_binding(struct layout_set *set, struct state *s, char *key, char *value)
{
    if (strcmp(key, "overlay") == 0) {
        s->parent = strcmp(value, "parent") == 0;
        return;
    }
    int slot = lookup_name(slot_ini_names, SLOT_COUNT, key);
    if (slot >= 0) {
        if (strcmp(value, "mouse_movement") == 0)
            s->slot[slot] = SLOT_MOUSE;
        else if (strcmp(value, "flick") == 0)
            s->slot[slot] = SLOT_FLICK;
        else {
            s->slot[slot] = SLOT_KEYS;
            if (*value)
                logmsg("layout: %s = %s not supported", key, value);
        }
        return;
    }
    int in = lookup_name(input_ini_names, IN_COUNT, key);
    if (in < 0) {
        logmsg("layout: unknown input %s", key);
        return;
    }
    s->bound[in] = parse_value(set, key, value, &s->bind[in], &s->taps[in]);
}

/* Parses a plate icon, "PATH" or "PATH@X,Y,W,H" with a region in texture pixels. */
static void parse_icon(struct plate *p, char *spec)
{
    char *at = strchr(spec, '@');
    if (at) {
        *at = '\0';
        p->icon_partial = sscanf(at + 1, "%f,%f,%f,%f", &p->icon_x, &p->icon_y, &p->icon_w, &p->icon_h) == 4;
        if (!p->icon_partial)
            logmsg("layout: bad icon region %s", at + 1);
    }
    strncpy(p->icon_path, spec, ICON_PATH_LEN - 1);
}

/* Parses one line of a wheel section: "kind = NAME" or a direction with a binding, a
 * quoted label and an optional icon. */
static void parse_wheel_line(struct layout_set *set, struct wheel *w, char *key, char *value)
{
    if (strcmp(key, "kind") == 0) {
        int k = lookup_name(wheel_kind_names, (int)(sizeof wheel_kind_names / sizeof *wheel_kind_names), value);
        if (k < 0)
            logmsg("layout: unknown wheel kind %s", value);
        else
            w->kind = (enum wheel_kind)k;
        return;
    }
    int slot = lookup_name(wheel_slot_names, WHEEL_SLOTS, key);
    if (slot < 0) {
        logmsg("layout: unknown wheel direction %s", key);
        return;
    }
    struct plate *p = &w->plates[slot];
    char *quote = strchr(value, '"');
    if (quote) {
        char *end = strchr(quote + 1, '"');
        if (end) {
            *end = '\0';
            char *icon = trim(end + 1);
            if (*icon)
                parse_icon(p, icon);
        }
        strncpy(p->label, quote + 1, LABEL_LEN - 1);
        *quote = '\0';
    }
    struct binding tap;
    p->bound = parse_value(set, key, value, &p->bind, &tap);
    p->bind.tap = NULL;
    p->available = true;
    if (p->bound && !p->label[0])
        strncpy(p->label, key, LABEL_LEN - 1);
}

/* Parses layout text in the gptokeyb2 ini format, plus [wheel:NAME] sections; the base
 * state is index 0. Labels in quotes are kept intact, a '#' only starts a comment outside
 * them. */
static void parse_layout(char *text, struct layout_set *set)
{
    memset(set, 0, sizeof *set);
    int base = find_state(set->states, &set->nstates, "controls");
    struct state *state = NULL;
    struct wheel *wheel = NULL;
    char *save = NULL;
    for (char *line = strtok_r(text, "\n", &save); line; line = strtok_r(NULL, "\n", &save)) {
        bool quoted = false;
        for (char *c = line; *c; c++) {
            if (*c == '"')
                quoted = !quoted;
            else if (*c == '#' && !quoted) {
                *c = '\0';
                break;
            }
        }
        line = trim(line);
        if (!*line)
            continue;
        if (*line == '[') {
            char *end = strchr(line, ']');
            if (end)
                *end = '\0';
            state = NULL;
            wheel = NULL;
            if (strcmp(line + 1, "controls") == 0) {
                state = &set->states[base];
            } else if (strncmp(line + 1, "controls:", 9) == 0) {
                int idx = find_state(set->states, &set->nstates, line + 10);
                state = idx >= 0 ? &set->states[idx] : NULL;
            } else if (strncmp(line + 1, "wheel:", 6) == 0) {
                int idx = find_wheel(set->wheels, &set->nwheels, line + 7);
                wheel = idx >= 0 ? &set->wheels[idx] : NULL;
            }
            continue;
        }
        char *eq = strchr(line, '=');
        if (!eq)
            continue;
        *eq = '\0';
        if (state)
            parse_binding(set, state, trim(line), trim(eq + 1));
        else if (wheel)
            parse_wheel_line(set, wheel, trim(line), trim(eq + 1));
    }
}

static void load_layout_text(char *text, const char *origin)
{
    static struct layout_set set;
    parse_layout(text, &set);
    if (set.nstates == 0) {
        logmsg("layout: nothing usable in %s", origin);
        return;
    }
    memcpy(states, set.states, sizeof states);
    memcpy(wheels, set.wheels, sizeof wheels);
    nstates = set.nstates;
    nwheels = set.nwheels;
    for (int i = 0; i < nstates; i++)
        for (int in = 0; in < IN_COUNT; in++)
            states[i].bind[in].tap = states[i].bind[in].tap ? &states[i].taps[in] : NULL;
    depth = 0;
    wheel_open = -1;
    logmsg("layout: %d states, %d wheels from %s", nstates, nwheels, origin);
}

static void load_layout(const char *path)
{
    FILE *f = fopen(path, "r");
    if (!f) {
        logmsg("layout: cannot open %s", path);
        return;
    }
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    rewind(f);
    char *text = size > 0 ? malloc((size_t)size + 1) : NULL;
    if (text && fread(text, 1, (size_t)size, f) == (size_t)size) {
        text[size] = '\0';
        load_layout_text(text, path);
    }
    free(text);
    fclose(f);
}

/* Re-reads a file when its mtime changes, at most once per poll interval. */
static void poll_file(const char *path, struct timespec *last_mtime, void (*load)(const char *))
{
    struct stat st;
    if (!path || stat(path, &st) != 0)
        return;
    if (st.st_mtim.tv_sec != last_mtime->tv_sec || st.st_mtim.tv_nsec != last_mtime->tv_nsec) {
        *last_mtime = st.st_mtim;
        load(path);
    }
}

static void poll_files(double t)
{
    static double last_poll;
    static struct timespec config_mtime, layout_mtime;
    if (t - last_poll < FILE_POLL_S)
        return;
    last_poll = t;
    poll_file(getenv(CONFIG_ENV), &config_mtime, load_config);
    poll_file(getenv(LAYOUT_ENV), &layout_mtime, load_layout);
}

/* ---- game state ---- */

static float capp_float(size_t offset)
{
    float v;
    memcpy(&v, (char *)app + offset, sizeof v);
    return v;
}

static int capp_int(size_t offset)
{
    int v;
    memcpy(&v, (char *)app + offset, sizeof v);
    return v;
}

static void *deref(const void *base, size_t off)
{
    void *p;
    memcpy(&p, (const char *)base + off, sizeof p);
    return p;
}

static int read_int(const void *base, size_t off)
{
    int v;
    memcpy(&v, (const char *)base + off, sizeof v);
    return v;
}

static Point read_point(const void *base, size_t off)
{
    Point p;
    memcpy(&p, (const char *)base + off, sizeof p);
    return p;
}

static size_t vector_count(const void *vec, size_t elem)
{
    char *b = deref(vec, VECTOR_BEGIN), *e = deref(vec, VECTOR_END);
    return b && e >= b ? (size_t)(e - b) / elem : 0;
}

static void *vector_at(const void *vec, size_t elem, size_t i)
{
    return deref(deref(vec, VECTOR_BEGIN), i * elem);
}

/* The Ship object of a CompleteShip, or NULL. */
static void *ship_of(const void *complete)
{
    void *manager = complete ? deref(complete, COMPLETE_SHIP_MANAGER) : NULL;
    return manager ? (char *)manager + SHIPMANAGER_SHIP : NULL;
}

static void *gui_ptr(void)
{
    return app ? deref(app, CAPP_GUI) : NULL;
}

static void *player_complete(void)
{
    void *gui = gui_ptr();
    return gui ? deref(gui, GUI_SHIP_COMPLETE) : NULL;
}

static void *enemy_complete(void)
{
    void *gui = gui_ptr();
    return gui ? deref((const char *)gui + GUI_COMBAT_CONTROL, COMBAT_CURRENT_TARGET) : NULL;
}

static bool game_paused(void)
{
    void *gui = gui_ptr();
    return gui && *((const unsigned char *)gui + GUI_PAUSED);
}

static bool window_open(const void *window)
{
    return window && *((const unsigned char *)window + FOCUS_OPEN);
}

/* The open tabbed screen (ship or store), or NULL. */
static void *open_tabbed(void)
{
    char *gui = gui_ptr();
    if (!gui)
        return NULL;
    if (window_open(gui + GUI_SHIP_SCREENS))
        return gui + GUI_SHIP_SCREENS;
    if (window_open(gui + GUI_STORE_SCREENS))
        return gui + GUI_STORE_SCREENS;
    return NULL;
}

static bool crew_selected(void)
{
    const char *gui = gui_ptr();
    return gui && vector_count(gui + GUI_CREW_CONTROL + CREW_SELECTED, sizeof(void *)) > 0;
}

/* The weapon or drone object in slot i of an armament control, or NULL for an empty slot. */
static void *armament_item(const void *control, int i)
{
    const void *boxes = (const char *)control + ARMAMENT_BOXES;
    if ((size_t)i >= vector_count(boxes, sizeof(void *)))
        return NULL;
    void *box = vector_at(boxes, sizeof(void *), (size_t)i);
    return box ? deref(box, ARMAMENTBOX_ITEM) : NULL;
}

static const char *weapon_control(void)
{
    const char *gui = gui_ptr();
    return gui ? gui + GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL : NULL;
}

static const char *drone_control(void)
{
    const char *gui = gui_ptr();
    return gui ? gui + GUI_COMBAT_CONTROL + COMBAT_DRONE_CONTROL : NULL;
}

/* Bounding box centre of a ship's rooms on screen. */
static bool ship_center(const void *ship, Point origin, float *cx, float *cy)
{
    const void *rooms = (const char *)ship + SHIP_ROOMS;
    size_t n = vector_count(rooms, sizeof(void *));
    int x0 = GAME_W, y0 = GAME_H, x1 = 0, y1 = 0;
    for (size_t i = 0; i < n; i++) {
        const void *room = vector_at(rooms, sizeof(void *), i);
        if (!room)
            continue;
        Rect r;
        memcpy(&r, (const char *)room + ROOM_RECT, sizeof r);
        x0 = r.x < x0 ? r.x : x0;
        y0 = r.y < y0 ? r.y : y0;
        x1 = r.x + r.w > x1 ? r.x + r.w : x1;
        y1 = r.y + r.h > y1 ? r.y + r.h : y1;
    }
    if (n == 0 || x1 <= x0)
        return false;
    *cx = origin.x + (x0 + x1) / 2.0f;
    *cy = origin.y + (y0 + y1) / 2.0f;
    return true;
}

static void add_door_list(const void *doors, Point origin)
{
    size_t n = vector_count(doors, sizeof(void *));
    for (size_t i = 0; i < n && ntargets < MAX_TARGETS; i++) {
        const void *door = vector_at(doors, sizeof(void *), i);
        if (!door)
            continue;
        struct target *t = &targets[ntargets++];
        int x = origin.x + read_int(door, DOOR_X), y = origin.y + read_int(door, DOOR_Y);
        t->rect = (Rect){x - DOOR_HALF, y - DOOR_HALF, 2 * DOOR_HALF, 2 * DOOR_HALF};
        t->id = (int)i;
        t->kind = T_DOOR;
    }
}

/* Inner doors and the airlocks on the hull. */
static void add_doors(const void *ship, Point origin)
{
    add_door_list((const char *)ship + SHIP_DOORS, origin);
    add_door_list((const char *)ship + SHIP_AIRLOCKS, origin);
}

static void add_rooms(const void *ship, Point origin, bool enemy)
{
    const void *rooms = (const char *)ship + SHIP_ROOMS;
    size_t n = vector_count(rooms, sizeof(void *));
    for (size_t i = 0; i < n && ntargets < MAX_TARGETS; i++) {
        const void *room = vector_at(rooms, sizeof(void *), i);
        if (!room)
            continue;
        struct target *t = &targets[ntargets++];
        memcpy(&t->rect, (const char *)room + ROOM_RECT, sizeof t->rect);
        t->rect.x += origin.x;
        t->rect.y += origin.y;
        t->id = read_int(room, ROOM_ID);
        t->kind = enemy ? T_ENEMY_ROOM : T_ROOM;
    }
}

static float read_float(const void *base, size_t off)
{
    float v;
    memcpy(&v, (const char *)base + off, sizeof v);
    return v;
}

/* Beacons, or the sector boxes while a new sector is being chosen; returns false when the
 * map is closed. */
static bool add_map_targets(const void *gui)
{
    const char *map = deref(gui, GUI_STAR_MAP);
    if (!map || !*(const unsigned char *)(map + FOCUS_OPEN))
        return false;
    if (*(const unsigned char *)(map + STARMAP_CHOOSING_SECTOR)) {
        Point offset = read_point(map, STARMAP_SECTOR_OFFSET);
        const void *boxes = map + STARMAP_SECTOR_HITBOXES;
        size_t n = vector_count(boxes, sizeof(Rect));
        for (size_t i = 0; i < n && ntargets < MAX_TARGETS; i++) {
            struct target *t = &targets[ntargets++];
            memcpy(&t->rect, (const char *)deref(boxes, VECTOR_BEGIN) + i * sizeof(Rect), sizeof(Rect));
            t->rect.x += offset.x;
            t->rect.y += offset.y;
            t->id = (int)i;
            t->kind = T_SECTOR;
        }
        return true;
    }
    Point origin = read_point(map, STARMAP_POSITION);
    float tx = read_float(map, STARMAP_TRANSLATION), ty = read_float(map, STARMAP_TRANSLATION + sizeof(float));
    const void *locations = map + STARMAP_LOCATIONS;
    size_t n = vector_count(locations, sizeof(void *));
    for (size_t i = 0; i < n && ntargets < MAX_TARGETS; i++) {
        const void *loc = vector_at(locations, sizeof(void *), i);
        if (!loc)
            continue;
        struct target *t = &targets[ntargets++];
        int cx = origin.x + (int)lroundf(tx + read_float(loc, LOCATION_LOC));
        int cy = origin.y + (int)lroundf(ty + read_float(loc, LOCATION_LOC + sizeof(float)));
        t->rect = (Rect){cx - BEACON_HALF, cy - BEACON_HALF, 2 * BEACON_HALF, 2 * BEACON_HALF};
        t->id = (int)i;
        t->kind = T_BEACON;
    }
    return true;
}

static bool aiming(void)
{
    const void *mouse = game.mouse_position;
    return read_int(mouse, MOUSE_AIMING) || read_int(mouse, MOUSE_TELEPORTING)
           || read_int(mouse, MOUSE_MIND) || read_int(mouse, MOUSE_HACKING);
}

static Point enemy_origin(const void *gui)
{
    const char *combat = (const char *)gui + GUI_COMBAT_CONTROL;
    Point origin = read_point(combat, COMBAT_POSITION);
    Point target_pos = read_point(combat, COMBAT_TARGET_POSITION);
    origin.x += target_pos.x;
    origin.y += target_pos.y;
    return origin;
}

/* The open event box's choices; returns false when it is closed. */
static bool add_choice_targets(const void *gui)
{
    const char *box = (const char *)gui + GUI_CHOICE_BOX;
    if (!*(const unsigned char *)(box + FOCUS_OPEN))
        return false;
    const void *rects = box + CHOICE_RECTS;
    size_t n = vector_count(rects, sizeof(Rect));
    for (size_t i = 0; i < n && ntargets < MAX_TARGETS; i++) {
        struct target *t = &targets[ntargets++];
        memcpy(&t->rect, (const char *)deref(rects, VECTOR_BEGIN) + i * sizeof(Rect), sizeof(Rect));
        t->id = (int)i;
        t->kind = T_CHOICE;
    }
    return true;
}

static bool inside(const void *p, const void *base, size_t size)
{
    return base && (const char *)p >= (const char *)base && (const char *)p < (const char *)base + size;
}

/* The game menu or the options screen when one is open: only its buttons count. */
static const char *modal_window(const char *gui, size_t *size)
{
    if (window_open(gui + GUI_OPTIONS_BOX)) {
        *size = OPTIONS_BOX_SIZE;
        return gui + GUI_OPTIONS_BOX;
    }
    if (window_open(gui + GUI_MENU_BOX)) {
        *size = MENU_BOX_SIZE;
        return gui + GUI_MENU_BOX;
    }
    return NULL;
}

/* With the map open only its own buttons count, a beacon click jumps by itself; with
 * another screen or the menu open the HUD's top and bottom rows drop out (the menu's
 * buttons live on the heap, so they cannot be told apart by address). */
static void add_button_targets(const void *gui, bool screen_open, bool map_open)
{
    const void *map = map_open ? deref(gui, GUI_STAR_MAP) : NULL;
    for (int i = 0; i < nbuttons && ntargets < MAX_TARGETS; i++) {
        if (map_open && (!inside(button_objects[i], map, STARMAP_SIZE)
                         || button_objects[i] == (const char *)map + STARMAP_CLOSE_BUTTON))
            continue;
        if (screen_open && (buttons[i].y < HUD_TOP_HEIGHT || buttons[i].y >= HUD_BOTTOM_TOP))
            continue;
        struct target *t = &targets[ntargets++];
        t->rect = buttons[i];
        t->id = i;
        t->kind = T_BUTTON;
    }
}

/* Equipment and crew slots of an open ship screen, which are not buttons. */
static void add_slot_targets(const void *gui)
{
    const struct {
        size_t window, boxes;
    } screens[2] = {{GUI_EQUIP_SCREEN, EQUIP_BOXES}, {GUI_CREW_SCREEN, CREWSCREEN_BOXES}};
    for (int k = 0; k < 2; k++) {
        const char *window = (const char *)gui + screens[k].window;
        if (!window_open(window))
            continue;
        const void *boxes = window + screens[k].boxes;
        size_t n = vector_count(boxes, sizeof(void *));
        for (size_t i = 0; i < n && ntargets < MAX_TARGETS; i++) {
            const char *box = vector_at(boxes, sizeof(void *), i);
            if (!box)
                continue;
            Rect r;
            memcpy(&r, box + EQUIPBOX_HITBOX, sizeof r);
            if (r.w <= 0 || r.h <= 0)
                continue;
            struct target *t = &targets[ntargets++];
            t->rect = r;
            t->id = (int)i;
            t->kind = T_SLOT;
        }
    }
}

/* Rebuilds the zones for the current screen: beacons or sector boxes with the map open,
 * event choices with an event open, otherwise rooms (the enemy's only while aiming, only
 * doors in precision mode); the buttons the game hit-tests come on top. */
static void build_targets(void)
{
    ntargets = 0;
    if (nbuttons_seen > 0) {
        memcpy(buttons, buttons_seen, (size_t)nbuttons_seen * sizeof *buttons);
        memcpy(button_objects, button_objects_seen, (size_t)nbuttons_seen * sizeof *button_objects);
        nbuttons = nbuttons_seen;
        nbuttons_seen = 0;
    }
    void *gui = gui_ptr();
    if (!gui)
        return;
    void *player = ship_of(player_complete());
    size_t modal_size = 0;
    bool screen_open = open_tabbed() != NULL || modal_window(gui, &modal_size) != NULL;
    bool map_open = window_open(deref(gui, GUI_STAR_MAP));
    if (player && !screen_open && !add_map_targets(gui) && !add_choice_targets(gui)) {
        void *enemy = ship_of(enemy_complete());
        Point origin = read_point(gui, GUI_SHIP_POSITION);
        if (aiming()) {
            if (enemy)
                add_rooms(enemy, enemy_origin(gui), true);
            add_rooms(player, origin, false);
        } else if (cfg.precision && !crew_selected()) {
            add_doors(player, origin);
        } else {
            add_rooms(player, origin, false);
            if (enemy)
                add_rooms(enemy, enemy_origin(gui), true);
        }
    }
    if (screen_open)
        add_slot_targets(gui);
    if (!cfg.precision || aiming() || crew_selected())
        add_button_targets(gui, screen_open, map_open);
}

static bool rect_contains(const Rect *r, int x, int y)
{
    return x >= r->x && x < r->x + r->w && y >= r->y && y < r->y + r->h;
}

static int target_under_cursor(void)
{
    for (int i = 0; i < ntargets; i++)
        if (rect_contains(&targets[i].rect, cursor.last_x, cursor.last_y))
            return i;
    return -1;
}

/* ---- cursor ---- */

/* Maps a logical position to the window pixels the mouse handlers take. */
static void to_window(float gx, float gy, int *wx, int *wy)
{
    float mx = capp_float(CAPP_MOUSE_MODIFIER_X), my = capp_float(CAPP_MOUSE_MODIFIER_Y);
    if (mx <= 0)
        mx = 1;
    if (my <= 0)
        my = 1;
    *wx = (int)lroundf((gx + (float)capp_int(CAPP_MODIFIER_X)) / mx) + capp_int(CAPP_X_BAR);
    *wy = (int)lroundf((gy + (float)capp_int(CAPP_MODIFIER_Y)) / my) + capp_int(CAPP_Y_BAR);
}

/* Adopts the game's cursor when something else moved it. */
static void sync_cursor(void)
{
    Point p = *game.mouse_position;
    if (cursor.synced && p.x == cursor.last_set.x && p.y == cursor.last_set.y)
        return;
    cursor.x = (float)p.x;
    cursor.y = (float)p.y;
    cursor.last_x = p.x;
    cursor.last_y = p.y;
    to_window(cursor.x, cursor.y, &cursor.last_wx, &cursor.last_wy);
    cursor.last_set = p;
    cursor.synced = true;
}

/* Reports the cursor to the game when its integer position changed. */
static void send_cursor(void)
{
    int x = (int)cursor.x, y = (int)cursor.y;
    if (x == cursor.last_x && y == cursor.last_y)
        return;
    int wx, wy;
    to_window(cursor.x, cursor.y, &wx, &wy);
    game.mouse_move(app, wx, wy, wx - cursor.last_wx, wy - cursor.last_wy, cursor.l_held, false,
                    cursor.r_held);
    cursor.last_x = x;
    cursor.last_y = y;
    cursor.last_wx = wx;
    cursor.last_wy = wy;
    cursor.last_set = *game.mouse_position;
}

/* Moves the cursor by a stick deflection over dt seconds. */
static void move_cursor(float rx, float ry, float dt)
{
    float mag = hypotf(rx, ry);
    if (mag <= cfg.cursor_deadzone)
        return;
    float k = (mag - cfg.cursor_deadzone) / (1.0f - cfg.cursor_deadzone);
    k = powf(clampf(k, 0.0f, 1.0f), cfg.cursor_curve);
    cursor.x += rx / mag * k * cfg.cursor_speed * dt;
    cursor.y += ry / mag * k * cfg.cursor_speed * dt;
    cursor.x = clampf(cursor.x, 0.0f, (float)(GAME_W - 1));
    cursor.y = clampf(cursor.y, 0.0f, (float)(GAME_H - 1));
    send_cursor();
}

static void buzz(void);
static void draw_help(void);
static void upper_ascii(char *text);
static void run_queued_clicks(void);
static void builtin(enum builtin id, bool down, double t);
static void badge_text(char *out, size_t len);
static int text_width(int font, const char *text);

/* Moves the cursor to the target nearest along a direction: inside the narrow cone first,
 * then the wide one, scored by distance grown with the angle off the axis. */
static void focus_move(float dx, float dy)
{
    if (ntargets == 0)
        return;
    float len = hypotf(dx, dy);
    if (len <= 0)
        return;
    dx /= len;
    dy /= len;
    int from = target_under_cursor();
    float cx = cursor.x, cy = cursor.y;
    int best = -1;
    float best_score = 0;
    const float cones[2] = {CONE_NARROW_DEG, CONE_WIDE_DEG};
    for (int pass = 0; pass < 2 && best < 0; pass++) {
        float cos_cone = cosf(cones[pass] * (float)M_PI / 180.0f);
        for (int i = 0; i < ntargets; i++) {
            if (i == from)
                continue;
            float tx = targets[i].rect.x + targets[i].rect.w / 2.0f;
            float ty = targets[i].rect.y + targets[i].rect.h / 2.0f;
            float vx = tx - cx, vy = ty - cy;
            float dist = hypotf(vx, vy);
            if (dist <= 0)
                continue;
            float cos_a = (vx * dx + vy * dy) / dist;
            if (cos_a < cos_cone)
                continue;
            float score = dist * (1.0f + FOCUS_SIDE_WEIGHT * (1.0f - cos_a));
            if (best < 0 || score < best_score) {
                best = i;
                best_score = score;
            }
        }
    }
    if (best < 0)
        return;
    cursor.x = targets[best].rect.x + targets[best].rect.w / 2.0f;
    cursor.y = targets[best].rect.y + targets[best].rect.h / 2.0f;
    send_cursor();
    buzz();
}

/* Screens that open on their first item. */
enum first_screen { FS_NONE, FS_MAIN_MENU, FS_GAME_MENU, FS_CHOICE };
/* Frames a newly opened screen gets to show its items before the focus gives up; a
 * dialog's choices wait for the jump animation. */
#define FIRST_FOCUS_FRAMES 300

/* The menu or dialog on screen, if it is one that opens on its first item. */
static enum first_screen first_screen(void)
{
    const char *gui = gui_ptr();
    if (gui && *((const unsigned char *)gui + GUI_CHOICE_BOX + FOCUS_OPEN))
        return FS_CHOICE;
    if (gui && window_open(gui + GUI_MENU_BOX))
        return FS_GAME_MENU;
    if (*((const unsigned char *)app + CAPP_MAIN_MENU_OPEN))
        return FS_MAIN_MENU;
    return FS_NONE;
}

/* The first item of a screen among the zones: a dialog's first choice, else the topmost
 * button, leftmost on a tie; -1 while there is none. */
static int first_item(enum first_screen screen)
{
    int best = -1;
    for (int i = 0; i < ntargets; i++) {
        const struct target *t = &targets[i];
        if (screen == FS_CHOICE) {
            if (t->kind == T_CHOICE && t->id == 0)
                return i;
        } else if (t->kind == T_BUTTON && (best < 0 || t->rect.y < targets[best].rect.y
                       || (t->rect.y == targets[best].rect.y && t->rect.x < targets[best].rect.x))) {
            best = i;
        }
    }
    return best;
}

/* Puts the cursor on the first item of a menu or dialog as it opens, unless the cursor
 * moves first. A dialog that follows another counts as new when its text changes or its
 * open timer restarts. */
static void focus_first_on_open(void)
{
    static enum first_screen prev;
    static const void *prev_text;
    static float prev_open_time;
    static int waiting;
    static float open_x, open_y;
    enum first_screen screen = first_screen();
    bool opened = screen != prev;
    if (screen == FS_CHOICE) {
        const char *box = (const char *)gui_ptr() + GUI_CHOICE_BOX;
        const void *text = deref(box, CHOICE_TEXT);
        float open_time;
        memcpy(&open_time, box + CHOICE_OPEN_TIME, sizeof open_time);
        opened |= text != prev_text || open_time < prev_open_time;
        prev_text = text;
        prev_open_time = open_time;
    }
    prev = screen;
    if (opened) {
        waiting = screen != FS_NONE ? FIRST_FOCUS_FRAMES : 0;
        open_x = cursor.x;
        open_y = cursor.y;
    }
    if (waiting == 0 || cursor.x != open_x || cursor.y != open_y) {
        waiting = 0;
        return;
    }
    waiting--;
    int first = first_item(screen);
    if (first < 0)
        return;
    cursor.x = targets[first].rect.x + targets[first].rect.w / 2.0f;
    cursor.y = targets[first].rect.y + targets[first].rect.h / 2.0f;
    send_cursor();
    waiting = 0;
}

/* ---- layout engine ---- */

static const struct state *state_at(int level)
{
    return &states[level == 0 ? 0 : stack[level].state];
}

/* The binding an input resolves to in the active state chain, or NULL. */
static const struct binding *lookup(enum input in)
{
    for (int level = depth; level >= 0; level--) {
        const struct state *s = state_at(level);
        if (s->bound[in])
            return &s->bind[in];
        if (level > 0 && !s->parent)
            break;
    }
    return NULL;
}

/* What a pointer source does in the active state chain: keys once any state binds its
 * directions, else the mode the nearest state sets. */
static enum slot_mode slot_mode(enum slot slot)
{
    for (int level = depth; level >= 0; level--) {
        const struct state *s = state_at(level);
        for (int d = 0; d < 4; d++)
            if (s->bound[slot_first[slot] + d])
                return SLOT_KEYS;
        if (s->slot[slot] != SLOT_KEYS)
            return s->slot[slot];
        if (level > 0 && !s->parent)
            break;
    }
    return SLOT_KEYS;
}

static void set_shift(bool on);
static void click(enum act_kind kind, bool down);

static void draw_text_left(int font, float x, float y, const char *text)
{
    void *str = NULL;
    char alloc = 0;
    game.string_ctor(&str, text, &alloc);
    game.print_left(font, x, y, &str);
    game.string_dtor(&str);
}

/* Prints text centred at x,y with the game's font code. */
static void draw_text_center(int font, float x, float y, const char *text)
{
    void *str = NULL;
    char alloc = 0;
    game.string_ctor(&str, text, &alloc);
    game.print_center(font, x, y, &str);
    game.string_dtor(&str);
}

/* The point the plates circle, kept inside the screen. */
static void wheel_origin(float *ox, float *oy)
{
    float reach = WHEEL_RADIUS + WHEEL_TILE / 2 + WHEEL_MARGIN;
    *ox = clampf(cursor.x, reach, GAME_W - reach);
    *oy = clampf(cursor.y, reach, GAME_H - reach);
}

/* Plate centre for a wheel slot. */
static void plate_center(int slot, float *cx, float *cy)
{
    float ox, oy;
    wheel_origin(&ox, &oy);
    float angle = (float)slot * (float)M_PI / 4.0f;
    *cx = ox + sinf(angle) * WHEEL_RADIUS;
    *cy = oy - cosf(angle) * WHEEL_RADIUS;
}

/* A panel in the game's style: a dark bevel around a framed fill. */
static void draw_panel(float x, float y, float w, float h, GL_Color fill, GL_Color edge)
{
    game.draw_rect(x - WHEEL_BEVEL, y - WHEEL_BEVEL, w + 2 * WHEEL_BEVEL, h + 2 * WHEEL_BEVEL, plate_bevel_color);
    game.draw_rect(x, y, w, h, fill);
    game.draw_rect_outline((int)x, (int)y, (int)w, (int)h, edge, OVERLAY_LINE);
}

/* A filled disc out of scanlines: the game draws no curves. */
static void draw_disc(float cx, float cy, float r, GL_Color color)
{
    for (float y = -r; y < r; y += DISC_SCANLINE) {
        float t = (y + DISC_SCANLINE / 2) / r;
        float half = r * sqrtf(fmaxf(0.0f, 1.0f - t * t));
        game.draw_rect(cx - half, cy + y, 2 * half, DISC_SCANLINE, color);
    }
}

/* Looks up a game image by its path under img/; returns its texture, or NULL when the
 * game has none. Results are cached, misses included. */
static void *game_image(const char *path)
{
    static struct {
        char path[ICON_PATH_LEN];
        void *texture;
    } cache[IMAGE_CACHE_SIZE];
    static int ncache;
    for (int i = 0; i < ncache; i++)
        if (strcmp(cache[i].path, path) == 0)
            return cache[i].texture;
    void *str = NULL;
    char alloc = 0;
    game.string_ctor(&str, path, &alloc);
    void *texture = game.image_id(game.resources, &str);
    game.string_dtor(&str);
    if (!texture)
        logmsg("image missing: %s", path);
    if (ncache < IMAGE_CACHE_SIZE) {
        strncpy(cache[ncache].path, path, ICON_PATH_LEN - 1);
        cache[ncache++].texture = texture;
    }
    return texture;
}

/* Draws a plate's icon or portrait centred at cx,cy. */
static void draw_plate_icon(const struct plate *pl, float cx, float cy, GL_Color tint)
{
    if (pl->portrait) {
        game.push_matrix();
        game.translate(cx, cy, 0.0f);
        game.scale(CREW_ICON_SCALE, CREW_ICON_SCALE, 1.0f);
        game.crew_icon(pl->portrait, true);
        game.pop_matrix();
        return;
    }
    float tw = (float)read_int(pl->icon, TEXTURE_W), th = (float)read_int(pl->icon, TEXTURE_H);
    float sw = pl->icon_partial ? pl->icon_w : tw, sh = pl->icon_partial ? pl->icon_h : th;
    if (tw <= 0 || th <= 0 || sw <= 0 || sh <= 0)
        return;
    float k = TILE_ICON / fmaxf(sw, sh);
    float w = sw * k, h = sh * k, x = cx - w / 2 - pl->icon_shift * k, y = cy - h / 2;
    if (pl->icon_partial)
        /* The region goes in as fractions of the texture. */
        game.blit_partial(pl->icon, x, y, w, h, pl->icon_x / tw, (pl->icon_x + pl->icon_w) / tw,
                          pl->icon_y / th, (pl->icon_y + pl->icon_h) / th, 1.0f, tint, false);
    else
        game.blit(pl->icon, x, y, w, h, 0.0f, tint, false);
}

/* Draws a row of power pips centred at cx with its top at y. */
static void draw_pips(int n, int lit, float cx, float y, bool available)
{
    float pw = fminf(PIP_MAX_W, (WHEEL_TILE - 2 * TILE_PAD - (float)(n - 1) * PIP_GAP) / (float)n);
    float x = cx - ((float)n * pw + (float)(n - 1) * PIP_GAP) / 2;
    for (int i = 0; i < n; i++, x += pw + PIP_GAP) {
        if (i < lit)
            game.draw_rect(x, y, pw, PIP_H, available ? pip_on_color : text_disabled_color);
        else
            game.draw_rect_outline((int)x, (int)y, (int)pw, (int)PIP_H, available ? plate_edge_color
                                   : plate_disabled_edge_color, OVERLAY_LINE);
    }
}

static void draw_plate(const struct plate *pl, float cx, float cy, bool selected)
{
    float x = cx - WHEEL_TILE / 2, y = cy - WHEEL_TILE / 2;
    if (pl->empty) {
        game.draw_rect_outline((int)x, (int)y, (int)WHEEL_TILE, (int)WHEEL_TILE, plate_empty_edge_color, OVERLAY_LINE);
        return;
    }
    if (!pl->available)
        draw_panel(x, y, WHEEL_TILE, WHEEL_TILE, plate_disabled_color, plate_disabled_edge_color);
    else if (selected)
        draw_panel(x, y, WHEEL_TILE, WHEEL_TILE, plate_selected_color, plate_selected_edge_color);
    else
        draw_panel(x, y, WHEEL_TILE, WHEEL_TILE, pl->powered ? plate_powered_color : plate_idle_color,
                   plate_edge_color);
    /* With pips the icon box sits at the top, without it is centred. */
    float icon_cy = pl->pips > 0 ? y + TILE_PAD + TILE_ICON / 2 : cy;
    GL_Color tint = pl->available ? text_color : text_disabled_color;
    if (pl->icon_well)
        game.draw_rect(cx - TILE_ICON / 2, icon_cy - TILE_ICON / 2, TILE_ICON, TILE_ICON,
                       pl->powered ? well_powered_color : well_color);
    if (pl->icon || pl->portrait)
        draw_plate_icon(pl, cx, icon_cy, tint);
    else if (pl->available) {
        game.set_color(tint);
        draw_text_center(plate_font, cx, icon_cy - WHEEL_TEXT_H / 2, pl->label);
        game.set_color(text_color);
    }
    if (pl->corner_icon) {
        float ix = x + WHEEL_TILE - CORNER_ICON - CORNER_INSET, iy = y + WHEEL_TILE - CORNER_ICON - CORNER_INSET;
        game.draw_rect(ix, iy, CORNER_ICON, CORNER_ICON, corner_color);
        game.blit(pl->corner_icon, ix, iy, CORNER_ICON, CORNER_ICON, 0.0f, text_color, false);
    }
    if (pl->pips > 0)
        draw_pips(pl->pips, pl->pips_on, cx, y + WHEEL_TILE - TILE_PAD - PIP_H, pl->available);
}

static void draw_wheel(void)
{
    const struct wheel *w = &wheels[wheel_open];
    float ox, oy;
    wheel_origin(&ox, &oy);
    draw_disc(ox, oy, WHEEL_RADIUS + WHEEL_TILE / 2 + WHEEL_DISC_PAD, disc_color);
    /* The middle: the wheel's title alone, or above the pick's name and detail. */
    char title[NAME_LEN];
    snprintf(title, sizeof title, "%s", w->name);
    upper_ascii(title);
    const struct plate *sel = wheel_sel >= 0 ? &w->plates[wheel_sel] : NULL;
    int lines = !sel ? 1 : sel->badge[0] ? 3 : 2;
    float text_h = (float)(lines - 1) * WHEEL_LINE + WHEEL_TEXT_H;
    float top = oy - text_h / 2;
    /* A shade under the text alone keeps it readable over the ship. */
    float text_w = (float)text_width(plate_font, title);
    if (sel)
        text_w = fmaxf(fmaxf(text_w, (float)text_width(plate_font, sel->label)), (float)text_width(plate_font, sel->badge));
    game.draw_rect(ox - text_w / 2 - WHEEL_TEXT_PAD, top - WHEEL_TEXT_PAD, text_w + 2 * WHEEL_TEXT_PAD,
                   text_h + 2 * WHEEL_TEXT_PAD, middle_color);
    game.set_color(plate_edge_color);
    draw_text_center(plate_font, ox, top, title);
    if (sel) {
        game.set_color(text_color);
        draw_text_center(plate_font, ox, top + WHEEL_LINE, sel->label);
        if (sel->badge[0]) {
            game.set_color(plate_selected_edge_color);
            draw_text_center(plate_font, ox, top + 2 * WHEEL_LINE, sel->badge);
        }
    }
    game.set_color(text_color);
    for (int i = 0; i < WHEEL_SLOTS; i++) {
        /* A position the layout leaves free looks like an empty slot. */
        static const struct plate free_slot = {.empty = true};
        float cx, cy;
        plate_center(i, &cx, &cy);
        draw_plate(w->plates[i].bound ? &w->plates[i] : &free_slot, cx, cy, i == wheel_sel);
    }
}

/* Points the wheel from the stronger stick; the selection sticks when both are centred. */
static void wheel_point(float lx, float ly, float rx, float ry)
{
    float x = lx, y = ly;
    if (hypotf(rx, ry) > hypotf(lx, ly)) {
        x = rx;
        y = ry;
    }
    if (hypotf(x, y) < WHEEL_ON)
        return;
    float angle = atan2f(x, -y);
    if (angle < 0)
        angle += 2.0f * (float)M_PI;
    int slot = (int)lroundf(angle / ((float)M_PI / 4.0f)) % WHEEL_SLOTS;
    const struct plate *pl = &wheels[wheel_open].plates[slot];
    if (pl->bound && pl->available && slot != wheel_sel) {
        wheel_sel = slot;
        buzz();
    }
}

/* Steps the selection to the next bound plate clockwise or counter-clockwise. */
static void wheel_step(int dir)
{
    const struct wheel *w = &wheels[wheel_open];
    int slot = wheel_sel < 0 ? (dir > 0 ? -1 : 0) : wheel_sel;
    for (int n = 0; n < WHEEL_SLOTS; n++) {
        slot = (slot + dir + WHEEL_SLOTS) % WHEEL_SLOTS;
        if (w->plates[slot].bound && w->plates[slot].available) {
            wheel_sel = slot;
            buzz();
            return;
        }
    }
}

/* The crew member a plate key selects, by F-key index into the ship's crew list. */
static void *crew_at(int index)
{
    void *complete = player_complete();
    void *manager = complete ? deref(complete, COMPLETE_SHIP_MANAGER) : NULL;
    if (!manager)
        return NULL;
    const void *crew = (const char *)manager + SHIPMANAGER_CREW;
    if ((size_t)index >= vector_count(crew, sizeof(void *)))
        return NULL;
    return vector_at(crew, sizeof(void *), (size_t)index);
}

/* Whether a crew member is in the current selection. */
static bool crew_is_selected(const void *member)
{
    const char *gui = gui_ptr();
    if (!gui)
        return false;
    const void *selected = gui + GUI_CREW_CONTROL + CREW_SELECTED;
    size_t n = vector_count(selected, sizeof(void *));
    for (size_t i = 0; i < n; i++)
        if (vector_at(selected, sizeof(void *), i) == member)
            return true;
    return false;
}

static const char *game_string(const void *str)
{
    const char *chars = deref(str, 0);
    return chars ? chars : "";
}

/* Copies a TextString's display text: literal ones as they are, keys through the
 * game's text library in its current language. */
static void display_text(const void *text, char *out, size_t len)
{
    if (*((const unsigned char *)text + TEXTSTRING_LITERAL)) {
        strncpy(out, game_string(text), len - 1);
        out[len - 1] = '\0';
        return;
    }
    void *result = NULL;
    game.get_text(&result, game.text_library, text, (char *)game.text_library + TEXTLIB_LANGUAGE);
    strncpy(out, game_string(&result), len - 1);
    out[len - 1] = '\0';
    game.string_dtor(&result);
}

/* Hotkey of a system type, or 0 when it has none. */
static int system_key(int type)
{
    switch (type) {
    case SYS_SHIELDS: return 'a';
    case SYS_ENGINES: return 's';
    case SYS_OXYGEN: return 'f';
    case SYS_MEDBAY: case SYS_CLONEBAY: return 'd';
    case SYS_TELEPORTER: return 'g';
    case SYS_CLOAKING: return 'h';
    case SYS_MIND: return 'k';
    case SYS_HACKING: return 'l';
    case SYS_ARTILLERY: return 'y';
    default: return 0;
    }
}

static const char *system_name(int type)
{
    switch (type) {
    case SYS_SHIELDS: return "Shields";
    case SYS_ENGINES: return "Engines";
    case SYS_OXYGEN: return "Oxygen";
    case SYS_MEDBAY: return "Medbay";
    case SYS_CLONEBAY: return "Clone bay";
    case SYS_TELEPORTER: return "Teleporter";
    case SYS_CLOAKING: return "Cloaking";
    case SYS_MIND: return "Mind control";
    case SYS_HACKING: return "Hacking";
    case SYS_ARTILLERY: return "Artillery";
    case SYS_WEAPONS: return "Weapons";
    case SYS_DRONES: return "Drones";
    case SYS_PILOT: return "Piloting";
    case SYS_SENSORS: return "Sensors";
    case SYS_DOORS: return "Doors";
    case SYS_BATTERY: return "Battery";
    default: return "?";
    }
}

/* The system type an ability key needs, or -1. */
static int ability_system(int key)
{
    switch (key) {
    case 'c': return SYS_CLOAKING;
    case 'b': return SYS_BATTERY;
    case 'n': return SYS_HACKING;
    case 'm': return SYS_MIND;
    case 't': case 'r': return SYS_TELEPORTER;
    case 'y': return SYS_ARTILLERY;
    default: return -1;
    }
}

#define CREWBP_RACE 8
#define CRYSTAL_RACE "crystal"

/* Lockdown needs a living crystal crew member. */
static bool has_crystal_crew(void)
{
    for (int i = 0;; i++) {
        const char *member = crew_at(i);
        if (!member)
            return false;
        if (!*(member + CREW_DEAD) && strcmp(game_string(member + CREW_BLUEPRINT + CREWBP_RACE), CRYSTAL_RACE) == 0)
            return true;
    }
}

static bool has_system(int type)
{
    void *complete = player_complete();
    void *manager = complete ? deref(complete, COMPLETE_SHIP_MANAGER) : NULL;
    return manager && game.system_room(manager, type) >= 0;
}

/* Sets a crew plate's corner icon and badge from the system in the member's room, or
 * the badge alone when the member is aboard the other ship. */
static void crew_location(struct plate *pl, const void *member)
{
    if (read_int(member, CREW_SHIP) != PLAYER_SHIP_ID) {
        strncpy(pl->badge, "Enemy ship", LABEL_LEN - 1);
        return;
    }
    void *complete = player_complete();
    void *manager = complete ? deref(complete, COMPLETE_SHIP_MANAGER) : NULL;
    const char *sys = manager ? game.system_in_room(manager, read_int(member, CREW_ROOM)) : NULL;
    if (!sys)
        return;
    pl->corner_icon = deref(sys, SYSTEM_ICON);
    strncpy(pl->badge, system_name(read_int(sys, SYSTEM_TYPE)), LABEL_LEN - 1);
}

/* Sets a weapon or drone plate's icon from its blueprint's icon name, and its pips. */
static void armament_plate(struct plate *pl, const void *icon_name, const char *prefix, const char *fallback,
                           float shift, int power)
{
    char path[ICON_PATH_LEN];
    snprintf(path, sizeof path, "%s%s%s", prefix, game_string(icon_name), ICON_SUFFIX);
    pl->icon = game_image(path);
    pl->icon_well = true;
    pl->icon_shift = shift;
    if (!pl->icon) {
        pl->icon = game_image(fallback);
        pl->icon_well = false;
        pl->icon_shift = 0;
    }
    pl->icon_partial = false;
    pl->pips = power;
    pl->pips_on = pl->powered ? power : 0;
}

/* Fills the open wheel's plates from the game: crew and systems wholesale, power and
 * action wheels their availability, badges and icons. */
static void refresh_wheel(void)
{
    struct wheel *w = &wheels[wheel_open];
    if (w->kind == WK_CREW || w->kind == WK_SYSTEMS)
        memset(w->plates, 0, sizeof w->plates);
    if (w->kind == WK_CREW) {
        int slot = 0;
        for (int i = 0; slot < WHEEL_SLOTS; i++) {
            const char *member = crew_at(i);
            if (!member)
                break;
            if (*(member + CREW_DEAD))
                continue;
            struct plate *pl = &w->plates[slot++];
            pl->bound = pl->available = true;
            pl->powered = crew_is_selected(member);
            pl->portrait = deref(member, CREW_ANIM);
            crew_location(pl, member);
            pl->bind = (struct binding){ACT_KEY, KEY_F1 + i, false, 0, NULL};
            display_text(member + CREW_BLUEPRINT + CREWBP_NAME, pl->label, LABEL_LEN);
        }
        /* Empty slots keep their places, so positions never shift. */
        for (; slot < WHEEL_SLOTS; slot++) {
            struct plate *pl = &w->plates[slot];
            pl->bound = true;
            pl->available = false;
            pl->empty = true;
            strncpy(pl->label, "Empty", LABEL_LEN - 1);
        }
        return;
    }
    if (w->kind == WK_SYSTEMS) {
        /* Fixed plates in power-bar order; a slot takes the medbay or the clone bay. */
        static const int slot_types[WHEEL_SLOTS][2] = {
            {SYS_SHIELDS, -1}, {SYS_ENGINES, -1}, {SYS_OXYGEN, -1}, {SYS_MEDBAY, SYS_CLONEBAY},
            {SYS_TELEPORTER, -1}, {SYS_CLOAKING, -1}, {SYS_MIND, -1}, {SYS_HACKING, -1},
        };
        const char *gui = gui_ptr();
        const void *boxes = gui ? gui + GUI_SYS_CONTROL + SYSCTRL_BOXES : NULL;
        size_t n = boxes ? vector_count(boxes, sizeof(void *)) : 0;
        for (int slot = 0; slot < WHEEL_SLOTS; slot++) {
            struct plate *pl = &w->plates[slot];
            pl->bound = true;
            pl->available = false;
            pl->empty = true;
            pl->powered = false;
            pl->icon = NULL;
            strncpy(pl->label, system_name(slot_types[slot][0]), LABEL_LEN - 1);
            pl->bind = (struct binding){ACT_KEY, system_key(slot_types[slot][0]), false, 0, NULL};
            for (size_t i = 0; i < n; i++) {
                const void *box = vector_at(boxes, sizeof(void *), i);
                const char *sys = box ? deref(box, SYSBOX_SYSTEM) : NULL;
                int type = sys ? read_int(sys, SYSTEM_TYPE) : -1;
                if (type < 0 || (type != slot_types[slot][0] && type != slot_types[slot][1]))
                    continue;
                pl->available = true;
                pl->empty = false;
                pl->powered = read_int(sys, SYSTEM_POWER) > 0;
                pl->icon = deref(sys, SYSTEM_ICON);
                pl->icon_partial = false;
                pl->pips = read_int(sys, SYSTEM_POWER + 4);
                pl->pips_on = read_int(sys, SYSTEM_POWER);
                strncpy(pl->label, system_name(type), LABEL_LEN - 1);
                snprintf(pl->badge, LABEL_LEN, "%d/%d", pl->pips_on, pl->pips);
                break;
            }
        }
        return;
    }
    for (int i = 0; i < WHEEL_SLOTS; i++) {
        struct plate *pl = &w->plates[i];
        pl->badge[0] = '\0';
        pl->powered = false;
        pl->pips = pl->pips_on = 0;
        pl->empty = false;
        pl->icon_well = false;
        pl->icon_shift = 0;
        pl->icon = pl->icon_path[0] ? game_image(pl->icon_path) : NULL;
        if (!pl->bound || pl->bind.kind != ACT_KEY)
            continue;
        int key = pl->bind.key;
        pl->available = true;
        if (w->kind == WK_POWER && key >= '1' && key < '1' + WEAPON_SLOTS) {
            const char *weapon = weapon_control() ? armament_item(weapon_control(), key - '1') : NULL;
            const char *bp = weapon ? deref(weapon, WEAPON_BLUEPRINT) : NULL;
            pl->available = weapon != NULL;
            pl->empty = !weapon;
            if (bp)
                display_text(bp + BLUEPRINT_SHORT_TITLE, pl->label, LABEL_LEN);
            else
                strncpy(pl->label, "Empty", LABEL_LEN - 1);
            pl->powered = weapon && *(weapon + WEAPON_POWERED);
            if (weapon)
                armament_plate(pl, bp + WEAPONBP_ICON, WEAPON_ICON_PREFIX, WEAPON_ICON_FALLBACK, WEAPON_ICON_SHIFT,
                               read_int(weapon, WEAPON_POWER));
        } else if (w->kind == WK_POWER && key >= '5' && key < '5' + DRONE_SLOTS) {
            const char *drone = drone_control() ? armament_item(drone_control(), key - '5') : NULL;
            const char *bp = drone ? deref(drone, DRONE_BLUEPRINT) : NULL;
            pl->available = drone != NULL;
            pl->empty = !drone;
            if (bp)
                display_text(bp + BLUEPRINT_SHORT_TITLE, pl->label, LABEL_LEN);
            else
                strncpy(pl->label, "Empty", LABEL_LEN - 1);
            pl->powered = drone && *(drone + DRONE_POWERED);
            if (drone)
                armament_plate(pl, bp + DRONEBP_ICON, DRONE_ICON_PREFIX, DRONE_ICON_FALLBACK, 0.0f,
                               read_int(drone, DRONE_POWER));
        } else if (w->kind == WK_POWER && key == 'v') {
            const char *control = weapon_control();
            pl->powered = control && *(control + WEAPONCTRL_AUTOFIRE);
        } else if (w->kind == WK_ACTION && ability_system(key) >= 0) {
            pl->available = has_system(ability_system(key));
        } else if (w->kind == WK_ACTION && key == 'p') {
            pl->available = has_crystal_crew();
        }
    }
}

static void tap_key(int key, bool shift)
{
    if (shift)
        set_shift(true);
    game.key_down(app, key);
    game.key_up(app, key);
    if (shift)
        set_shift(false);
}

static void wheel_close(void)
{
    if (wheel_open < 0)
        return;
    if (wheel_paused && game_paused())
        tap_key(KEY_SPACE, false);
    wheel_paused = false;
    wheel_open = -1;
    wheel_sel = -1;
}

static void wheel_show(int index, enum input by)
{
    if (wheel_open >= 0) {
        if (wheel_sticky && wheel_open == index)
            wheel_close();
        return;
    }
    wheel_open = index;
    wheel_by = by;
    wheel_sticky = true;
    wheel_sel = -1;
    wheel_opened_aiming = aiming();
    refresh_wheel();
    /* Opens on the top plate, or the first one clockwise that can be picked. */
    for (int i = 0; i < WHEEL_SLOTS && wheel_sel < 0; i++)
        if (wheels[index].plates[i].bound && wheels[index].plates[i].available)
            wheel_sel = i;
    if (cfg.autopause_wheel && !game_paused()) {
        tap_key(KEY_SPACE, false);
        wheel_paused = true;
    }
    if (debug)
        logmsg("%s: wheel %s", input_ini_names[by], wheels[index].name);
}

/* A on the selected plate: keys and clicks run as taps; power wheels stay open, the
 * others close. The abilities key is remembered for repeat_ability. */
static void wheel_act(void)
{
    struct wheel *w = &wheels[wheel_open];
    if (wheel_sel < 0 || !w->plates[wheel_sel].bound || !w->plates[wheel_sel].available)
        return;
    const struct binding *b = &w->plates[wheel_sel].bind;
    if (w->kind == WK_CREW && b->kind == ACT_KEY) {
        /* Shift adds to the selection instead of replacing it. */
        tap_key(b->key, crew_selected());
    } else if (b->kind == ACT_KEY) {
        tap_key(b->key, b->shift);
        if (w->kind == WK_ACTION && ability_system(b->key) >= 0)
            last_ability_key = b->key;
    } else if (b->kind == ACT_LCLICK || b->kind == ACT_RCLICK) {
        click(b->kind, true);
        click(b->kind, false);
    } else if (b->kind == ACT_BUILTIN) {
        builtin((enum builtin)b->state, true, now_s());
        builtin((enum builtin)b->state, false, now_s());
    }
    if (debug)
        logmsg("wheel %s: %s", w->name, w->plates[wheel_sel].label);
    if (w->kind != WK_POWER && w->kind != WK_SYSTEMS && w->kind != WK_SETTINGS && w->kind != WK_CREW)
        wheel_close();
}

/* B in a wheel closes it. */
static void wheel_back(void)
{
    wheel_close();
}

/* D-pad up and down on a power or systems wheel: the selected item's key, with Shift to
 * take power away. On the crew wheel they add the member to the selection or drop them. */
static void wheel_power(int dir)
{
    struct wheel *w = &wheels[wheel_open];
    if (w->kind == WK_CREW) {
        if (wheel_sel < 0 || !w->plates[wheel_sel].available)
            return;
        void *member = crew_at(w->plates[wheel_sel].bind.key - KEY_F1);
        if (!member)
            return;
        if (dir > 0)
            tap_key(w->plates[wheel_sel].bind.key, crew_selected());
        else
            game.deselect_crew((char *)gui_ptr() + GUI_CREW_CONTROL, member);
        return;
    }
    if (w->kind != WK_POWER && w->kind != WK_SYSTEMS)
        return;
    if (wheel_sel < 0 || !w->plates[wheel_sel].bound || !w->plates[wheel_sel].available
            || w->plates[wheel_sel].bind.kind != ACT_KEY)
        return;
    tap_key(w->plates[wheel_sel].bind.key, dir < 0);
    if (debug)
        logmsg("wheel %s: %s power %s", w->name, w->plates[wheel_sel].label, dir < 0 ? "down" : "up");
}

static void set_shift(bool on)
{
    shift_count += on ? 1 : -1;
    if (on && shift_count == 1)
        game.key_down(app, KEY_LSHIFT);
    else if (!on && shift_count == 0)
        game.key_up(app, KEY_LSHIFT);
}

static void click(enum act_kind kind, bool down)
{
    int wx, wy;
    to_window(cursor.x, cursor.y, &wx, &wy);
    if (kind == ACT_LCLICK) {
        (down ? game.lbutton_down : game.lbutton_up)(app, wx, wy);
        cursor.l_held = down;
    } else {
        (down ? game.rbutton_down : game.rbutton_up)(app, wx, wy);
        cursor.r_held = down;
    }
}

/* Moves the cursor to the player's or the enemy's ship centre, whichever is farther. */
static void center_swap(void)
{
    void *gui = gui_ptr();
    void *player = ship_of(player_complete());
    void *enemy = ship_of(enemy_complete());
    float px, py, ex, ey;
    bool p = player && ship_center(player, read_point(gui, GUI_SHIP_POSITION), &px, &py);
    bool e = enemy && ship_center(enemy, enemy_origin(gui), &ex, &ey);
    if (!p && !e)
        return;
    bool to_enemy = e && (!p || hypotf(px - cursor.x, py - cursor.y) < hypotf(ex - cursor.x, ey - cursor.y));
    cursor.x = to_enemy ? ex : px;
    cursor.y = to_enemy ? ey : py;
    send_cursor();
}

/* The first crew member of the current selection, or NULL. */
static const void *first_selected_crew(void)
{
    const char *gui = gui_ptr();
    if (!gui)
        return NULL;
    const void *selected = gui + GUI_CREW_CONTROL + CREW_SELECTED;
    if (vector_count(selected, sizeof(void *)) == 0)
        return NULL;
    return vector_at(selected, sizeof(void *), 0);
}

/* Moves the cursor to the room of the first selected crew member. */
static void cursor_to_selected_crew(void)
{
    const char *member = first_selected_crew();
    if (!member)
        return;
    int room = read_int(member, CREW_ROOM);
    bool on_player = read_int(member, CREW_SHIP) == PLAYER_SHIP_ID;
    for (int i = 0; i < ntargets; i++)
        if (targets[i].id == room && targets[i].kind == (on_player ? T_ROOM : T_ENEMY_ROOM)) {
            cursor.x = targets[i].rect.x + targets[i].rect.w / 2.0f;
            cursor.y = targets[i].rect.y + targets[i].rect.h / 2.0f;
            send_cursor();
            if (debug)
                logmsg("cursor to crew room %d", room);
            return;
        }
}

/* Clicks queued for later frames: the game resolves what sits under the pointer a frame
 * after the move, so each click takes a move, a press and a release on three frames. */
#define MAX_QUEUED_CLICKS 16
/* Frames between the phases of a queued click. */
#define QUEUED_CLICK_FRAMES 4
static struct {
    float x, y;
} queued_clicks[MAX_QUEUED_CLICKS];
static int nqueued_clicks, queued_phase, queued_wait;

static void click_at(float gx, float gy)
{
    if (nqueued_clicks < MAX_QUEUED_CLICKS) {
        queued_clicks[nqueued_clicks].x = gx;
        queued_clicks[nqueued_clicks].y = gy;
        nqueued_clicks++;
    }
}

/* Runs one step of the queued clicks; the cursor returns once the queue is empty. */
static void run_queued_clicks(void)
{
    if (nqueued_clicks == 0)
        return;
    if (queued_wait > 0) {
        queued_wait--;
        return;
    }
    queued_wait = QUEUED_CLICK_FRAMES - 1;
    int wx, wy;
    to_window(queued_clicks[0].x, queued_clicks[0].y, &wx, &wy);
    if (queued_phase == 0) {
        game.mouse_move(app, wx, wy, 0, 0, false, false, false);
    } else if (queued_phase == 1) {
        game.lbutton_down(app, wx, wy);
    } else {
        game.lbutton_up(app, wx, wy);
        memmove(queued_clicks, queued_clicks + 1, (size_t)(nqueued_clicks - 1) * sizeof *queued_clicks);
        nqueued_clicks--;
        if (nqueued_clicks == 0) {
            to_window(cursor.x, cursor.y, &wx, &wy);
            game.mouse_move(app, wx, wy, 0, 0, false, false, false);
        }
    }
    queued_phase = (queued_phase + 1) % 3;
    cursor.last_set = *game.mouse_position;
}

/* Toggles the doors of the player room under the cursor: closes them all while any is
 * open, else opens them all. Each door is clicked, as the game toggles them. */
static void room_doors(void)
{
    int under = target_under_cursor();
    if (under < 0 || targets[under].kind != T_ROOM)
        return;
    int room = targets[under].id;
    const char *gui = gui_ptr();
    void *ship = ship_of(player_complete());
    if (!gui || !ship)
        return;
    Point origin = read_point(gui, GUI_SHIP_POSITION);
    const void *lists[2] = {(const char *)ship + SHIP_DOORS, (const char *)ship + SHIP_AIRLOCKS};
    bool any_open = false;
    for (int l = 0; l < 2; l++) {
        size_t n = vector_count(lists[l], sizeof(void *));
        for (size_t i = 0; i < n; i++) {
            const char *door = vector_at(lists[l], sizeof(void *), i);
            if (door && (read_int(door, DOOR_ROOM1) == room || read_int(door, DOOR_ROOM2) == room))
                any_open |= *(door + DOOR_OPEN) != 0;
        }
    }
    for (int l = 0; l < 2; l++) {
        size_t n = vector_count(lists[l], sizeof(void *));
        for (size_t i = 0; i < n; i++) {
            const char *door = vector_at(lists[l], sizeof(void *), i);
            if (!door || (read_int(door, DOOR_ROOM1) != room && read_int(door, DOOR_ROOM2) != room))
                continue;
            if ((*(door + DOOR_OPEN) != 0) == any_open)
                click_at((float)(origin.x + read_int(door, DOOR_X)), (float)(origin.y + read_int(door, DOOR_Y)));
        }
    }
    if (debug)
        logmsg("room %d doors %s", room, any_open ? "closed" : "opened");
}

/* Selects the living crew member after the first selected one, wrapping. */
static void select_next_crew(void)
{
    int count = 0;
    while (crew_at(count))
        count++;
    if (count == 0)
        return;
    int start = -1;
    for (int i = 0; i < count && start < 0; i++)
        if (crew_is_selected(crew_at(i)))
            start = i;
    for (int n = 1; n <= count; n++) {
        int i = (start + n) % count;
        const char *member = crew_at(i);
        if (member && !*(member + CREW_DEAD)) {
            tap_key(KEY_F1 + i, false);
            return;
        }
    }
}

/* Arms the next powered weapon after the armed one, wrapping. */
static void arm_next(void)
{
    const char *control = weapon_control();
    if (!control)
        return;
    const void *armed = deref(control, WEAPONCTRL_ARMED);
    int start = -1;
    for (int i = 0; i < WEAPON_SLOTS; i++)
        if (armed && armament_item(control, i) == armed)
            start = i;
    for (int n = 1; n <= WEAPON_SLOTS; n++) {
        int i = (start + n) % WEAPON_SLOTS;
        const char *weapon = armament_item(control, i);
        if (weapon && *(weapon + WEAPON_POWERED)) {
            tap_key('1' + i, false);
            return;
        }
    }
}

static void tab_step(int dir)
{
    char *tabbed = open_tabbed();
    if (!tabbed)
        return;
    const void *buttons_vec = tabbed + TABBED_BUTTONS;
    int n = (int)vector_count(buttons_vec, sizeof(void *));
    if (n <= 0)
        return;
    int current = (int)*(unsigned *)(tabbed + TABBED_CURRENT);
    game.set_tab(tabbed, (unsigned)((current + dir + n) % n));
}

/* Runs a builtin on press or release. */
static void builtin(enum builtin id, bool down, double t)
{
    switch (id) {
    case BI_CONTEXT_CLICK:
        if (wheel_open >= 0) {
            if (down)
                wheel_act();
        } else {
            /* With crew selected and a room under the cursor, A sends them there, which
             * the game does on the right button. */
            static bool crew_order;
            if (down) {
                int under = target_under_cursor();
                crew_order = crew_selected() && !aiming() && under >= 0
                             && (targets[under].kind == T_ROOM || targets[under].kind == T_ENEMY_ROOM);
            }
            click(crew_order ? ACT_RCLICK : ACT_LCLICK, down);
        }
        break;
    case BI_CANCEL:
        if (!down)
            break;
        if (wheel_open >= 0)
            wheel_back();
        else if (gui_ptr() && (window_open((char *)gui_ptr() + GUI_MENU_BOX)
                               || window_open((char *)gui_ptr() + GUI_OPTIONS_BOX)))
            tap_key(KEY_ESCAPE, false);
        else if (aiming()) {
            /* The right button cancels targeting. */
            click(ACT_RCLICK, true);
            click(ACT_RCLICK, false);
        } else if (crew_selected()) {
            game.clear_crew((char *)gui_ptr() + GUI_CREW_CONTROL);
        } else if (open_tabbed() || (gui_ptr() && window_open((char *)gui_ptr() + GUI_CHOICE_BOX))
                   || (gui_ptr() && window_open(deref(gui_ptr(), GUI_STAR_MAP)))) {
            tap_key(KEY_ESCAPE, false);
        }
        break;
    case BI_PAUSE_FF:
        if (down) {
            tap_key(KEY_SPACE, false);
            pause_ff_since = t;
        } else {
            if (fast_forward)
                game.key_up(app, KEY_F12);
            fast_forward = false;
            pause_ff_since = 0;
        }
        break;
    case BI_ARM_NEXT:
        if (down)
            arm_next();
        break;
    case BI_AIM_CREW:
        /* On a tabbed screen the shoulder tap walks its tabs instead. */
        if (down)
            open_tabbed() ? tab_step(1) : select_next_crew();
        break;
    case BI_REPEAT_ABILITY:
        if (down && last_ability_key)
            tap_key(last_ability_key, false);
        break;
    case BI_PRECISION:
        /* Held: doors are the zones for as long as the button is down. */
        cfg.precision = down;
        break;
    case BI_CENTER_SWAP:
        if (down)
            center_swap();
        break;
    case BI_PREV_TAB:
    case BI_NEXT_TAB:
        if (down)
            tab_step(id == BI_NEXT_TAB ? 1 : -1);
        break;
    case BI_HELP:
        if (down)
            cfg.help = !cfg.help;
        break;
    case BI_ROOM_DOORS:
        if (down)
            room_doors();
        break;
    case BI_PAUSE:
        if (down)
            tap_key(KEY_SPACE, false);
        break;
    case BI_DPAD_MODE:
        if (down)
            dpad_mouse = !dpad_mouse;
        break;
    case BI_FAST_FORWARD:
        if (down) {
            if (game_paused())
                tap_key(KEY_SPACE, false);
            game.key_down(app, KEY_F12);
            memcpy((char *)game.fps_control + FPS_LEVEL, &cfg.speed_level, sizeof cfg.speed_level);
            fast_forward = true;
        } else {
            game.key_up(app, KEY_F12);
            fast_forward = false;
        }
        break;
    case BI_COUNT:
        break;
    }
}

/* Performs a binding's press or release. */
static void perform(const struct binding *b, enum input in, bool down, double t)
{
    switch (b->kind) {
    case ACT_NONE:
        break;
    case ACT_KEY:
        if (down) {
            if (b->shift)
                set_shift(true);
            game.key_down(app, b->key);
        } else {
            game.key_up(app, b->key);
            if (b->shift)
                set_shift(false);
        }
        break;
    case ACT_LCLICK:
    case ACT_RCLICK:
        click(b->kind, down);
        break;
    case ACT_STATE:
        if (down) {
            if (depth + 1 < MAX_STATES) {
                depth++;
                stack[depth].state = b->state;
                stack[depth].by = in;
            }
        } else {
            for (int level = depth; level > 0; level--)
                if (stack[level].by == in) {
                    depth = level - 1;
                    break;
                }
        }
        break;
    case ACT_WHEEL:
        if (down)
            wheel_show(b->state, in);
        else if (wheel_open >= 0 && wheel_by == in && !wheel_sticky)
            wheel_close();
        break;
    case ACT_BUILTIN:
        builtin((enum builtin)b->state, down, t);
        break;
    }
    if (debug && down)
        logmsg("%s: %s", input_ini_names[in],
               b->kind == ACT_KEY ? "key" : b->kind == ACT_BUILTIN ? builtin_names[b->state]
               : b->kind == ACT_WHEEL ? "wheel" : b->kind == ACT_STATE ? "state" : "click");
}

/* Whether a tap binding would do nothing in the current context. */
static bool tap_idle(const struct binding *tap)
{
    if (tap->kind != ACT_BUILTIN)
        return false;
    switch ((enum builtin)tap->state) {
    case BI_PREV_TAB:
    case BI_NEXT_TAB:
        return !open_tabbed();
    case BI_REPEAT_ABILITY:
        return !last_ability_key;
    default:
        return false;
    }
}

/* A press runs its binding at once, unless it carries a tap half that would act: then it
 * waits for the threshold (hold) or the release (tap). */
static void press(enum input in, double t)
{
    const struct binding *b = lookup(in);
    active[in].kind = ACT_NONE;
    if (!b)
        return;
    /* The open wheel's own button closes it, its tap half included. */
    if (b->kind == ACT_WHEEL && b->state == wheel_open) {
        wheel_close();
        return;
    }
    if (b->tap && !tap_idle(b->tap)) {
        pending[in].pending = true;
        pending[in].since = t;
        return;
    }
    perform(b, in, true, t);
    active[in].kind = b->kind;
    active[in].key = b->key;
    active[in].shift = b->shift;
    active[in].state = b->state;
}

static void release(enum input in, double t)
{
    if (pending[in].pending) {
        pending[in].pending = false;
        const struct binding *b = lookup(in);
        if (b && b->tap) {
            perform(b->tap, in, true, t);
            perform(b->tap, in, false, t);
        }
        return;
    }
    struct binding b = {active[in].kind, active[in].key, active[in].shift, 0, NULL};
    if (b.kind == ACT_STATE || b.kind == ACT_WHEEL || b.kind == ACT_BUILTIN)
        b.state = active[in].state;
    perform(&b, in, false, t);
    active[in].kind = ACT_NONE;
}

/* Turns presses that outlived the tap threshold into holds. */
static void settle_pending(double t)
{
    for (int in = 0; in < IN_COUNT; in++) {
        if (!pending[in].pending || t - pending[in].since < TAP_HOLD_S)
            continue;
        pending[in].pending = false;
        const struct binding *b = lookup(in);
        if (!b)
            continue;
        perform(b, (enum input)in, true, t);
        active[in].kind = b->kind;
        active[in].key = b->key;
        active[in].shift = b->shift;
        active[in].state = b->state;
    }
}

/* ---- pad reading ---- */

/* Reads a stick as four directions with hysteresis; only the dominant axis is active. */
static void stick_directions(float x, float y, bool out[4], const bool was[4])
{
    for (int i = 0; i < 4; i++)
        out[i] = false;
    for (int i = 0; i < 4; i++)
        if (was[i]) {
            float v = (i == 0 || i == 2) ? x : y;
            bool still = (i == 0 || i == 1) ? v <= -STICK_KEY_OFF : v >= STICK_KEY_OFF;
            out[i] = still;
            return;
        }
    bool horizontal = fabsf(x) >= fabsf(y);
    out[0] = horizontal && x <= -STICK_KEY_ON;
    out[2] = horizontal && x >= STICK_KEY_ON;
    out[1] = !horizontal && y <= -STICK_KEY_ON;
    out[3] = !horizontal && y >= STICK_KEY_ON;
}

static int pad_index = -1;

/* Reads a:bN .. y:bN out of the SDL mapping string. */
static void load_sdl_map(void)
{
    const char *map = getenv(SDL_MAP_ENV);
    if (!map)
        return;
    for (int i = 0; i < 4; i++) {
        char token[8];
        snprintf(token, sizeof token, ",%s:b", sdl_face_names[i]);
        const char *at = strstr(map, token);
        if (at)
            sdl_face[i] = atoi(at + strlen(token));
    }
    logmsg("sdl map: a=%d b=%d x=%d y=%d", sdl_face[0], sdl_face[1], sdl_face[2], sdl_face[3]);
}

static void buzz(void)
{
    if (cfg.rumble && pad_index >= 0)
        joy.rumble(pad_index, RUMBLE_STRENGTH, RUMBLE_STRENGTH, RUMBLE_S);
}

static int active_pad(void)
{
    int n = joy.count();
    for (int i = 0; i < n; i++)
        if (joy.connected(i))
            return i;
    return -1;
}

struct pad {
    bool down[IN_COUNT];
    float stick[2][2];
    int dpad[2];
};

static void read_pad(int dev, struct pad *p)
{
    for (int b = 0; b < SIL_BUTTONS; b++) {
        int idx = joy.button_mapping(dev, b);
        p->down[b] = idx >= 0 && joy.button_state(dev, idx);
    }
    bool mapped = cfg.nintendo < 0 && sdl_face[0] >= 0 && sdl_face[1] >= 0 && sdl_face[2] >= 0 && sdl_face[3] >= 0;
    if (mapped) {
        for (int i = 0; i < 4; i++)
            p->down[sdl_face_inputs[i]] = joy.button_state(dev, sdl_face[i]);
    } else if (cfg.nintendo != 0) {
        if (cfg.swap_ab) {
            bool south = p->down[IN_FACE_DOWN];
            p->down[IN_FACE_DOWN] = p->down[IN_FACE_RIGHT];
            p->down[IN_FACE_RIGHT] = south;
        }
        if (cfg.swap_xy) {
            bool west = p->down[IN_FACE_LEFT];
            p->down[IN_FACE_LEFT] = p->down[IN_FACE_UP];
            p->down[IN_FACE_UP] = west;
        }
    }
    if (joy.num_sticks(dev) > TRIGGER_STICK) {
        float l2, r2;
        joy.read_stick(dev, TRIGGER_STICK, &l2, &r2);
        if (joy.button_mapping(dev, IN_L2) < 0)
            p->down[IN_L2] = l2 > TRIGGER_ON;
        if (joy.button_mapping(dev, IN_R2) < 0)
            p->down[IN_R2] = r2 > TRIGGER_ON;
    }
    joy.read_dpad(dev, &p->dpad[0], &p->dpad[1]);
    p->down[IN_DPAD_LEFT] = p->dpad[0] < 0;
    p->down[IN_DPAD_RIGHT] = p->dpad[0] > 0;
    p->down[IN_DPAD_UP] = p->dpad[1] < 0;
    p->down[IN_DPAD_DOWN] = p->dpad[1] > 0;
    joy.read_stick(dev, LEFT_STICK, &p->stick[0][0], &p->stick[0][1]);
    joy.read_stick(dev, RIGHT_STICK, &p->stick[1][0], &p->stick[1][1]);
    stick_directions(p->stick[0][0], p->stick[0][1], p->down + IN_LSTICK_LEFT, held + IN_LSTICK_LEFT);
    stick_directions(p->stick[1][0], p->stick[1][1], p->down + IN_RSTICK_LEFT, held + IN_RSTICK_LEFT);
}

static bool is_dpad(int i)
{
    return i >= IN_DPAD_LEFT && i <= IN_DPAD_DOWN;
}

/* Applies one frame of pad state: pointer sources move the cursor or flick the magnet,
 * sticks point an open wheel, everything else goes through the layout. */
static void apply_pad(struct pad *p, double t, float dt)
{
    static const int dir_dx[4] = {-1, 0, 1, 0}, dir_dy[4] = {0, -1, 0, 1};
    static bool dpad_spent[4];
    static long frame;
    enum slot_mode mode[SLOT_COUNT];
    for (int s = 0; s < SLOT_COUNT; s++) {
        mode[s] = slot_mode((enum slot)s);
        /* The mode toggle: cursor on the D-pad and the sticks, or zones and flicks. */
        if (wheel_open < 0) {
            if (s == SLOT_DPAD && dpad_mouse)
                mode[s] = SLOT_MOUSE;
            if (s != SLOT_DPAD && !dpad_mouse && mode[s] == SLOT_MOUSE)
                mode[s] = SLOT_FLICK;
        }
        if (mode[s] != SLOT_KEYS)
            for (int d = 0; d < 4; d++)
                p->down[slot_first[s] + d] = false;
    }
    bool in_wheel = wheel_open >= 0;
    if (in_wheel) {
        refresh_wheel();
        wheel_point(p->stick[0][0], p->stick[0][1], p->stick[1][0], p->stick[1][1]);
        if (aiming() && !wheel_opened_aiming)
            wheel_close();
        if (!aiming())
            wheel_opened_aiming = false;
    }
    bool snapping = cfg.snap_dpad && depth == 0 && ntargets > 0 && mode[SLOT_DPAD] == SLOT_KEYS;
    if (++frame % BUTTON_REFRESH_FRAMES == 0 && cursor.synced && !cursor.l_held && !cursor.r_held
            && nqueued_clicks == 0)
        game.mouse_move(app, cursor.last_wx, cursor.last_wy, 0, 0, false, false, false);

    settle_pending(t);
    for (int i = 0; i < IN_COUNT; i++) {
        if (p->down[i] == held[i])
            continue;
        held[i] = p->down[i];
        /* Any button closes the help; that press does nothing else. */
        if (cfg.help && p->down[i] && i < SIL_BUTTONS) {
            cfg.help = false;
            continue;
        }
        if (is_dpad(i)) {
            int d = i - IN_DPAD_LEFT;
            if (p->down[i] && (in_wheel || snapping)) {
                dpad_spent[d] = true;
                if (in_wheel) {
                    if (d == 0 || d == 2)
                        wheel_step(d == 2 ? 1 : -1);
                    else
                        wheel_power(d == 1 ? 1 : -1);
                } else {
                    focus_move((float)dir_dx[d], (float)dir_dy[d]);
                }
                continue;
            }
            if (!p->down[i] && dpad_spent[d]) {
                dpad_spent[d] = false;
                continue;
            }
        }
        if (p->down[i])
            press((enum input)i, t);
        else
            release((enum input)i, t);
    }

    if (!in_wheel) {
        for (int s = SLOT_LSTICK; s <= SLOT_RSTICK; s++) {
            float x = p->stick[s - SLOT_LSTICK][0], y = p->stick[s - SLOT_LSTICK][1];
            if (mode[s] == SLOT_MOUSE) {
                move_cursor(x, y, dt);
            } else if (mode[s] == SLOT_FLICK) {
                int k = s - SLOT_LSTICK;
                float mag = hypotf(x, y);
                if (mag < FLICK_OFF) {
                    flick_armed[k] = true;
                } else if (mag >= cfg.flick_on
                           && (flick_armed[k] || (cfg.flick_repeat > 0 && t - flick_last[k] >= cfg.flick_repeat))) {
                    flick_armed[k] = false;
                    flick_last[k] = t;
                    focus_move(x, y);
                }
            }
        }
        if (mode[SLOT_DPAD] == SLOT_MOUSE && (p->dpad[0] || p->dpad[1]))
            move_cursor((float)p->dpad[0], (float)p->dpad[1], dt);
    }

    /* Y held past the threshold: unpause and run fast. */
    if (pause_ff_since > 0 && !fast_forward && t - pause_ff_since >= FF_HOLD_S) {
        if (game_paused())
            tap_key(KEY_SPACE, false);
        game.key_down(app, KEY_F12);
        /* The patch stored its own level on the key; ours replaces it. */
        memcpy((char *)game.fps_control + FPS_LEVEL, &cfg.speed_level, sizeof cfg.speed_level);
        fast_forward = true;
    }

    /* Mode changes read from the game: the cursor starts where the payload wants it. */
    bool now_aiming = aiming();
    const void *now_crew = first_selected_crew();
    if (now_aiming && !prev_aiming) {
        void *gui = gui_ptr();
        void *enemy = ship_of(enemy_complete());
        float ex, ey;
        if (enemy && ship_center(enemy, enemy_origin(gui), &ex, &ey)) {
            cursor.x = ex;
            cursor.y = ey;
            send_cursor();
        }
        if (cfg.autopause_aim && !game_paused())
            tap_key(KEY_SPACE, false);
    }
    if (now_crew && now_crew != prev_crew)
        cursor_to_selected_crew();
    prev_aiming = now_aiming;
    prev_crew = now_crew;

    if (held[IN_SELECT] && held[IN_START]) {
        if (chord_since == 0)
            chord_since = t;
        if (!exit_requested && t - chord_since >= EXIT_HOLD_S) {
            exit_requested = true;
            logmsg("Select + Start: exit");
            game.request_exit(app);
        }
    } else {
        chord_since = 0;
    }
}

/* ---- test commands ---- */

static void dump_targets(void)
{
    logmsg("dump: aiming=%d targets=%d cursor=%d,%d under=%d", aiming(), ntargets,
           cursor.last_x, cursor.last_y, target_under_cursor());
    void *gui = deref(app, CAPP_GUI);
    void *player = gui ? ship_of(deref(gui, GUI_SHIP_COMPLETE)) : NULL;
    void *enemy = gui ? ship_of(deref((char *)gui + GUI_COMBAT_CONTROL, COMBAT_CURRENT_TARGET)) : NULL;
    for (int i = 0; i < ntargets; i++) {
        struct target *t = &targets[i];
        int cx = t->rect.x + t->rect.w / 2, cy = t->rect.y + t->rect.h / 2;
        if (t->kind == T_ROOM || t->kind == T_ENEMY_ROOM) {
            bool is_enemy = t->kind == T_ENEMY_ROOM;
            void *ship = is_enemy ? enemy : player;
            Point origin = is_enemy ? enemy_origin(gui) : read_point(gui, GUI_SHIP_POSITION);
            int check = ship ? game.selected_room(ship, cx - origin.x, cy - origin.y, false) : -2;
            logmsg("  %s %d at %d,%d %dx%d, game says %d", target_kind_names[t->kind], t->id,
                   t->rect.x, t->rect.y, t->rect.w, t->rect.h, check);
        } else {
            logmsg("  %s %d at %d,%d %dx%d", target_kind_names[t->kind], t->id, t->rect.x,
                   t->rect.y, t->rect.w, t->rect.h);
        }
    }
}

static void dump_state(void)
{
    char line[LINE_LEN] = "";
    size_t used = 0;
    for (int level = 0; level <= depth; level++)
        used += (size_t)snprintf(line + used, sizeof line - used, " %s", state_at(level)->name);
    static const char *const mode_names[] = {"keys", "mouse", "flick"};
    logmsg("state:%s; wheel %s sel %d; shift %d; dpad %s, left %s, right %s; aiming %d crew %d paused %d precision %d",
           line, wheel_open >= 0 ? wheels[wheel_open].name : "-", wheel_sel, shift_count,
           mode_names[slot_mode(SLOT_DPAD)], mode_names[slot_mode(SLOT_LSTICK)],
           mode_names[slot_mode(SLOT_RSTICK)], aiming(), crew_selected(), game_paused(), cfg.precision);
}

static void run_command(char *line)
{
    int x, y, key;
    if (sscanf(line, "move %d %d", &x, &y) == 2) {
        cursor.x = (float)x;
        cursor.y = (float)y;
        send_cursor();
    } else if (strncmp(line, "click", 5) == 0) {
        click(ACT_LCLICK, true);
        click(ACT_LCLICK, false);
    } else if (strncmp(line, "rclick", 6) == 0) {
        click(ACT_RCLICK, true);
        click(ACT_RCLICK, false);
    } else if (sscanf(line, "key %d", &key) == 1) {
        game.key_down(app, key);
        game.key_up(app, key);
    } else if (strncmp(line, "dump", 4) == 0) {
        dump_targets();
    } else if (strncmp(line, "state", 5) == 0) {
        dump_state();
    } else if (sscanf(line, "font %d", &key) == 1) {
        plate_font = key;
    } else if (strncmp(line, "doors", 5) == 0) {
        int under = target_under_cursor();
        void *ship = ship_of(player_complete());
        const void *doors = ship ? (const char *)ship + SHIP_DOORS : NULL;
        size_t n = doors ? vector_count(doors, sizeof(void *)) : 0;
        for (size_t i = 0; i < n; i++) {
            const char *door = vector_at(doors, sizeof(void *), i);
            if (door && under >= 0 && (read_int(door, DOOR_ROOM1) == targets[under].id || read_int(door, DOOR_ROOM2) == targets[under].id))
                logmsg("door %zu rooms %d,%d at %d,%d open=%d", i, read_int(door, DOOR_ROOM1), read_int(door, DOOR_ROOM2),
                       read_int(door, DOOR_X), read_int(door, DOOR_Y), *(door + DOOR_OPEN));
        }
    } else if (strncmp(line, "roomdoors", 9) == 0) {
        room_doors();
    } else if (strncmp(line, "buttons", 7) == 0) {
        const char *gui = gui_ptr();
        const char *map = gui ? deref(gui, GUI_STAR_MAP) : NULL;
        for (int i = 0; i < nbuttons; i++)
            logmsg("button %d at %d,%d %dx%d gui+%ld map+%ld", i, buttons[i].x, buttons[i].y, buttons[i].w, buttons[i].h,
                   gui ? (long)((const char *)button_objects[i] - gui) : -1L,
                   map ? (long)((const char *)button_objects[i] - map) : -1L);
    } else if (strncmp(line, "wheel ", 6) == 0) {
        const char *name = trim(line + 6);
        wheel_close();
        for (int i = 0; i < nwheels; i++)
            if (strcmp(wheels[i].name, name) == 0)
                wheel_show(i, IN_HOME);
    } else if (sscanf(line, "pick %d", &key) == 1) {
        if (wheel_open >= 0 && key >= -1 && key < WHEEL_SLOTS)
            wheel_sel = key;
    } else if (strncmp(line, "badge", 5) == 0) {
        char text[LABEL_LEN * 3];
        badge_text(text, sizeof text);
        logmsg("badge: '%s' width %d", text, text[0] ? text_width(WHEEL_FONT, text) : 0);
    } else {
        logmsg("unknown command: %s", line);
    }
}

static void poll_commands(void)
{
    static char buf[CMD_BUF];
    static size_t used;
    if (cmd_fd < 0)
        return;
    ssize_t n = read(cmd_fd, buf + used, sizeof buf - 1 - used);
    if (n <= 0)
        return;
    used += (size_t)n;
    buf[used] = '\0';
    char *start = buf, *nl;
    while ((nl = strchr(start, '\n'))) {
        *nl = '\0';
        if (*start)
            run_command(start);
        start = nl + 1;
    }
    used = strlen(start);
    memmove(buf, start, used + 1);
}

/* ---- hooks ---- */

static void onloop_hook(void *self)
{
    static double last_t, last_beat;
    static long frames;
    app = self;
    frames++;
    double t = now_s();
    float dt = last_t ? (float)fmin(t - last_t, MAX_DT) : 0.0f;
    last_t = t;
    if (frames == 1)
        logmsg("first OnLoop");

    poll_files(t);
    run_queued_clicks();
    sync_cursor();
    build_targets();
    focus_first_on_open();
    poll_commands();
    struct pad pad = {{false}, {{0, 0}, {0, 0}}, {0, 0}};
    int dev = active_pad();
    pad_index = dev;
    if (dev >= 0)
        read_pad(dev, &pad);
    apply_pad(&pad, t, dt);

    if (debug && t - last_beat >= HEARTBEAT_S) {
        logmsg("%ld frames, pad %d, cursor %d,%d, depth %d", frames, dev, cursor.last_x,
               cursor.last_y, depth);
        last_beat = t;
    }
    onloop_orig(self);
}

static void mouse_render_hook(void *self)
{
    if (wheel_open >= 0)
        draw_wheel();
    if (cfg.help && cursor.synced)
        draw_help();
    if (cursor.synced && cfg.overlay && gui_ptr()) {
        char text[LABEL_LEN * 3];
        badge_text(text, sizeof text);
        if (text[0]) {
            float w = (float)text_width(WHEEL_FONT, text) + 2 * BADGE_PAD;
            float x = GAME_W - BADGE_MARGIN - w, y = BADGE_MARGIN;
            draw_panel(x, y, w, BADGE_H, plate_color, plate_edge_color);
            draw_text_center(WHEEL_FONT, x + w / 2, y + WHEEL_TEXT_DY, text);
        }
    }
    /* The game's pointer would cover the wheel's middle. */
    if (wheel_open < 0)
        mouse_render_orig(self);
}

/* What the game's own hotkeys do, as the help names them. */
static const struct {
    int code;
    const char *title;
} key_titles[] = {
    {KEY_ESCAPE, "Menu"}, {KEY_SPACE, "Pause"}, {'j', "Jump"}, {'u', "Upgrades"},
    {'v', "Autofire"}, {'z', "Open doors"}, {'x', "Close doors"}, {'q', "All crew"},
    {KEY_ENTER, "To stations"}, {'/', "Save stations"},
};

static void describe_binding(const struct binding *b, char *out, size_t len)
{
    switch (b->kind) {
    case ACT_KEY: {
        const char *title = NULL, *name = NULL;
        for (size_t i = 0; i < sizeof key_titles / sizeof *key_titles && !title; i++)
            if (key_titles[i].code == b->key)
                title = key_titles[i].title;
        for (size_t i = 0; i < sizeof key_names / sizeof *key_names && !name; i++)
            if (key_names[i].code == b->key)
                name = key_names[i].name;
        if (title)
            snprintf(out, len, "%s", title);
        else if (name)
            snprintf(out, len, "%s", name);
        else if (b->key >= KEY_F1 && b->key < KEY_F1 + KEY_F_COUNT)
            snprintf(out, len, "f%d", b->key - KEY_F1 + 1);
        else
            snprintf(out, len, "%c", b->key);
        break;
    }
    case ACT_LCLICK: snprintf(out, len, "click"); break;
    case ACT_RCLICK: snprintf(out, len, "right click"); break;
    case ACT_STATE: snprintf(out, len, "%s page", states[b->state].name); break;
    case ACT_WHEEL:
        snprintf(out, len, "%s wheel", wheels[b->state].name);
        out[0] = (char)toupper((unsigned char)out[0]);
        break;
    case ACT_BUILTIN: snprintf(out, len, "%s", builtin_titles[b->state]); break;
    default: out[0] = '\0';
    }
}

/* Whether a binding is something held rather than tapped. */
static bool is_hold_action(const struct binding *b)
{
    return b->kind == ACT_WHEEL || b->kind == ACT_STATE
           || (b->kind == ACT_BUILTIN && (b->state == BI_PRECISION || b->state == BI_FAST_FORWARD));
}

/* Draws the base state's bindings as a table: button, tap, hold. */
static void draw_help(void)
{
    const struct state *base = &states[0];
    int lines = 1;
    for (int in = 0; in < SIL_BUTTONS; in++)
        lines += base->bound[in];
    float h = HELP_LINE * lines + 2 * HELP_PAD;
    float x = (GAME_W - HELP_W) / 2, y = (GAME_H - h) / 2;
    draw_panel(x, y, HELP_W, h, plate_color, plate_edge_color);
    float line_y = y + HELP_PAD;
    draw_text_left(HELP_FONT, x + HELP_PAD, line_y + WHEEL_TEXT_DY, "BUTTON");
    draw_text_left(HELP_FONT, x + HELP_COL_TAP, line_y + WHEEL_TEXT_DY, "TAP");
    draw_text_left(HELP_FONT, x + HELP_COL_HOLD, line_y + WHEEL_TEXT_DY, "HOLD");
    line_y += HELP_LINE;
    game.draw_rect(x + HELP_PAD, line_y - 2, HELP_W - 2 * HELP_PAD, HELP_RULE_H, plate_edge_color);
    for (int in = 0; in < SIL_BUTTONS; in++) {
        if (!base->bound[in])
            continue;
        char hold[LABEL_LEN * 2] = "", tap[LABEL_LEN * 2] = "";
        const struct binding *b = &base->bind[in];
        if (b->tap) {
            describe_binding(b, hold, sizeof hold);
            describe_binding(b->tap, tap, sizeof tap);
        } else {
            describe_binding(b, is_hold_action(b) ? hold : tap, LABEL_LEN * 2);
        }
        draw_text_left(HELP_FONT, x + HELP_PAD, line_y + WHEEL_TEXT_DY, input_titles[in]);
        draw_text_left(HELP_FONT, x + HELP_COL_TAP, line_y + WHEEL_TEXT_DY, tap);
        draw_text_left(HELP_FONT, x + HELP_COL_HOLD, line_y + WHEEL_TEXT_DY, hold);
        line_y += HELP_LINE;
    }
}

static void upper_ascii(char *text)
{
    for (; *text; text++)
        *text = (char)toupper((unsigned char)*text);
}

static int text_width(int font, const char *text)
{
    void *str = NULL;
    char alloc = 0;
    game.string_ctor(&str, text, &alloc);
    int w = game.measure_width(font, &str);
    game.string_dtor(&str);
    return w;
}

/* The mode badge text, empty in the plain state. */
static void badge_text(char *out, size_t len)
{
    const char *gui = gui_ptr();
    const void *mouse = game.mouse_position;
    char main[LABEL_LEN * 2] = "";
    if (wheel_open >= 0) {
        out[0] = '\0';
        return;
    } else if (read_int(mouse, MOUSE_TELEPORTING)) {
        snprintf(main, sizeof main, "Teleport");
    } else if (read_int(mouse, MOUSE_MIND)) {
        snprintf(main, sizeof main, "Mind control");
    } else if (read_int(mouse, MOUSE_HACKING)) {
        snprintf(main, sizeof main, "Hacking");
    } else if (read_int(mouse, MOUSE_AIMING)) {
        const char *control = weapon_control();
        const char *armed = control ? deref(control, WEAPONCTRL_ARMED) : NULL;
        const char *bp = armed ? deref(armed, WEAPON_BLUEPRINT) : NULL;
        char name[LABEL_LEN] = "";
        if (bp)
            display_text(bp + BLUEPRINT_SHORT_TITLE, name, sizeof name);
        snprintf(main, sizeof main, name[0] ? "Target: %s" : "Target", name);
    } else if (crew_selected()) {
        const void *selected = gui + GUI_CREW_CONTROL + CREW_SELECTED;
        size_t n = vector_count(selected, sizeof(void *));
        const char *member = vector_at(selected, sizeof(void *), 0);
        char name[LABEL_LEN] = "";
        if (n == 1 && member)
            display_text(member + CREW_BLUEPRINT + CREWBP_NAME, name, sizeof name);
        if (name[0])
            snprintf(main, sizeof main, "Crew: %s", name);
        else
            snprintf(main, sizeof main, "Crew: %zu", n);
    } else if (window_open(deref(gui, GUI_STAR_MAP))) {
        snprintf(main, sizeof main, "Map");
    }
    const char *parts[3] = {cfg.precision ? "Doors" : NULL, fast_forward ? "Fast" : NULL, dpad_mouse ? "Mouse" : NULL};
    size_t used = (size_t)snprintf(out, len, "%s", main);
    for (int i = 0; i < 3; i++)
        if (parts[i] && used < len)
            used += (size_t)snprintf(out + used, len - used, "%s%s", used ? BADGE_SEPARATOR : "", parts[i]);
    upper_ascii(out);
}

static int get_shift_hook(void)
{
    return shift_count > 0 || get_shift_orig();
}

static void button_move_hook(void *self, int x, int y, bool silent)
{
    const char *gui = gui_ptr();
    bool crew_control_button = gui && (const char *)self >= gui + GUI_CREW_CONTROL
                               && (const char *)self < gui + GUI_CREW_CONTROL + CREW_CONTROL_SIZE;
    bool drawn = !*((const unsigned char *)self + BUTTON_RENDER_OFF);
    if (nbuttons_seen < MAX_BUTTONS && *((const unsigned char *)self + BUTTON_ACTIVE) && drawn && !crew_control_button) {
        Rect r;
        memcpy(&r, (const char *)self + BUTTON_HITBOX, sizeof r);
        /* The armament bars keep their buttons' boxes relative to the bar. */
        const char *bars[2] = {gui ? weapon_control() : NULL, gui ? drone_control() : NULL};
        const int bar_sizes[2] = {WEAPON_CONTROL_SIZE, DRONE_CONTROL_SIZE};
        for (int i = 0; i < 2; i++)
            if (bars[i] && (const char *)self >= bars[i] && (const char *)self < bars[i] + bar_sizes[i]) {
                Point at = read_point(bars[i], ARMAMENT_LOCATION);
                r.x += at.x;
                r.y += at.y;
            }
        bool corner = r.x < CORNER_BUTTON_MAX && r.y < CORNER_BUTTON_MAX && r.w <= CORNER_BUTTON_SIZE;
        if (r.w > 0 && r.h > 0 && !corner && r.x + r.w <= GAME_W && r.y + r.h <= GAME_H) {
            button_objects_seen[nbuttons_seen] = self;
            buttons_seen[nbuttons_seen++] = r;
        }
    }
    button_move_orig(self, x, y, silent);
}

__attribute__((constructor)) static void init(void)
{
    debug = getenv(DEBUG_ENV) != NULL;
    if (resolve_symbols() != 0) {
        logmsg("unknown build, inactive");
        return;
    }
    if (!mapped_executable(game.onloop)) {
        logmsg("game not mapped in this process, inactive");
        return;
    }
    /* The engine drops joystick events while the X window lacks focus. */
    joy.enable_unfocused(1);
    onloop_orig = hook(game.onloop, onloop_hook, onloop_prologue, sizeof onloop_prologue);
    if (!onloop_orig) {
        logmsg("OnLoop prologue differs, inactive");
        return;
    }
    mouse_render_orig = hook(game.mouse_render, mouse_render_hook, mouse_render_prologue,
                             sizeof mouse_render_prologue);
    get_shift_orig = hook(game.get_shift, get_shift_hook, get_shift_prologue,
                          sizeof get_shift_prologue);
    button_move_orig = hook(game.button_move, button_move_hook, button_move_prologue,
                            sizeof button_move_prologue);
    logmsg("active: overlay %s, shift %s, buttons %s", mouse_render_orig ? "on" : "off",
           get_shift_orig ? "on" : "off", button_move_orig ? "on" : "off");

    char text[sizeof default_layout];
    memcpy(text, default_layout, sizeof text);
    load_layout_text(text, "built-in default");
    poll_files(now_s());
    load_sdl_map();
    const char *cmd = getenv(CMD_ENV);
    if (cmd) {
        cmd_fd = open(cmd, O_RDONLY | O_NONBLOCK);
        logmsg("command channel %s", cmd_fd >= 0 ? "open" : "unavailable");
    }
}
