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
 * the cursor; the left stick picks one and the release performs its binding as a tap.
 * Labels print through the game's own font code, which takes its bundled copy-on-write
 * std::string, built and destroyed through the constructors in the binary.
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
#define CAPP_MODIFIER_X 14648
#define CAPP_MODIFIER_Y 14652
#define CAPP_MOUSE_MODIFIER_X 14668
#define CAPP_MOUSE_MODIFIER_Y 14672
#define CAPP_X_BAR 14692
#define CAPP_Y_BAR 14696
#define GUI_CREW_CONTROL 616
#define GUI_COMBAT_CONTROL 1416
#define GUI_SHIP_COMPLETE 6512
#define GUI_SHIP_POSITION 6656
#define GUI_STAR_MAP 6504
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
#define BUTTON_HITBOX 16
#define BUTTON_ACTIVE 72
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
#define KEY_F_COUNT 15

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
#define OVERLAY_HALF 10
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
#define MAX_BUTTONS 64
/* Frames between synthetic mouse moves that refresh the button zones. */
#define BUTTON_REFRESH_FRAMES 6
#define WHEEL_ON 0.5f
#define WHEEL_OFF 0.3f
/* Plates sit on an ellipse, wider than tall, so neighbours do not touch. */
#define WHEEL_RADIUS_X 150.0f
#define WHEEL_RADIUS_Y 96.0f
#define WHEEL_PLATE_W 118.0f
#define WHEEL_PLATE_H 24.0f
#define WHEEL_MARGIN 8.0f
#define WHEEL_FONT 12
/* Text baseline sits this far below a plate's top edge. */
#define WHEEL_TEXT_DY 5.0f
/* A centred stick keeps the last plate this long, so releasing after it still fires. */
#define WHEEL_GRACE_S 0.35
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

enum target_kind { T_ROOM, T_ENEMY_ROOM, T_BEACON, T_SECTOR, T_CHOICE, T_BUTTON };
static const char *const target_kind_names[] = {"player room", "enemy room", "beacon", "sector", "choice", "button"};

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
    int (*selected_room)(void *, int, int, bool);
    void (*string_ctor)(void *, const char *, const void *);
    void (*string_dtor)(void *);
    void (*print_center)(int, float, float, const void *);
    void *button_move;
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
    {"_ZNSsC1EPKcRKSaIcE", (void **)&game.string_ctor},
    {"_ZNSsD1Ev", (void **)&game.string_dtor},
    {"_ZN8freetype16easy_printCenterEiffRKSs", (void **)&game.print_center},
    {"_ZN13GenericButton9MouseMoveEiib", &game.button_move},
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

enum act_kind { ACT_NONE, ACT_KEY, ACT_LCLICK, ACT_RCLICK, ACT_STATE, ACT_WHEEL };

struct binding {
    enum act_kind kind;
    int key;
    bool shift;
    int state;
};

struct state {
    char name[NAME_LEN];
    bool parent;
    bool bound[IN_COUNT];
    struct binding bind[IN_COUNT];
    bool slot_mouse[SLOT_COUNT];
};

/* Eight plates clockwise from the top. */
static const char *const wheel_slot_names[WHEEL_SLOTS] = {
    "up", "up_right", "right", "down_right", "down", "down_left", "left", "up_left",
};

struct wheel {
    char name[NAME_LEN];
    bool bound[WHEEL_SLOTS];
    struct binding bind[WHEEL_SLOTS];
    char label[WHEEL_SLOTS][LABEL_LEN];
    /* Input that adds Shift to the chosen binding while held, or IN_COUNT. */
    int shift_input;
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

static const GL_Color overlay_color = {0.3f, 0.9f, 1.0f, 0.9f};
static const GL_Color focus_color = {1.0f, 0.85f, 0.2f, 0.9f};
static const GL_Color plate_color = {0.05f, 0.1f, 0.12f, 0.85f};
static const GL_Color plate_selected_color = {0.2f, 0.45f, 0.5f, 0.95f};
static const GL_Color plate_edge_color = {0.5f, 0.8f, 0.85f, 0.9f};

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
} cfg = {CURSOR_SPEED, STICK_DEADZONE, CURSOR_CURVE, true, true};

static bool debug;
static void *app;
static int cmd_fd = -1;

