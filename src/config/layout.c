/* Layout parsing: gptokeyb2-style [controls] states, HOLD | TAP bindings, builtins,
 * and [wheel:NAME] sections with plate labels and icons. */
#include "ftlpad.h"

/* Layout sections: the base state, a pushed state and a wheel, the last two by name. */
#define BASE_SECTION "controls"

static const char *const slot_ini_names[SLOT_COUNT] = {"dpad", "left_analog", "right_analog"};

/* Input names as the ini spells them; face buttons by label, A being FACE_DOWN. */
const char *const input_ini_names[IN_COUNT] = {
    "guide", "start", "back", "y", "x", "b", "a", "l1", "r1", "l2", "r2", "l3", "r3",
    "left", "up", "right", "down",
    "left_analog_left", "left_analog_up", "left_analog_right", "left_analog_down",
    "right_analog_left", "right_analog_up", "right_analog_right", "right_analog_down",
};

const char *const builtin_names[BI_COUNT] = {
    "pause_ff", "cancel", "context_click", "arm_next", "aim_crew", "repeat_ability",
    "precision", "center_swap", "prev_tab", "next_tab", "help", "room_doors", "pause",
    "fast_forward", "dpad_mode",
};

/* Eight plates clockwise from the top. */
static const char *const wheel_slot_names[WHEEL_SLOTS] = {
    "up", "up_right", "right", "down_right", "down", "down_left", "left", "up_left",
};

static const char *const wheel_kind_names[] = {"action", "power", "crew", "systems", "settings"};

static const struct key_name key_names[] = {
    {"space", KEY_SPACE}, {"enter", KEY_ENTER}, {"return", KEY_ENTER}, {"esc", KEY_ESCAPE},
    {"escape", KEY_ESCAPE}, {"tab", 9},
    {"backspace", 8}, {"delete", 127}, {"up", 273}, {"down", 274}, {"right", 275},
    {"left", 276}, {"insert", 277}, {"home", 278}, {"end", 279}, {"pageup", 280},
    {"pagedown", 281}, {"shift", KEY_LSHIFT}, {"lshift", KEY_LSHIFT}, {"rshift", 303},
    {"ctrl", 306}, {"lctrl", 306}, {"alt", 308}, {"lalt", 308}, {"/", 47}, {"-", 45},
    {"=", 61}, {",", 44}, {".", 46}, {";", 59}, {"'", 39}, {"[", 91}, {"]", 93}, {"\\", 92},
    {"`", 96}, {"+", 43},
};

struct state states[MAX_STATES];
static int nstates;
int nwheels;

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

/* Names a key code as the ini spells it; returns NULL for a code with no name there. */
const char *key_name(int code)
{
    for (size_t i = 0; i < sizeof key_names / sizeof *key_names; i++)
        if (key_names[i].code == code)
            return key_names[i].name;
    return NULL;
}

char *trim(char *s)
{
    while (isspace((unsigned char)*s))
        s++;
    char *end = s + strlen(s);
    while (end > s && isspace((unsigned char)end[-1]))
        *--end = '\0';
    return s;
}

/* Parses one action ("key [add_shift]", mouse_left, mouse_right, hold_state NAME, wheel
 * NAME, a builtin name) for the named input; returns false when nothing usable was found. */
