/* The FIFO test channel: dumps of zones and state, and commands that drive a run. */
#include "ftlpad.h"

static const char *const target_kind_names[] = {"player room", "enemy room", "beacon", "sector", "choice", "button", "door", "slot"};
int cmd_fd = -1;

static void dump_targets(void)
{
    logmsg("dump: aiming=%d targets=%d cursor=%d,%d under=%d", aiming(), ntargets,
           cursor.last_x, cursor.last_y, target_under_cursor());
    void *gui = gui_ptr();
    void *player = ship_of(player_complete());
    void *enemy = ship_of(enemy_complete());
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
    for (int level = 0; level <= depth && used < sizeof line; level++)
        used += (size_t)snprintf(line + used, sizeof line - used, " %s", state_at(level)->name);
    static const char *const mode_names[] = {"keys", "mouse", "flick"};
    logmsg("state:%s; wheel %s sel %d; shift %d; dpad %s, left %s, right %s; aiming %d crew %d paused %d precision %d",
           line, wheel_open >= 0 ? wheels[wheel_open].name : "-", wheel_sel, shift_count,
           mode_names[slot_mode(SLOT_DPAD)], mode_names[slot_mode(SLOT_LSTICK)],
           mode_names[slot_mode(SLOT_RSTICK)], aiming(), crew_selected(), game_paused(), precision);
    int level;
    float step;
    memcpy(&level, (char *)game.fps_control + FPS_LEVEL, sizeof level);
    memcpy(&step, (char *)game.fps_control + FPS_STEP, sizeof step);
    logmsg("speed: level %d, step %.3f, cap %.2f", level, step, step_cap ? *step_cap : -1.0f);
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
    } else if (strncmp(line, "badge", 5) == 0 && gui_ptr()) {
        char text[LABEL_LEN * 3];
        badge_text(text, sizeof text);
        logmsg("badge: '%s' width %d", text, text[0] ? text_width(WHEEL_FONT, text) : 0);
    } else {
        logmsg("unknown command: %s", line);
    }
}

void poll_commands(void)
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
    if (used == sizeof buf - 1) {
        logmsg("command too long, dropped");
        used = 0;
    }
    memmove(buf, start, used + 1);
}
