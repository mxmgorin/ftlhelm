/* Load-time setup and the per-frame entry: the CApp::OnLoop hook drives polling,
 * zones, the pad and focus; the Shift and GenericButton::MouseMove hooks feed them. */
#include "ftlpad.h"

#define DEBUG_ENV "FTLPAD_DEBUG"
#define CMD_ENV "FTLPAD_CMD"
#define HEARTBEAT_S 10.0

/* Small unplaced buttons the game parks in this corner. */
#define CORNER_BUTTON_MAX 40

#define CORNER_BUTTON_SIZE 24
#define MAX_DT 0.05

/* Used when no layout file is named. */
static const char default_layout[] =
    "[controls]\n"
    "a = space\n"
    "back = esc\n"
    "start = j\n"
    "l2 = mouse_right\n"
    "r2 = mouse_left\n"
    "right_analog = mouse_movement\n";

/* mov 0x10(%rdi),%eax; movzbl 0x49(%rdi),%r8d */
static const unsigned char button_move_prologue[] = {0x8b, 0x47, 0x10, 0x44, 0x0f, 0xb6, 0x47, 0x49};

/* push %rbp; push %rbx; mov %rdi,%rbx */
static const unsigned char onloop_prologue[] = {0x55, 0x53, 0x48, 0x89, 0xfb};

/* push %r15; mov %rdi,%r15 */
static const unsigned char mouse_render_prologue[] = {0x41, 0x57, 0x49, 0x89, 0xff};

/* sub $0x8,%rsp; mov $KEY_LEFTSHIFT,%edi */
static const unsigned char get_shift_prologue[] = {0x48, 0x83, 0xec, 0x08, 0xbf, 0x4b, 0x00, 0x00, 0x00};

static void (*onloop_orig)(void *);
void (*mouse_render_orig)(void *);
static int (*get_shift_orig)(void);
static void (*button_move_orig)(void *, int, int, bool);
bool debug;
void *app;

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
    if (cfg.focus_first)
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

static int get_shift_hook(void)
{
    return shift_count > 0 || get_shift_orig();
}

/* Whether a button is already in the set seen since the last frame. */
static bool button_seen(const void *self)
{
    for (int i = 0; i < nbuttons_seen; i++)
        if (button_objects_seen[i] == self)
            return true;
    return false;
}

static void button_move_hook(void *self, int x, int y, bool silent)
{
    buttons_tested = true;
    const char *gui = gui_ptr();
    bool crew_control_button = gui && (const char *)self >= gui + GUI_CREW_CONTROL
                               && (const char *)self < gui + GUI_CREW_CONTROL + CREW_CONTROL_SIZE;
    bool drawn = !*((const unsigned char *)self + BUTTON_RENDER_OFF);
    if (nbuttons_seen < MAX_BUTTONS && *((const unsigned char *)self + BUTTON_ACTIVE) && drawn && !crew_control_button
            && !button_seen(self)) {
        Rect r;
        memcpy(&r, (const char *)self + BUTTON_HITBOX, sizeof r);
        /* The armament bars keep their buttons' boxes relative to the bar. */
        const char *bars[] = {gui ? weapon_control() : NULL, gui ? drone_control() : NULL};
        const int bar_sizes[] = {WEAPON_CONTROL_SIZE, DRONE_CONTROL_SIZE};
        for (size_t i = 0; i < sizeof bars / sizeof *bars; i++)
            if (bars[i] && (const char *)self >= bars[i] && (const char *)self < bars[i] + bar_sizes[i]) {
                Point at = read_point(bars[i], ARMAMENT_LOCATION);
                r.x += at.x;
                r.y += at.y;
            }
        /* So does the star map, relative to its position. */
        const char *map = gui ? deref(gui, GUI_STAR_MAP) : NULL;
        if (map && (const char *)self >= map && (const char *)self < map + STARMAP_SIZE) {
            Point at = read_point(map, STARMAP_POSITION);
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
    cfg = cfg_defaults;
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
    onloop_orig = hook("OnLoop", game.onloop, onloop_hook, onloop_prologue, sizeof onloop_prologue);
    if (!onloop_orig) {
        logmsg("inactive");
        return;
    }
    mouse_render_orig = hook("MouseControl::OnRender", game.mouse_render, mouse_render_hook,
                             mouse_render_prologue, sizeof mouse_render_prologue);
    get_shift_orig = hook("GetShiftState", game.get_shift, get_shift_hook, get_shift_prologue,
                          sizeof get_shift_prologue);
    button_move_orig = hook("GenericButton::MouseMove", game.button_move, button_move_hook,
                            button_move_prologue, sizeof button_move_prologue);
    step_cap = redirect_step_cap();
    if (step_cap)
        step_cap_normal = *step_cap;
    logmsg("active: overlay %s, shift %s, buttons %s, step cap %s", mouse_render_orig ? "on" : "off",
           get_shift_orig ? "on" : "off", button_move_orig ? "on" : "off", step_cap ? "on" : "off");

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