static bool parse_action(struct layout_set *set, const char *key, char *value, struct binding *out)
{
    struct binding b = {ACT_NONE, 0, false, 0, NULL};
    char *save = NULL;
    char *tok = strtok_r(value, " \t", &save);
    if (!tok)
        return false;
    int bi = lookup_name(builtin_names, BI_COUNT, tok);
    if (bi >= 0) {
        b.kind = ACT_BUILTIN;
        b.state = bi;
    } else if (strcmp(tok, "mouse_left") == 0) {
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

/* Parses "HOLD | TAP" or a single action into the hold binding and its tap half. */
static bool parse_value(struct layout_set *set, const char *key, char *value, struct binding *out,
                        struct binding *tap)
{
    char *bar = strchr(value, '|');
    tap->kind = ACT_NONE;
    if (bar) {
        *bar = '\0';
        if (!parse_action(set, key, trim(bar + 1), tap))
            tap->kind = ACT_NONE;
    }
    bool ok = parse_action(set, key, trim(value), out);
    out->tap = tap->kind != ACT_NONE ? tap : NULL;
    return ok;
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
        if (strcmp(value, "mouse_movement") == 0)
            s->slot[slot] = SLOT_MOUSE;
        else if (strcmp(value, "flick") == 0)
            s->slot[slot] = SLOT_FLICK;
        else {
            s->slot[slot] = SLOT_KEYS;
            if (*value)
                logmsg("layout: %s = %s not supported", key, value);
        }
        return;
    }
    int in = lookup_name(input_ini_names, IN_COUNT, key);
    if (in < 0) {
        logmsg("layout: unknown input %s", key);
        return;
    }
    s->bound[in] = parse_value(set, key, value, &s->bind[in], &s->taps[in]);
}

/* Parses a plate icon, "PATH" or "PATH@X,Y,W,H" with a region in texture pixels. */
static void parse_icon(struct plate *p, char *spec)
{
    char *at = strchr(spec, '@');
    if (at) {
        *at = '\0';
        p->icon_partial = sscanf(at + 1, "%f,%f,%f,%f", &p->icon_x, &p->icon_y, &p->icon_w, &p->icon_h) == 4;
        if (!p->icon_partial)
            logmsg("layout: bad icon region %s", at + 1);
    }
    strncpy(p->icon_path, spec, ICON_PATH_LEN - 1);
}

/* Parses one line of a wheel section: "kind = NAME" or a direction with a binding, a
 * quoted label and an optional icon. */
static void parse_wheel_line(struct layout_set *set, struct wheel *w, char *key, char *value)
{
    if (strcmp(key, "kind") == 0) {
        int k = lookup_name(wheel_kind_names, (int)(sizeof wheel_kind_names / sizeof *wheel_kind_names), value);
        if (k < 0)
            logmsg("layout: unknown wheel kind %s", value);
        else
            w->kind = (enum wheel_kind)k;
        return;
    }
    int slot = lookup_name(wheel_slot_names, WHEEL_SLOTS, key);
    if (slot < 0) {
        logmsg("layout: unknown wheel direction %s", key);
        return;
    }
    struct plate *p = &w->plates[slot];
    char *quote = strchr(value, '"');
    if (quote) {
        char *end = strchr(quote + 1, '"');
        if (end) {
            *end = '\0';
            char *icon = trim(end + 1);
            if (*icon)
                parse_icon(p, icon);
        }
        strncpy(p->label, quote + 1, LABEL_LEN - 1);
        *quote = '\0';
    }
    struct binding tap;
    p->bound = parse_value(set, key, value, &p->bind, &tap);
    p->bind.tap = NULL;
    p->available = true;
    if (p->bound && !p->label[0])
        strncpy(p->label, key, LABEL_LEN - 1);
}

/* Parses layout text in the gptokeyb2 ini format, plus [wheel:NAME] sections; the base
 * state is index 0. Labels in quotes are kept intact, a '#' only starts a comment outside
 * them. */
static void parse_layout(char *text, struct layout_set *set)
{
    memset(set, 0, sizeof *set);
    int base = find_state(set->states, &set->nstates, BASE_SECTION);
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
            const char *section = line + 1;
            if (strcmp(section, BASE_SECTION) == 0) {
                state = &set->states[base];
            } else if (strncmp(section, STATE_SECTION, strlen(STATE_SECTION)) == 0) {
                int idx = find_state(set->states, &set->nstates, section + strlen(STATE_SECTION));
                state = idx >= 0 ? &set->states[idx] : NULL;
                if (idx >= 0)
                    set->state_defined[idx] = true;
            } else if (strncmp(section, WHEEL_SECTION, strlen(WHEEL_SECTION)) == 0) {
                int idx = find_wheel(set->wheels, &set->nwheels, section + strlen(WHEEL_SECTION));
                wheel = idx >= 0 ? &set->wheels[idx] : NULL;
                if (idx >= 0)
                    set->wheel_defined[idx] = true;
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
    for (int i = base + 1; i < set->nstates; i++)
        if (!set->state_defined[i])
            logmsg("layout: state %s has no section", set->states[i].name);
    for (int i = 0; i < set->nwheels; i++)
        if (!set->wheel_defined[i])
            logmsg("layout: wheel %s has no section", set->wheels[i].name);
}

void load_layout_text(char *text, const char *origin)
{
    static struct layout_set set;
    parse_layout(text, &set);
    if (set.nstates == 0) {
        logmsg("layout: nothing usable in %s", origin);
        return;
    }
    wheel_close();
    memcpy(states, set.states, sizeof states);
    memcpy(wheels, set.wheels, sizeof wheels);
    nstates = set.nstates;
    nwheels = set.nwheels;
    for (int i = 0; i < nstates; i++)
        for (int in = 0; in < IN_COUNT; in++)
            states[i].bind[in].tap = states[i].bind[in].tap ? &states[i].taps[in] : NULL;
    depth = 0;
    memset(pending, 0, sizeof pending);
    logmsg("layout: %d states, %d wheels from %s", nstates, nwheels, origin);
}

void load_layout(const char *path)
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
