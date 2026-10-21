/* Pad reading through the SIL joystick API: face labels, sticks and triggers as
 * digital inputs, flicks, and the per-frame dispatch. */
#include "ftlpad.h"

/* PortMaster's per-device SDL mapping; its button indices match SIL's for pads whose
 * buttons all sit in the gamepad key range. */
#define SDL_MAP_ENV "SDL_GAMECONTROLLERCONFIG"

#define SDL_MAP_UNSET -1

/* Face buttons the mapping names: a, b, x, y. */
#define FACE_COUNT 4

#define STICK_KEY_ON 0.5f
#define STICK_KEY_OFF 0.3f

/* SIL reports an analog trigger as a stick axis resting at -1. */
#define TRIGGER_ON 0.0f

#define EXIT_HOLD_S 0.6

/* Frames between synthetic mouse moves that refresh the button zones. */
#define BUTTON_REFRESH_FRAMES 6

#define LEFT_STICK 0
#define RIGHT_STICK 1
#define TRIGGER_STICK 2

/* SIL button index per face label from the SDL mapping, or SDL_MAP_UNSET. */
static int sdl_face[FACE_COUNT] = {SDL_MAP_UNSET, SDL_MAP_UNSET, SDL_MAP_UNSET, SDL_MAP_UNSET};

static const char *const sdl_face_names[FACE_COUNT] = {"a", "b", "x", "y"};
static const enum input sdl_face_inputs[FACE_COUNT] = {IN_FACE_DOWN, IN_FACE_RIGHT, IN_FACE_LEFT, IN_FACE_UP};

/* Whether the game was already aiming when the wheel opened; only a new aim closes it. */
bool wheel_opened_aiming;

static bool prev_aiming;
static const void *prev_crew;
static double flick_last[2];
static bool flick_armed[2] = {true, true};
static bool held[IN_COUNT];
static double chord_since;
static bool exit_requested;
int pad_index = -1;

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

/* Reads a:bN .. y:bN out of the SDL mapping string. */
void load_sdl_map(void)
{
    const char *map = getenv(SDL_MAP_ENV);
    if (!map)
        return;
    for (int i = 0; i < FACE_COUNT; i++) {
        char token[NAME_LEN];
        snprintf(token, sizeof token, ",%s:b", sdl_face_names[i]);
        const char *at = strstr(map, token);
        if (at)
            sdl_face[i] = atoi(at + strlen(token));
    }
    logmsg("sdl map: a=%d b=%d x=%d y=%d", sdl_face[0], sdl_face[1], sdl_face[2], sdl_face[3]);
}

void buzz(void)
{
    if (cfg.rumble && pad_index >= 0)
        joy.rumble(pad_index, cfg.rumble_strength, cfg.rumble_strength, cfg.rumble_time);
}

int active_pad(void)
{
    int n = joy.count();
    for (int i = 0; i < n; i++)
        if (joy.connected(i))
            return i;
    return -1;
}

void read_pad(int dev, struct pad *p)
{
    for (int b = 0; b < SIL_BUTTONS; b++) {
        int idx = joy.button_mapping(dev, b);
        p->down[b] = idx >= 0 && joy.button_state(dev, idx);
    }
    bool mapped = cfg.nintendo < 0;
    for (int i = 0; i < FACE_COUNT; i++)
        mapped &= sdl_face[i] >= 0;
    if (mapped) {
        for (int i = 0; i < FACE_COUNT; i++)
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

/* Makes the game hit-test its buttons at the cursor: a mouse move in place, in window
 * pixels of the current transform. */
static void refresh_buttons(void)
{
    to_window(cursor.x, cursor.y, &cursor.last_wx, &cursor.last_wy);
    game.mouse_move(app, cursor.last_wx, cursor.last_wy, 0, 0, false, false, false);
}

/* Applies one frame of pad state: pointer sources move the cursor or flick the magnet,
 * sticks point an open wheel, everything else goes through the layout. */
void apply_pad(struct pad *p, double t, float dt)
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
        if (--wheel_refresh_wait <= 0)
            refresh_wheel();
        wheel_point(p->stick[0][0], p->stick[0][1], p->stick[1][0], p->stick[1][1]);
        if ((aiming() && !wheel_opened_aiming) || !wheel_screen())
            wheel_close();
        if (!aiming())
            wheel_opened_aiming = false;
    }
    bool snapping = cfg.snap_dpad && depth == 0 && ntargets > 0 && mode[SLOT_DPAD] == SLOT_KEYS;
    if (++frame % BUTTON_REFRESH_FRAMES == 0 && cursor.synced && !cursor.l_held && !cursor.r_held
            && nqueued_clicks == 0)
        refresh_buttons();

    settle_pending(t);
    for (int i = 0; i < IN_COUNT; i++) {
        if (p->down[i] == held[i])
            continue;
        held[i] = p->down[i];
        /* Any button closes the help; that press does nothing else. */
        if (help_shown && p->down[i] && i < SIL_BUTTONS) {
            help_shown = false;
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

    /* pause_ff held past the threshold: unpause and run fast. */
    if (pause_ff_since > 0 && !fast_forward && t - pause_ff_since >= cfg.pause_ff_hold) {
        if (game_paused())
            tap_key(KEY_SPACE, false);
        set_fast(true);
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
        if (cfg.crew_follow)
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
