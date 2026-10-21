/* ftlpad: a gamepad layer for the Linux build of FTL, preloaded into the game process
 * (natively, or under box64 on ARM handhelds).
 *
 * The game's SIL engine scans /dev/input for joysticks and polls them every frame, but
 * nothing in the game reads the result. The library resolves the engine's joystick API
 * and the game's entry points from the executable's symbol table, hooks the game loop,
 * and drives the game from the pad: a virtual cursor fed to the mouse handlers, clicks,
 * key presses, and a Shift state the hotkey code reads. Hook sites are diverted through
 * a trampoline page mapped within a rel32 jump of the executable, since the library
 * itself loads far away. The game binary on disk is never changed.
 *
 * Modules: core/ resolves symbols, places hooks and runs the per-frame entry; config/
 * reads ftlpad.ini and the layout; game/ reads the game's objects, builds the zones and
 * performs actions on the game; input/ reads the pad, resolves bindings and moves the
 * cursor; ui/ draws wheels, help and the mode badge; debug/ serves the FIFO test channel.
 * This header holds what they share; game/offsets.h holds the game's object layout.
 *
 * The layout comes from a gptokeyb2-style ini: [controls] is the base state, each
 * [controls:NAME] a state a hold_state button pushes while held, overlay = parent letting
 * lookups fall through to the state below. A press records what it did so its release
 * undoes exactly that, whatever state is active by then. A binding may carry two actions,
 * "HOLD | TAP"; a tap half that would do nothing on the current screen makes the press a
 * hold at once.
 *
 * The cursor lives in the game's logical screen; the mouse handlers take window pixels,
 * so positions pass through the inverse of the game's own transform. Key codes are the
 * SDL1 keysyms the game's key handlers expect. Modes are read from the game, never
 * tracked: aiming from MouseControl, crew selection from CrewControl, screens from their
 * open flags.
 *
 * Zones for the D-pad come from the screen on top: an event's choices, the star map's
 * beacons or sector boxes, ship rooms and doors, equipment slots, and the buttons the
 * game hit-tests on mouse moves, collected by a hook on GenericButton::MouseMove and
 * refreshed with a synthetic move. Boxes kept relative to a parent (armament bars, the
 * star map) get the parent's position added.
 *
 * Wheels open around the cursor on the ship view; plates are icons drawn with the game's
 * own textures, fonts and renderer. Labels print through the game's font code, which
 * takes its bundled copy-on-write std::string, built and destroyed through the
 * constructors in the binary. Everything logs to stderr under one prefix.
 */
#ifndef FTLPAD_H
#define FTLPAD_H

#define _GNU_SOURCE
#include <ctype.h>
#include <elf.h>
#include <fcntl.h>
#include <locale.h>
#include <math.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#include "game/offsets.h"

#define MAPS "/proc/self/maps"
#define JMP_LEN 5
#define SPEED_LEVEL_DEFAULT 5
#define RUMBLE_STRENGTH 0.4f
#define RUMBLE_S 0.05f

/* Cursor defaults, overridden by the config file. */
#define STICK_DEADZONE 0.12f

/* Logical pixels per second at full deflection. */
#define CURSOR_SPEED 800.0f

#define CURSOR_CURVE 2.0f
#define OVERLAY_LINE 1.0f
#define MAX_TARGETS 64

/* Direction scoring: sideways distance counts this many times the forward distance. */
#define FOCUS_SIDE_WEIGHT 2.0f

#define MAX_STATES 8
#define MAX_WHEELS 8
#define WHEEL_SLOTS 8
#define NAME_LEN 32
#define LABEL_LEN 24
#define ICON_PATH_LEN 64
#define MAX_BUTTONS 64
#define TAP_HOLD_S 0.2f
#define FF_HOLD_S 0.25f
#define FLICK_ON 0.8f
#define FLICK_OFF 0.25f

/* Zero: one flick per deflection, the stick must return to centre. */
#define FLICK_REPEAT_S 0.0

/* Magnet cone half-angles: the narrow one first, the wide one when it finds nothing. */
#define CONE_NARROW_DEG 30.0f

#define CONE_WIDE_DEG 60.0f
#define BADGE_FONT 12

/* The game's outlined header font, as on its own buttons. */
#define WHEEL_FONT 62

#define LINE_LEN 256
#define STATE_SECTION "controls:"
#define WHEEL_SECTION "wheel:"
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

struct target {
    Rect rect;
    int id;
    enum target_kind kind;
};

/* SIL joystick API, resolved from the symbol table. */
struct joy_api {
    int (*count)(void);
    int (*connected)(int);
    int (*num_sticks)(int);
    int (*button_mapping)(int, int);
    int (*button_state)(int, int);
    void (*read_stick)(int, int, float *, float *);
    void (*read_dpad)(int, int *, int *);
    void (*enable_unfocused)(int);
    void (*rumble)(int, float, float, float);
};