static struct state states[MAX_STATES];
static int nstates;
static struct wheel wheels[MAX_WHEELS];
static int nwheels;
/* The open wheel, the input holding it and the plate the left stick points at. */
static int wheel_open = -1;
static enum input wheel_by;
static int wheel_sel = -1;
static double wheel_centered_since;
/* Buttons the game hit-tested since the last frame, and the last full set. */
static Rect buttons_seen[MAX_BUTTONS], buttons[MAX_BUTTONS];
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
        else
            logmsg("config: unknown key %s", key);
    }
    fclose(f);
    logmsg("config: cursor_speed=%.0f cursor_deadzone=%.2f cursor_curve=%.1f overlay=%d snap_dpad=%d",
           cfg.cursor_speed, cfg.cursor_deadzone, cfg.cursor_curve, cfg.overlay, cfg.snap_dpad);
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
    w->shift_input = IN_COUNT;
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

/* Parses a binding value ("key [add_shift]", mouse_left, mouse_right, hold_state NAME,
 * wheel NAME) for the named input; returns false when nothing usable was found. */
static bool parse_value(struct layout_set *set, const char *key, char *value, struct binding *out)
{
    struct binding b = {ACT_NONE, 0, false, 0};
    char *save = NULL;
    char *tok = strtok_r(value, " \t", &save);
    if (!tok)
        return false;
    if (strcmp(tok, "mouse_left") == 0) {
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

/* Parses one "input = value" line of a controls section into the state. */
static void parse_binding(struct layout_set *set, struct state *s, char *key, char *value)
{
    if (strcmp(key, "overlay") == 0) {
        s->parent = strcmp(value, "parent") == 0;
        return;
    }
    int slot = lookup_name(slot_ini_names, SLOT_COUNT, key);
    if (slot >= 0) {
        s->slot_mouse[slot] = strcmp(value, "mouse_movement") == 0;
        if (!s->slot_mouse[slot] && *value)
            logmsg("layout: %s = %s not supported", key, value);
        return;
    }
    int in = lookup_name(input_ini_names, IN_COUNT, key);
    if (in < 0) {
        logmsg("layout: unknown input %s", key);
        return;
    }
    s->bound[in] = parse_value(set, key, value, &s->bind[in]);
}

/* Parses one line of a wheel section: a direction with a binding and a quoted label, or
 * "shift = INPUT". */
static void parse_wheel_line(struct layout_set *set, struct wheel *w, char *key, char *value)
{
    if (strcmp(key, "shift") == 0) {
        int in = lookup_name(input_ini_names, IN_COUNT, value);
        if (in < 0)
            logmsg("layout: unknown input %s for wheel shift", value);
        else
            w->shift_input = in;
        return;
    }
    int slot = lookup_name(wheel_slot_names, WHEEL_SLOTS, key);
    if (slot < 0) {
        logmsg("layout: unknown wheel direction %s", key);
        return;
    }
    char *quote = strchr(value, '"');
    if (quote) {
        char *end = strchr(quote + 1, '"');
        if (end)
            *end = '\0';
        strncpy(w->label[slot], quote + 1, LABEL_LEN - 1);
        *quote = '\0';
    }
    w->bound[slot] = parse_value(set, key, value, &w->bind[slot]);
    if (w->bound[slot] && !w->label[slot][0])
        strncpy(w->label[slot], key, LABEL_LEN - 1);
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

static void add_button_targets(void)
{
    for (int i = 0; i < nbuttons && ntargets < MAX_TARGETS; i++) {
        struct target *t = &targets[ntargets++];
        t->rect = buttons[i];
        t->id = i;
        t->kind = T_BUTTON;
    }
}

/* Rebuilds the snap targets for the current screen: beacons or sector boxes with the map
 * open, event choices with an event open, otherwise the rooms on screen (the enemy's only
 * while aiming); the buttons the game hit-tests come on top in every case. */
static void build_targets(void)
{
    ntargets = 0;
    if (nbuttons_seen > 0) {
        memcpy(buttons, buttons_seen, (size_t)nbuttons_seen * sizeof *buttons);
        nbuttons = nbuttons_seen;
        nbuttons_seen = 0;
    }
    void *gui = app ? deref(app, CAPP_GUI) : NULL;
    if (!gui)
        return;
    void *player = ship_of(deref(gui, GUI_SHIP_COMPLETE));
    if (player && !add_map_targets(gui) && !add_choice_targets(gui)) {
        void *enemy = ship_of(deref((const char *)gui + GUI_COMBAT_CONTROL, COMBAT_CURRENT_TARGET));
        if (!aiming())
            add_rooms(player, read_point(gui, GUI_SHIP_POSITION), false);
        if (enemy)
            add_rooms(enemy, enemy_origin(gui), true);
    }
    add_button_targets();
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

/* Moves the cursor to the target nearest in the given direction, by forward distance
 * plus weighted sideways distance; the nearest of all when none lies that way. */
static void focus_move(int dx, int dy)
{
    if (ntargets == 0)
        return;
    int from = target_under_cursor();
    float cx = cursor.x, cy = cursor.y;
    int best = -1;
    float best_score = 0;
    for (int pass = 0; pass < 2 && best < 0; pass++) {
        for (int i = 0; i < ntargets; i++) {
            if (i == from)
                continue;
            float tx = targets[i].rect.x + targets[i].rect.w / 2.0f;
            float ty = targets[i].rect.y + targets[i].rect.h / 2.0f;
            float vx = tx - cx, vy = ty - cy;
            float forward = vx * dx + vy * dy;
            float side = fabsf(vx * dy) + fabsf(vy * dx);
            float score;
            if (pass == 0) {
                if (forward <= 0 || side > forward)
                    continue;
                score = forward + FOCUS_SIDE_WEIGHT * side;
            } else {
                score = hypotf(vx, vy);
            }
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

/* Whether a slot moves the cursor in the active state chain: its mouse_movement binding
 * must resolve before any binding of its directions. */
static bool slot_is_mouse(enum slot slot)
{
    for (int level = depth; level >= 0; level--) {
        const struct state *s = state_at(level);
        for (int d = 0; d < 4; d++)
            if (s->bound[slot_first[slot] + d])
                return false;
        if (s->slot_mouse[slot])
            return true;
        if (level > 0 && !s->parent)
            break;
    }
    return false;
}

/* Prints text centred at x,y with the game's font code. */
static void draw_text_center(int font, float x, float y, const char *text)
{
    void *str = NULL;
    char alloc;
    game.string_ctor(&str, text, &alloc);
    game.print_center(font, x, y, &str);
    game.string_dtor(&str);
}

/* Plate centre for a wheel slot, around a centre kept inside the screen. */
static void plate_center(int slot, float *cx, float *cy)
{
    float reach_x = WHEEL_RADIUS_X + WHEEL_PLATE_W / 2 + WHEEL_MARGIN;
    float reach_y = WHEEL_RADIUS_Y + WHEEL_PLATE_H / 2 + WHEEL_MARGIN;
    float ox = clampf(cursor.x, reach_x, GAME_W - reach_x);
    float oy = clampf(cursor.y, reach_y, GAME_H - reach_y);
    float angle = (float)slot * (float)M_PI / 4.0f;
    *cx = ox + sinf(angle) * WHEEL_RADIUS_X;
    *cy = oy - cosf(angle) * WHEEL_RADIUS_Y;
}

static void draw_wheel(void)
{
    const struct wheel *w = &wheels[wheel_open];
    for (int i = 0; i < WHEEL_SLOTS; i++) {
        if (!w->bound[i])
            continue;
        float cx, cy;
        plate_center(i, &cx, &cy);
        float x = cx - WHEEL_PLATE_W / 2, y = cy - WHEEL_PLATE_H / 2;
        game.draw_rect(x, y, WHEEL_PLATE_W, WHEEL_PLATE_H, i == wheel_sel ? plate_selected_color : plate_color);
        game.draw_rect_outline((int)x, (int)y, (int)WHEEL_PLATE_W, (int)WHEEL_PLATE_H,
                               i == wheel_sel ? focus_color : plate_edge_color, OVERLAY_LINE);
        draw_text_center(WHEEL_FONT, cx, y + WHEEL_TEXT_DY, w->label[i]);
    }
}

/* Updates the pointed plate from the left stick, with hysteresis around the deadzone and
 * a grace period before a centred stick drops the selection. */
static void wheel_point(float x, float y, double t)
{
    float mag = hypotf(x, y);
    if (mag < (wheel_sel >= 0 ? WHEEL_OFF : WHEEL_ON)) {
        if (wheel_centered_since == 0)
            wheel_centered_since = t;
        else if (t - wheel_centered_since >= WHEEL_GRACE_S)
            wheel_sel = -1;
        return;
    }
    wheel_centered_since = 0;
    float angle = atan2f(x, -y);
    if (angle < 0)
        angle += 2.0f * (float)M_PI;
    int slot = (int)lroundf(angle / ((float)M_PI / 4.0f)) % WHEEL_SLOTS;
    wheel_sel = wheels[wheel_open].bound[slot] ? slot : -1;
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

/* Performs a wheel plate's binding as a press and release. */
static void tap_binding(const struct binding *b, bool shift)
{
    switch (b->kind) {
    case ACT_KEY:
        if (shift)
            set_shift(true);
        game.key_down(app, b->key);
        game.key_up(app, b->key);
        if (shift)
            set_shift(false);
        if (debug)
            logmsg("wheel: key %#x%s", b->key, shift ? " shift" : "");
        break;
    case ACT_LCLICK:
    case ACT_RCLICK:
        click(b->kind, true);
        click(b->kind, false);
        break;
    default:
        logmsg("wheel: only keys and clicks can sit on a plate");
        break;
    }
}

static void press(enum input in)
{
    const struct binding *b = lookup(in);
    active[in].kind = ACT_NONE;
    if (!b)
        return;
    switch (b->kind) {
    case ACT_NONE:
        break;
    case ACT_WHEEL:
        if (wheel_open < 0) {
            wheel_open = b->state;
            wheel_by = in;
            wheel_sel = -1;
            wheel_centered_since = 0;
            if (debug)
                logmsg("%s: wheel %s", input_ini_names[in], wheels[b->state].name);
        }
        break;
    case ACT_KEY:
        if (b->shift)
            set_shift(true);
        game.key_down(app, b->key);
        if (debug)
            logmsg("%s: key %#x%s", input_ini_names[in], b->key, b->shift ? " shift" : "");
        break;
    case ACT_LCLICK:
    case ACT_RCLICK:
        click(b->kind, true);
        if (debug)
            logmsg("%s: %s click", input_ini_names[in], b->kind == ACT_LCLICK ? "left" : "right");
        break;
    case ACT_STATE:
        if (depth + 1 < MAX_STATES) {
            depth++;
            stack[depth].state = b->state;
            stack[depth].by = in;
            if (debug)
                logmsg("%s: state %s", input_ini_names[in], states[b->state].name);
        }
        break;
    }
    active[in].kind = b->kind;
    active[in].key = b->key;
    active[in].shift = b->shift;
}

static void release(enum input in)
{
    switch (active[in].kind) {
    case ACT_NONE:
        break;
    case ACT_KEY:
        game.key_up(app, active[in].key);
        if (active[in].shift)
            set_shift(false);
        break;
    case ACT_LCLICK:
    case ACT_RCLICK:
        click(active[in].kind, false);
        break;
    case ACT_STATE:
        for (int level = depth; level > 0; level--)
            if (stack[level].by == in) {
                depth = level - 1;
                break;
            }
        break;
    case ACT_WHEEL:
        if (wheel_open >= 0 && wheel_by == in) {
            const struct wheel *w = &wheels[wheel_open];
            if (wheel_sel >= 0 && w->bound[wheel_sel]) {
                bool shift = w->bind[wheel_sel].shift
                             || (w->shift_input < IN_COUNT && held[w->shift_input]);
                tap_binding(&w->bind[wheel_sel], shift);
            }
            wheel_open = -1;
            wheel_sel = -1;
        }
        break;
    }
    active[in].kind = ACT_NONE;
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

/* Applies one frame of pad state: pointer sources move the cursor, everything else is
 * pressed and released through the layout. */
static void apply_pad(struct pad *p, double t, float dt)
{
    static const int dir_dx[4] = {-1, 0, 1, 0}, dir_dy[4] = {0, -1, 0, 1};
    static bool dpad_spent[4];
    static long frame;
    bool mouse[SLOT_COUNT];
    for (int s = 0; s < SLOT_COUNT; s++) {
        mouse[s] = slot_is_mouse((enum slot)s);
        if (mouse[s])
            for (int d = 0; d < 4; d++)
                p->down[slot_first[s] + d] = false;
    }
    if (wheel_open >= 0) {
        mouse[SLOT_LSTICK] = false;
        for (int d = 0; d < 4; d++)
            p->down[IN_LSTICK_LEFT + d] = false;
        wheel_point(p->stick[0][0], p->stick[0][1], t);
    }
    bool snapping = cfg.snap_dpad && depth == 0 && wheel_open < 0 && ntargets > 0 && !mouse[SLOT_DPAD];
    /* A synthetic move makes the game hit-test its buttons, which refreshes the zones. */
    if (++frame % BUTTON_REFRESH_FRAMES == 0 && cursor.synced && !cursor.l_held && !cursor.r_held)
        game.mouse_move(app, cursor.last_wx, cursor.last_wy, 0, 0, false, false, false);

    for (int i = 0; i < IN_COUNT; i++) {
        if (p->down[i] == held[i])
            continue;
        held[i] = p->down[i];
        if (is_dpad(i)) {
            int d = i - IN_DPAD_LEFT;
            if (p->down[i] && snapping) {
                dpad_spent[d] = true;
                focus_move(dir_dx[d], dir_dy[d]);
                continue;
            }
            if (!p->down[i] && dpad_spent[d]) {
                dpad_spent[d] = false;
                continue;
            }
        }
        if (p->down[i])
            press((enum input)i);
        else
            release((enum input)i);
    }

    if (mouse[SLOT_RSTICK])
        move_cursor(p->stick[1][0], p->stick[1][1], dt);
    if (mouse[SLOT_LSTICK])
        move_cursor(p->stick[0][0], p->stick[0][1], dt);
    if (mouse[SLOT_DPAD] && (p->dpad[0] || p->dpad[1]))
        move_cursor((float)p->dpad[0], (float)p->dpad[1], dt);

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
    logmsg("state:%s; wheel %s sel %d; shift %d; dpad %s, left %s, right %s", line,
           wheel_open >= 0 ? wheels[wheel_open].name : "-", wheel_sel, shift_count,
           slot_is_mouse(SLOT_DPAD) ? "mouse" : "keys", slot_is_mouse(SLOT_LSTICK) ? "mouse" : "keys",
           slot_is_mouse(SLOT_RSTICK) ? "mouse" : "keys");
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
    sync_cursor();
    build_targets();
    poll_commands();
    struct pad pad = {{false}, {{0, 0}, {0, 0}}, {0, 0}};
    int dev = active_pad();
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
    if (cursor.synced && cfg.overlay) {
        int under = target_under_cursor();
        if (under >= 0) {
            const Rect *r = &targets[under].rect;
            game.draw_rect_outline(r->x, r->y, r->w, r->h, focus_color, OVERLAY_LINE);
        }
        game.draw_rect_outline(cursor.last_x - OVERLAY_HALF, cursor.last_y - OVERLAY_HALF,
                               2 * OVERLAY_HALF, 2 * OVERLAY_HALF, overlay_color, OVERLAY_LINE);
    }
    if (wheel_open >= 0)
        draw_wheel();
    mouse_render_orig(self);
}

static int get_shift_hook(void)
{
    return shift_count > 0 || get_shift_orig();
}

static void button_move_hook(void *self, int x, int y, bool silent)
{
    if (nbuttons_seen < MAX_BUTTONS && *((const unsigned char *)self + BUTTON_ACTIVE)) {
        Rect r;
        memcpy(&r, (const char *)self + BUTTON_HITBOX, sizeof r);
        if (r.w > 0 && r.h > 0 && r.x >= 0 && r.y >= 0 && r.x + r.w <= GAME_W && r.y + r.h <= GAME_H)
            buttons_seen[nbuttons_seen++] = r;
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
    const char *cmd = getenv(CMD_ENV);
    if (cmd) {
        cmd_fd = open(cmd, O_RDONLY | O_NONBLOCK);
        logmsg("command channel %s", cmd_fd >= 0 ? "open" : "unavailable");
    }
}
