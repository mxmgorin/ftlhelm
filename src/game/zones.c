/* The zones the D-pad walks, rebuilt each frame for the screen on top. */
#include "ftlpad.h"

#define DOOR_HALF 10

/* Half the side of the square a beacon counts as. */
#define BEACON_HALF 12

/* Buttons the game hit-tested since the last frame, and the last full set. */
Rect buttons_seen[MAX_BUTTONS], buttons[MAX_BUTTONS];

const void *button_objects_seen[MAX_BUTTONS], *button_objects[MAX_BUTTONS];
int nbuttons_seen, nbuttons;

/* Whether the game hit-tested any button since the last frame, so the seen set is whole. */
bool buttons_tested;

struct target targets[MAX_TARGETS];
int ntargets;

/* Bounding box centre of a ship's rooms on screen. */
bool ship_center(const void *ship, Point origin, float *cx, float *cy)
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

/* With the map open only its own buttons count, a beacon click jumps by itself; with
 * another screen or the menu open the HUD's top and bottom rows drop out (the menu's
 * buttons live on the heap, so they cannot be told apart by address). */
static void add_button_targets(const void *gui, bool screen_open, bool map_open)
{
    const void *map = map_open ? deref(gui, GUI_STAR_MAP) : NULL;
    for (int i = 0; i < nbuttons && ntargets < MAX_TARGETS; i++) {
        if (map_open && !inside(button_objects[i], map, STARMAP_SIZE))
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
    } screens[] = {{GUI_EQUIP_SCREEN, EQUIP_BOXES}, {GUI_CREW_SCREEN, CREWSCREEN_BOXES}};
    for (size_t k = 0; k < sizeof screens / sizeof *screens; k++) {
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

/* Rebuilds the zones for the current screen: an event's choices alone while it is open
 * (it sits over any screen but the game menu), beacons or sector boxes with the map open,
 * otherwise rooms (the enemy's only while aiming, only
 * doors in precision mode); the buttons the game hit-tests come on top. */
void build_targets(void)
{
    ntargets = 0;
    if (buttons_tested) {
        memcpy(buttons, buttons_seen, (size_t)nbuttons_seen * sizeof *buttons);
        memcpy(button_objects, button_objects_seen, (size_t)nbuttons_seen * sizeof *button_objects);
        nbuttons = nbuttons_seen;
        nbuttons_seen = 0;
        buttons_tested = false;
    }
    void *gui = gui_ptr();
    if (!gui)
        return;
    void *player = ship_of(player_complete());
    size_t modal_size = 0;
    bool modal_open = modal_window(gui, &modal_size) != NULL;
    if (!modal_open && add_choice_targets(gui))
        return;
    bool screen_open = open_tabbed() != NULL || modal_open;
    bool map_open = window_open(deref(gui, GUI_STAR_MAP));
    if (player && !screen_open && !add_map_targets(gui)) {
        void *enemy = ship_of(enemy_complete());
        Point origin = read_point(gui, GUI_SHIP_POSITION);
        if (aiming()) {
            if (enemy)
                add_rooms(enemy, enemy_origin(gui), true);
            add_rooms(player, origin, false);
        } else if (precision && !crew_selected()) {
            add_doors(player, origin);
        } else {
            add_rooms(player, origin, false);
            if (enemy)
                add_rooms(enemy, enemy_origin(gui), true);
        }
    }
    if (screen_open)
        add_slot_targets(gui);
    if (!precision || aiming() || crew_selected())
        add_button_targets(gui, screen_open, map_open);
}

static bool rect_contains(const Rect *r, int x, int y)
{
    return x >= r->x && x < r->x + r->w && y >= r->y && y < r->y + r->h;
}

int target_under_cursor(void)
{
    for (int i = 0; i < ntargets; i++)
        if (rect_contains(&targets[i].rect, cursor.last_x, cursor.last_y))
            return i;
    return -1;
}
