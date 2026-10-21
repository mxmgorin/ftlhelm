/* Readers over the game's objects: memory helpers, GUI windows, crew, systems,
 * weapons and drones, and their display names. */
#include "ftlpad.h"

void *deref(const void *base, size_t off)
{
    void *p;
    memcpy(&p, (const char *)base + off, sizeof p);
    return p;
}

int read_int(const void *base, size_t off)
{
    int v;
    memcpy(&v, (const char *)base + off, sizeof v);
    return v;
}

float read_float(const void *base, size_t off)
{
    float v;
    memcpy(&v, (const char *)base + off, sizeof v);
    return v;
}

Point read_point(const void *base, size_t off)
{
    Point p;
    memcpy(&p, (const char *)base + off, sizeof p);
    return p;
}

size_t vector_count(const void *vec, size_t elem)
{
    char *b = deref(vec, VECTOR_BEGIN), *e = deref(vec, VECTOR_END);
    return b && e >= b ? (size_t)(e - b) / elem : 0;
}

void *vector_at(const void *vec, size_t elem, size_t i)
{
    return deref(deref(vec, VECTOR_BEGIN), i * elem);
}

/* The Ship object of a CompleteShip, or NULL. */
void *ship_of(const void *complete)
{
    void *manager = complete ? deref(complete, COMPLETE_SHIP_MANAGER) : NULL;
    return manager ? (char *)manager + SHIPMANAGER_SHIP : NULL;
}

void *gui_ptr(void)
{
    return app ? deref(app, CAPP_GUI) : NULL;
}

void *player_complete(void)
{
    void *gui = gui_ptr();
    return gui ? deref(gui, GUI_SHIP_COMPLETE) : NULL;
}

void *enemy_complete(void)
{
    void *gui = gui_ptr();
    return gui ? deref((const char *)gui + GUI_COMBAT_CONTROL, COMBAT_CURRENT_TARGET) : NULL;
}

void *player_manager(void)
{
    void *complete = player_complete();
    return complete ? deref(complete, COMPLETE_SHIP_MANAGER) : NULL;
}

/* CrewControl's vector of selected crew, or NULL without a GUI. */
static const void *selected_crew(void)
{
    const char *gui = gui_ptr();
    return gui ? gui + GUI_CREW_CONTROL + CREW_SELECTED : NULL;
}

static size_t selected_count(void)
{
    const void *selected = selected_crew();
    return selected ? vector_count(selected, sizeof(void *)) : 0;
}

bool game_paused(void)
{
    void *gui = gui_ptr();
    return gui && *((const unsigned char *)gui + GUI_PAUSED);
}

bool window_open(const void *window)
{
    return window && *((const unsigned char *)window + FOCUS_OPEN);
}

/* The open tabbed screen (ship or store), or NULL. */
void *open_tabbed(void)
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

bool crew_selected(void)
{
    return selected_count() > 0;
}

/* The weapon or drone object in slot i of an armament control, or NULL for an empty slot. */
void *armament_item(const void *control, int i)
{
    const void *boxes = (const char *)control + ARMAMENT_BOXES;
    if ((size_t)i >= vector_count(boxes, sizeof(void *)))
        return NULL;
    void *box = vector_at(boxes, sizeof(void *), (size_t)i);
    return box ? deref(box, ARMAMENTBOX_ITEM) : NULL;
}

const char *weapon_control(void)
{
    const char *gui = gui_ptr();
    return gui ? gui + GUI_COMBAT_CONTROL + COMBAT_WEAPON_CONTROL : NULL;
}

const char *drone_control(void)
{
    const char *gui = gui_ptr();
    return gui ? gui + GUI_COMBAT_CONTROL + COMBAT_DRONE_CONTROL : NULL;
}

bool aiming(void)
{
    const void *mouse = game.mouse_position;
    return read_int(mouse, MOUSE_AIMING) || read_int(mouse, MOUSE_TELEPORTING)
           || read_int(mouse, MOUSE_MIND) || read_int(mouse, MOUSE_HACKING);
}

Point enemy_origin(const void *gui)
{
    const char *combat = (const char *)gui + GUI_COMBAT_CONTROL;
    Point origin = read_point(combat, COMBAT_POSITION);
    Point target_pos = read_point(combat, COMBAT_TARGET_POSITION);
    origin.x += target_pos.x;
    origin.y += target_pos.y;
    return origin;
}

bool inside(const void *p, const void *base, size_t size)
{
    return base && (const char *)p >= (const char *)base && (const char *)p < (const char *)base + size;
}

/* The game menu or the options screen when one is open: only its buttons count. */
const char *modal_window(const char *gui, size_t *size)
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

/* The crew member a plate key selects, by F-key index into the ship's crew list. */
void *crew_at(int index)
{
    void *manager = player_manager();
    if (!manager)
        return NULL;
    const void *crew = (const char *)manager + SHIPMANAGER_CREW;
    if ((size_t)index >= vector_count(crew, sizeof(void *)))
        return NULL;
    return vector_at(crew, sizeof(void *), (size_t)index);
}

/* Whether a crew member is in the current selection. */
bool crew_is_selected(const void *member)
{
    const void *selected = selected_crew();
    size_t n = selected_count();
    for (size_t i = 0; i < n; i++)
        if (vector_at(selected, sizeof(void *), i) == member)
            return true;
    return false;
}

const char *game_string(const void *str)
{
    const char *chars = deref(str, 0);
    return chars ? chars : "";
}

/* Copies a TextString's display text: literal ones as they are, keys through the
 * game's text library in its current language. */
void display_text(const void *text, char *out, size_t len)
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
int system_key(int type)
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

const char *system_name(int type)
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
int ability_system(int key)
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

/* Lockdown needs a living crystal crew member. */
bool has_crystal_crew(void)
{
    for (int i = 0;; i++) {
        const char *member = crew_at(i);
        if (!member)
            return false;
        if (!*(member + CREW_DEAD) && strcmp(game_string(member + CREW_BLUEPRINT + CREWBP_RACE), CRYSTAL_RACE) == 0)
            return true;
    }
}

bool has_system(int type)
{
    void *manager = player_manager();
    return manager && game.system_room(manager, type) >= 0;
}

/* The first crew member of the current selection, or NULL. */
const void *first_selected_crew(void)
{
    return crew_selected() ? vector_at(selected_crew(), sizeof(void *), 0) : NULL;
}