extern struct joy_api joy;

/* Game entry points, resolved the same way. Hooked ones are kept as plain addresses. */
struct game_api {
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
    unsigned char *fps_loop;
    void (*get_text)(void *, void *, const void *, const void *);
    void *text_library;
    Point *mouse_position;
};

extern struct game_api game;

struct symbol_slot {
    const char *name;
    void **slot;
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

/* Whole-pointer sources a state can bind to mouse_movement. */
enum slot { SLOT_DPAD, SLOT_LSTICK, SLOT_RSTICK, SLOT_COUNT };

extern const enum input slot_first[SLOT_COUNT];
extern const char *const input_ini_names[IN_COUNT];
enum act_kind { ACT_NONE, ACT_KEY, ACT_LCLICK, ACT_RCLICK, ACT_STATE, ACT_WHEEL, ACT_BUILTIN };

enum builtin {
    BI_PAUSE_FF, BI_CANCEL, BI_CONTEXT_CLICK, BI_ARM_NEXT, BI_AIM_CREW, BI_REPEAT_ABILITY,
    BI_PRECISION, BI_CENTER_SWAP, BI_PREV_TAB, BI_NEXT_TAB, BI_HELP, BI_ROOM_DOORS, BI_PAUSE,
    BI_FAST_FORWARD, BI_DPAD_MODE, BI_COUNT
};

extern const char *const builtin_names[BI_COUNT];

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

/* An action wheel closes after performing a plate, the others stay open; crew and
 * systems fill their plates from the game. */
enum wheel_kind { WK_ACTION, WK_POWER, WK_CREW, WK_SYSTEMS, WK_SETTINGS };

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

struct key_name {
    const char *name;
    int code;
};

extern const GL_Color plate_color;
extern const GL_Color plate_edge_color;
extern const GL_Color text_color;
extern const GL_Color text_disabled_color;
extern void (*mouse_render_orig)(void *);

struct config {
    float cursor_speed;
    float cursor_deadzone;
    float cursor_curve;
    bool overlay;
    bool snap_dpad;
    bool autopause_wheel;
    bool autopause_aim;
    int speed_level;
    bool rumble;
    float flick_on;
    float flick_repeat;
    /* Face buttons by label: -1 from the SDL mapping, with the swaps below when there is
     * none; 1 forces the swaps, 0 forces none. */
    int nintendo;
    /* Which raw pairs to swap for the labels: A/B (east/south) and X/Y (north/west). */
    bool swap_ab;
    bool swap_xy;
    float tap_hold;
    float pause_ff_hold;
    float focus_cone_narrow;
    float focus_cone_wide;
    float focus_side_weight;
    /* Menus, dialogs and the map open with the cursor on their first item. */
    bool focus_first;
    /* The cursor moves to the room of a newly selected crew member. */
    bool crew_follow;
    /* Wheels open only on the ship view. */
    bool wheels_ship_only;
    float rumble_strength;
    float rumble_time;
};

extern const struct config cfg_defaults;
extern struct config cfg;
extern bool precision;
extern bool help_shown;
extern bool debug;
extern void *app;
extern int cmd_fd;
extern int plate_font;
extern struct state states[MAX_STATES];
extern struct wheel wheels[MAX_WHEELS];
extern int nwheels;
extern int wheel_open;
extern int wheel_sel;
extern int wheel_refresh_wait;
extern bool wheel_opened_aiming;
extern int last_ability_key;
extern bool fast_forward;
extern float *step_cap;
extern float step_cap_normal;
extern bool dpad_mouse;
extern double pause_ff_since;

/* Presses waiting to turn into a tap or a hold, with the binding resolved at the press. */
struct pending_press {
    const struct binding *bind;
    double since;
};

extern struct pending_press pending[IN_COUNT];
extern Rect buttons_seen[MAX_BUTTONS], buttons[MAX_BUTTONS];
extern const void *button_objects_seen[MAX_BUTTONS], *button_objects[MAX_BUTTONS];
extern int nbuttons_seen, nbuttons;
extern bool buttons_tested;

/* Pushed states above the base, with the input holding each. */
struct stack_entry {
    int state;
    enum input by;
};

extern int depth;

/* The input slot of actions a wheel plate performs. */
#define NO_INPUT IN_COUNT

extern int shift_count;
extern struct target targets[MAX_TARGETS];
extern int ntargets;

struct cursor_state {
    float x, y;
    int last_x, last_y;
    int last_wx, last_wy;
    Point last_set;
    bool synced, l_held, r_held;
};

extern struct cursor_state cursor;
enum option_type { OPT_FLOAT, OPT_INT, OPT_BOOL };

/* Config file keys: the field each sets and the range a number is clamped to. */
struct option_def {
    const char *name;
    enum option_type type;
    size_t offset;
    float lo, hi;
};

struct layout_set {
    struct state states[MAX_STATES];
    int nstates;
    struct wheel wheels[MAX_WHEELS];
    int nwheels;
    /* Which states and wheels have a section of their own. */
    bool state_defined[MAX_STATES];
    bool wheel_defined[MAX_WHEELS];
};

/* Screens that open on their first item. */
enum first_screen { FS_NONE, FS_MAIN_MENU, FS_GAME_MENU, FS_CHOICE, FS_MAP };

/* Where a power wheel finds the item a slot key powers: weapons, then drones. */
struct armament {
    int first_key, slots;
    const char *(*control)(void);
    size_t blueprint, powered, power, icon;
    const char *icon_prefix, *icon_fallback;
    float icon_shift;
};

/* Clicks queued for later frames: the game resolves what sits under the pointer a frame
 * after the move, so each click takes a move, a press and a release on three frames. */
struct queued_click {
    float x, y;
};

extern int nqueued_clicks, queued_phase, queued_wait;
extern int pad_index;

struct pad {
    bool down[IN_COUNT];
    float stick[2][2];
    int dpad[2];
};

/* What the game's own hotkeys do, as the help names them. */
struct key_title {
    int code;
    const char *title;
};

/* ---- functions shared between modules ---- */

double now_s(void);
void logmsg(const char *fmt, ...);
float clampf(float v, float lo, float hi);
int resolve_symbols(void);
int mapped_executable(const void *addr);
void *hook(const char *name, void *fn, void *repl, const unsigned char *prologue, size_t len);
float *redirect_step_cap(void);
const char *key_name(int code);
char *trim(char *s);
void load_layout_text(char *text, const char *origin);
void load_layout(const char *path);
void poll_files(double t);
void *deref(const void *base, size_t off);
int read_int(const void *base, size_t off);
float read_float(const void *base, size_t off);
Point read_point(const void *base, size_t off);
size_t vector_count(const void *vec, size_t elem);
void *vector_at(const void *vec, size_t elem, size_t i);
void *ship_of(const void *complete);
void *gui_ptr(void);
void *player_complete(void);
void *enemy_complete(void);
void *player_manager(void);
bool game_paused(void);
bool window_open(const void *window);
void *open_tabbed(void);
bool crew_selected(void);
void *armament_item(const void *control, int i);
const char *weapon_control(void);
const char *drone_control(void);
bool ship_center(const void *ship, Point origin, float *cx, float *cy);
bool aiming(void);
Point enemy_origin(const void *gui);
bool inside(const void *p, const void *base, size_t size);
const char *modal_window(const char *gui, size_t *size);
void build_targets(void);
int target_under_cursor(void);
void to_window(float gx, float gy, int *wx, int *wy);
void sync_cursor(void);
void send_cursor(void);
void move_cursor(float rx, float ry, float dt);
void focus_move(float dx, float dy);
void focus_first_on_open(void);
const struct state *state_at(int level);
enum slot_mode slot_mode(enum slot slot);
void draw_text_left(int font, float x, float y, const char *text);
void draw_text_center(int font, float x, float y, const char *text);
void draw_panel(float x, float y, float w, float h, GL_Color fill, GL_Color edge);
void draw_disc(float cx, float cy, float r, GL_Color color);
void *game_image(const char *path);
void draw_wheel(void);
void wheel_point(float lx, float ly, float rx, float ry);
void wheel_step(int dir);
void *crew_at(int index);
bool crew_is_selected(const void *member);
const char *game_string(const void *str);
void display_text(const void *text, char *out, size_t len);
int system_key(int type);
const char *system_name(int type);
int ability_system(int key);
bool has_crystal_crew(void);
bool has_system(int type);
void refresh_wheel(void);
void tap_key(int key, bool shift);
void wheel_close(void);
void wheel_show(int index, enum input by);
void wheel_act(void);
void wheel_power(int dir);
void set_shift(bool on);
void click(enum act_kind kind, bool down);
void center_swap(void);
const void *first_selected_crew(void);
void cursor_to_selected_crew(void);
void run_queued_clicks(void);
void room_doors(void);
void select_next_crew(void);
void arm_next(void);
void tab_step(int dir);
void set_fast(bool on);
void builtin(enum builtin id, enum input in, bool down, double t);
void start(const struct binding *b, enum input in, double t);
bool wheel_screen(void);
void press(enum input in, double t);
void release(enum input in, double t);
void settle_pending(double t);
void load_sdl_map(void);
void buzz(void);
int active_pad(void);
void read_pad(int dev, struct pad *p);
void apply_pad(struct pad *p, double t, float dt);
void poll_commands(void);
void mouse_render_hook(void *self);
void upper_ascii(char *text);
int text_width(int font, const char *text);
void badge_text(char *out, size_t len);

#endif
