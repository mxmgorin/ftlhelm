/* The virtual cursor: game to window coordinates, stick movement, D-pad focus
 * moves between zones, and the first item a screen opens on. */
#include "ftlpad.h"

/* Frames a newly opened screen gets to show its items before the focus gives up; a
 * dialog's choices wait for the jump animation. */
#define FIRST_FOCUS_FRAMES 300

struct cursor_state cursor;

/* Maps a logical position to the window pixels the mouse handlers take. */
void to_window(float gx, float gy, int *wx, int *wy)
{
    float mx = read_float(app, CAPP_MOUSE_MODIFIER_X), my = read_float(app, CAPP_MOUSE_MODIFIER_Y);
    if (mx <= 0)
        mx = 1;
    if (my <= 0)
        my = 1;
    *wx = (int)lroundf((gx + (float)read_int(app, CAPP_MODIFIER_X)) / mx) + read_int(app, CAPP_X_BAR);
    *wy = (int)lroundf((gy + (float)read_int(app, CAPP_MODIFIER_Y)) / my) + read_int(app, CAPP_Y_BAR);
}

/* Adopts the game's cursor when something else moved it. */
void sync_cursor(void)
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
void send_cursor(void)
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
void move_cursor(float rx, float ry, float dt)
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

/* Moves the cursor to the target nearest along a direction: inside the narrow cone first,
 * then the wide one, scored by distance grown with the angle off the axis. Both are
 * measured to a target's nearest point, or its centre when the cursor is inside it. */
void focus_move(float dx, float dy)
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
    const float cones[] = {cfg.focus_cone_narrow, cfg.focus_cone_wide};
    for (size_t pass = 0; pass < sizeof cones / sizeof *cones && best < 0; pass++) {
        float cos_cone = cosf(cones[pass] * (float)M_PI / 180.0f);
        for (int i = 0; i < ntargets; i++) {
            if (i == from)
                continue;
            const Rect *r = &targets[i].rect;
            float vx = clampf(cx, (float)r->x, (float)(r->x + r->w)) - cx;
            float vy = clampf(cy, (float)r->y, (float)(r->y + r->h)) - cy;
            if (vx == 0 && vy == 0) {
                vx = r->x + r->w / 2.0f - cx;
                vy = r->y + r->h / 2.0f - cy;
            }
            float dist = hypotf(vx, vy);
            if (dist <= 0)
                continue;
            float cos_a = (vx * dx + vy * dy) / dist;
            if (cos_a < cos_cone)
                continue;
            float score = dist * (1.0f + cfg.focus_side_weight * (1.0f - cos_a));
            if (best < 0 || score < best_score) {
                best = i;
                best_score = score;
            }
        }
    }
    if (debug)
        logmsg("focus %.0f,%.0f: from %d to %d", dx, dy, from, best);
    if (best < 0)
        return;
    cursor.x = targets[best].rect.x + targets[best].rect.w / 2.0f;
    cursor.y = targets[best].rect.y + targets[best].rect.h / 2.0f;
    send_cursor();
    buzz();
}

/* The menu or dialog on screen, if it is one that opens on its first item. */
static enum first_screen first_screen(void)
{
    const char *gui = gui_ptr();
    if (gui && window_open(gui + GUI_MENU_BOX))
        return FS_GAME_MENU;
    if (gui && *((const unsigned char *)gui + GUI_CHOICE_BOX + FOCUS_OPEN))
        return FS_CHOICE;
    const char *map = gui ? deref(gui, GUI_STAR_MAP) : NULL;
    if (map && window_open(map) && !*(const unsigned char *)(map + STARMAP_CHOOSING_SECTOR))
        return FS_MAP;
    if (*((const unsigned char *)app + CAPP_MAIN_MENU_OPEN))
        return FS_MAIN_MENU;
    return FS_NONE;
}

/* The first item of a screen among the zones: a dialog's first choice, the map's beacon
 * under the ship, else the topmost button, leftmost on a tie; -1 while there is none. */
static int first_item(enum first_screen screen)
{
    const char *map = screen == FS_MAP ? deref(gui_ptr(), GUI_STAR_MAP) : NULL;
    const void *current = map ? deref(map, STARMAP_CURRENT_LOC) : NULL;
    int best = -1;
    for (int i = 0; i < ntargets; i++) {
        const struct target *t = &targets[i];
        if (screen == FS_MAP) {
            if (t->kind == T_BEACON && current
                    && vector_at(map + STARMAP_LOCATIONS, sizeof(void *), (size_t)t->id) == current)
                return i;
        } else if (screen == FS_CHOICE) {
            if (t->kind == T_CHOICE && t->id == 0)
                return i;
        } else if (t->kind == T_BUTTON && (best < 0 || t->rect.y < targets[best].rect.y
                       || (t->rect.y == targets[best].rect.y && t->rect.x < targets[best].rect.x))) {
            best = i;
        }
    }
    return best;
}

/* Puts the cursor on the first item of a menu, dialog or the map as it opens, unless the
 * cursor moves first. A dialog that follows another counts as new when its text changes or its
 * open timer restarts. */
void focus_first_on_open(void)
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
