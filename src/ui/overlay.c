/* What the layer draws over the game each frame: the open wheel, the help table and
 * the mode badge. */
#include "ftlpad.h"

#define HELP_FONT WHEEL_FONT
#define HELP_LINE 30.0f
#define HELP_W 760.0f
#define HELP_PAD 12.0f
#define HELP_COL_TAP 130.0f
#define HELP_COL_HOLD 430.0f
#define HELP_RULE_H 1.0f
#define BADGE_H 30.0f
#define BADGE_PAD 14.0f
#define BADGE_SEPARATOR " / "
#define BADGE_MARGIN 8.0f

/* Text baseline sits this far below a plate's top edge. */
#define WHEEL_TEXT_DY 5.0f

static const char *const builtin_titles[BI_COUNT] = {
    "Pause, hold: fast", "Cancel", "Click", "Next weapon", "Next crew / tab",
    "Repeat ability", "Doors mode", "Other ship", "Previous tab", "Next tab", "Help",
    "Room doors", "Pause", "Fast forward", "Cursor / zones",
};

static const char *const input_titles[IN_COUNT] = {
    "Guide", "Start", "Select", "Y", "X", "B", "A", "L1", "R1", "L2", "R2", "L3", "R3",
    "Left", "Up", "Right", "Down", "LS left", "LS up", "LS right", "LS down",
    "RS left", "RS up", "RS right", "RS down",
};

bool help_shown;

static const struct key_title key_titles[] = {
    {KEY_ESCAPE, "Menu"}, {KEY_SPACE, "Pause"}, {'j', "Jump"}, {'u', "Upgrades"},
    {'v', "Autofire"}, {'z', "Open doors"}, {'x', "Close doors"}, {'q', "All crew"},
    {KEY_ENTER, "To stations"}, {'/', "Save stations"},
};

static void draw_help(void);

void mouse_render_hook(void *self)
{
    if (wheel_open >= 0)
        draw_wheel();
    if (help_shown && cursor.synced)
        draw_help();
    if (cursor.synced && cfg.overlay && gui_ptr()) {
        char text[LABEL_LEN * 3];
        badge_text(text, sizeof text);
        if (text[0]) {
            float w = (float)text_width(WHEEL_FONT, text) + 2 * BADGE_PAD;
            float x = GAME_W - BADGE_MARGIN - w, y = BADGE_MARGIN;
            draw_panel(x, y, w, BADGE_H, plate_color, plate_edge_color);
            draw_text_center(WHEEL_FONT, x + w / 2, y + WHEEL_TEXT_DY, text);
        }
    }
    /* The game's pointer would cover the wheel's middle. */
    if (wheel_open < 0)
        mouse_render_orig(self);
}

static void describe_binding(const struct binding *b, char *out, size_t len)
{
    switch (b->kind) {
    case ACT_KEY: {
        const char *title = NULL, *name = NULL;
        for (size_t i = 0; i < sizeof key_titles / sizeof *key_titles && !title; i++)
            if (key_titles[i].code == b->key)
                title = key_titles[i].title;
        name = key_name(b->key);
        if (title)
            snprintf(out, len, "%s", title);
        else if (name)
            snprintf(out, len, "%s", name);
        else if (b->key >= KEY_F1 && b->key < KEY_F1 + KEY_F_COUNT)
            snprintf(out, len, "f%d", b->key - KEY_F1 + 1);
        else
            snprintf(out, len, "%c", b->key);
        break;
    }
    case ACT_LCLICK: snprintf(out, len, "click"); break;
    case ACT_RCLICK: snprintf(out, len, "right click"); break;
    case ACT_STATE: snprintf(out, len, "%s page", states[b->state].name); break;
    case ACT_WHEEL:
        snprintf(out, len, "%s wheel", wheels[b->state].name);
        out[0] = (char)toupper((unsigned char)out[0]);
        break;
    case ACT_BUILTIN: snprintf(out, len, "%s", builtin_titles[b->state]); break;
    default: out[0] = '\0';
    }
}

/* Whether a binding is something held rather than tapped. */
static bool is_hold_action(const struct binding *b)
{
    return b->kind == ACT_WHEEL || b->kind == ACT_STATE
           || (b->kind == ACT_BUILTIN && (b->state == BI_PRECISION || b->state == BI_FAST_FORWARD));
}

/* Draws the base state's bindings as a table: button, tap, hold. */
static void draw_help(void)
{
    const struct state *base = &states[0];
    int lines = 1;
    for (int in = 0; in < SIL_BUTTONS; in++)
        lines += base->bound[in];
    float h = HELP_LINE * lines + 2 * HELP_PAD;
    float x = (GAME_W - HELP_W) / 2, y = (GAME_H - h) / 2;
    draw_panel(x, y, HELP_W, h, plate_color, plate_edge_color);
    float line_y = y + HELP_PAD;
    draw_text_left(HELP_FONT, x + HELP_PAD, line_y + WHEEL_TEXT_DY, "BUTTON");
    draw_text_left(HELP_FONT, x + HELP_COL_TAP, line_y + WHEEL_TEXT_DY, "TAP");
    draw_text_left(HELP_FONT, x + HELP_COL_HOLD, line_y + WHEEL_TEXT_DY, "HOLD");
    line_y += HELP_LINE;
    game.draw_rect(x + HELP_PAD, line_y - 2, HELP_W - 2 * HELP_PAD, HELP_RULE_H, plate_edge_color);
    for (int in = 0; in < SIL_BUTTONS; in++) {
        if (!base->bound[in])
            continue;
        char hold[LABEL_LEN * 2] = "", tap[LABEL_LEN * 2] = "";
        const struct binding *b = &base->bind[in];
        if (b->tap) {
            describe_binding(b, hold, sizeof hold);
            describe_binding(b->tap, tap, sizeof tap);
        } else {
            describe_binding(b, is_hold_action(b) ? hold : tap, sizeof hold);
        }
        draw_text_left(HELP_FONT, x + HELP_PAD, line_y + WHEEL_TEXT_DY, input_titles[in]);
        draw_text_left(HELP_FONT, x + HELP_COL_TAP, line_y + WHEEL_TEXT_DY, tap);
        draw_text_left(HELP_FONT, x + HELP_COL_HOLD, line_y + WHEEL_TEXT_DY, hold);
        line_y += HELP_LINE;
    }
}

/* The mode badge text, empty in the plain state: the weapon waiting for a target and the
 * layer's own modes, which the game does not show. */
void badge_text(char *out, size_t len)
{
    char main[LABEL_LEN * 2] = "";
    if (wheel_open >= 0) {
        out[0] = '\0';
        return;
    }
    if (read_int(game.mouse_position, MOUSE_AIMING)) {
        const char *control = weapon_control();
        const char *armed = control ? deref(control, WEAPONCTRL_ARMED) : NULL;
        const char *bp = armed ? deref(armed, WEAPON_BLUEPRINT) : NULL;
        char name[LABEL_LEN] = "";
        if (bp)
            display_text(bp + BLUEPRINT_SHORT_TITLE, name, sizeof name);
        snprintf(main, sizeof main, name[0] ? "Target: %s" : "Target", name);
    }
    const char *parts[] = {precision ? "Doors" : NULL, fast_forward ? "Fast" : NULL, dpad_mouse ? "Mouse" : NULL};
    size_t used = (size_t)snprintf(out, len, "%s", main);
    for (size_t i = 0; i < sizeof parts / sizeof *parts; i++)
        if (parts[i] && used < len)
            used += (size_t)snprintf(out + used, len - used, "%s%s", used ? BADGE_SEPARATOR : "", parts[i]);
    upper_ascii(out);
}
