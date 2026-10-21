/* What the layer does to the game: keys, Shift, clicks and queued clicks, room
 * doors, crew and weapon cycling, tabs, and the fast speed. */
#include "ftlpad.h"

#define SPEED_LEVEL_NORMAL 0
#define STEP_CAP_FAST 2.0f

/* Queued clicks: capacity, frames between phases, phases per click. */
#define MAX_QUEUED_CLICKS 16

#define QUEUED_CLICK_FRAMES 4
#define QUEUED_CLICK_PHASES 3

/* The step cap CFPS::OnLoop reads once redirected here, and the game's own value. */
float *step_cap;

float step_cap_normal;
int shift_count;
static struct queued_click queued_clicks[MAX_QUEUED_CLICKS];
int nqueued_clicks, queued_phase, queued_wait;

void tap_key(int key, bool shift)
{
    if (shift)
        set_shift(true);
    game.key_down(app, key);
    game.key_up(app, key);
    if (shift)
        set_shift(false);
}

void set_shift(bool on)
{
    shift_count += on ? 1 : -1;
    *((bool *)app + CAPP_SHIFT_HELD) = shift_count > 0;
    if (on && shift_count == 1)
        game.key_down(app, KEY_LSHIFT);
    else if (!on && shift_count == 0)
        game.key_up(app, KEY_LSHIFT);
}

void click(enum act_kind kind, bool down)
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
void center_swap(void)
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

/* Moves the cursor to the room of the first selected crew member. */
void cursor_to_selected_crew(void)
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

static void click_at(float gx, float gy)
{
    if (nqueued_clicks < MAX_QUEUED_CLICKS) {
        queued_clicks[nqueued_clicks].x = gx;
        queued_clicks[nqueued_clicks].y = gy;
        nqueued_clicks++;
    }
}

/* Runs one step of the queued clicks; the cursor returns once the queue is empty. */
void run_queued_clicks(void)
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
    queued_phase = (queued_phase + 1) % QUEUED_CLICK_PHASES;
    cursor.last_set = *game.mouse_position;
}

/* Toggles the doors of the player room under the cursor: closes them all while any is
 * open, else opens them all. Each door is clicked, as the game toggles them. */
void room_doors(void)
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
void select_next_crew(void)
{
    int count = 0;
    while (count < CREW_HOTKEYS && crew_at(count))
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
void arm_next(void)
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

void tab_step(int dir)
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

/* Runs the game at the configured speed level, or back at the normal one. */
void set_fast(bool on)
{
    int level = on ? cfg.speed_level : SPEED_LEVEL_NORMAL;
    memcpy((char *)game.fps_control + FPS_LEVEL, &level, sizeof level);
    if (step_cap)
        *step_cap = on ? step_cap_normal * STEP_CAP_FAST : step_cap_normal;
    fast_forward = on;
}
