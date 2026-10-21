/* Wheels: layout around the cursor, plates with icons and power pips, picking,
 * filling from the game, and acting on the pick. */
#include "ftlpad.h"

/* Weapon art sits right of its texture's centre by this many texture pixels. */
#define WEAPON_ICON_SHIFT 3.0f

/* CrewAnimation::RenderIcon draws the face small, centred on the origin. */
#define CREW_ICON_SCALE 2.0f

/* Frames between refills of an open wheel from the game. */
#define WHEEL_REFRESH_FRAMES 6

#define WHEEL_ON 0.5f

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

static const GL_Color plate_selected_color = {0.42f, 0.38f, 0.1f, 0.95f};
static const GL_Color plate_selected_edge_color = {0.96f, 0.84f, 0.28f, 1.0f};

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
static const GL_Color pip_on_color = {0.47f, 0.95f, 0.47f, 1.0f};
struct wheel wheels[MAX_WHEELS];

/* The open wheel and the plate the sticks point at. */
int wheel_open = -1;

int wheel_sel = -1;

/* Frames until the open wheel refills from the game. */
int wheel_refresh_wait;

/* Whether the open wheel paused the game, so its close unpauses. */
static bool wheel_paused;

static const struct armament armaments[] = {
    {'1', WEAPON_SLOTS, weapon_control, WEAPON_BLUEPRINT, WEAPON_POWERED, WEAPON_POWER, WEAPONBP_ICON,
     WEAPON_ICON_PREFIX, WEAPON_ICON_FALLBACK, WEAPON_ICON_SHIFT},
    {'1' + WEAPON_SLOTS, DRONE_SLOTS, drone_control, DRONE_BLUEPRINT, DRONE_POWERED, DRONE_POWER, DRONEBP_ICON,
     DRONE_ICON_PREFIX, DRONE_ICON_FALLBACK, 0.0f},
};

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

void draw_wheel(void)
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
void wheel_point(float lx, float ly, float rx, float ry)
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
void wheel_step(int dir)
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

/* Sets a crew plate's corner icon and badge from the system in the member's room, or
 * the badge alone when the member is aboard the other ship. */
static void crew_location(struct plate *pl, const void *member)
{
    if (read_int(member, CREW_SHIP) != PLAYER_SHIP_ID) {
        strncpy(pl->badge, "Enemy ship", LABEL_LEN - 1);
        return;
    }
    void *manager = player_manager();
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
void refresh_wheel(void)
{
    wheel_refresh_wait = WHEEL_REFRESH_FRAMES;
    struct wheel *w = &wheels[wheel_open];
    if (w->kind == WK_CREW || w->kind == WK_SYSTEMS)
        memset(w->plates, 0, sizeof w->plates);
    if (w->kind == WK_CREW) {
        int slot = 0;
        for (int i = 0; i < CREW_HOTKEYS && slot < WHEEL_SLOTS; i++) {
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
                pl->pips = read_int(sys, SYSTEM_POWER_MAX);
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
        const struct armament *arm = NULL;
        for (size_t a = 0; a < sizeof armaments / sizeof *armaments && w->kind == WK_POWER; a++)
            if (key >= armaments[a].first_key && key < armaments[a].first_key + armaments[a].slots)
                arm = &armaments[a];
        if (arm) {
            const char *control = arm->control();
            const char *item = control ? armament_item(control, key - arm->first_key) : NULL;
            const char *bp = item ? deref(item, arm->blueprint) : NULL;
            pl->available = bp != NULL;
            pl->empty = !bp;
            pl->powered = bp && *(item + arm->powered);
            if (bp) {
                display_text(bp + BLUEPRINT_SHORT_TITLE, pl->label, LABEL_LEN);
                armament_plate(pl, bp + arm->icon, arm->icon_prefix, arm->icon_fallback, arm->icon_shift,
                               read_int(item, arm->power));
            } else {
                strncpy(pl->label, "Empty", LABEL_LEN - 1);
            }
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

void wheel_close(void)
{
    if (wheel_open < 0)
        return;
    if (wheel_paused && game_paused())
        tap_key(KEY_SPACE, false);
    wheel_paused = false;
    wheel_open = -1;
    wheel_sel = -1;
}

void wheel_show(int index, enum input by)
{
    if (wheel_open >= 0) {
        if (wheel_open == index)
            wheel_close();
        return;
    }
    if (!wheel_screen())
        return;
    wheel_open = index;
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

/* Performs the selected plate: keys and clicks run as taps; an action wheel closes after,
 * the others refill. The abilities key is remembered for repeat_ability. */
void wheel_act(void)
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
        builtin((enum builtin)b->state, NO_INPUT, true, now_s());
        builtin((enum builtin)b->state, NO_INPUT, false, now_s());
    }
    if (debug)
        logmsg("wheel %s: %s", w->name, w->plates[wheel_sel].label);
    if (w->kind == WK_ACTION)
        wheel_close();
    else if (wheel_open >= 0)
        refresh_wheel();
}

/* Adds power to the selected item of a power or systems wheel, or takes it away for a
 * negative dir; on the crew wheel, adds the member to the selection or drops them. */
void wheel_power(int dir)
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
        refresh_wheel();
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
    refresh_wheel();
}
