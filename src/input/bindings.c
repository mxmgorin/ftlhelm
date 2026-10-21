/* The binding engine: state stack lookup, tap and hold resolution, builtins, and
 * where wheels may open. */
#include "ftlpad.h"

const enum input slot_first[SLOT_COUNT] = {IN_DPAD_LEFT, IN_LSTICK_LEFT, IN_RSTICK_LEFT};

/* Doors are the zones while precision is held. */
bool precision;

/* The abilities key X repeats on a tap. */
int last_ability_key;

bool fast_forward;

/* dpad_mode toggled: the D-pad and the sticks move the cursor instead of walking zones. */
bool dpad_mouse;

double pause_ff_since;
struct pending_press pending[IN_COUNT];
static struct stack_entry stack[MAX_STATES];
int depth;

/* What each held input did when pressed, undone on release. */
static struct binding active[IN_COUNT];

/* The button context_click pressed for each input, ACT_NONE when a wheel took the press. */
static enum act_kind context_done[IN_COUNT + 1];

const struct state *state_at(int level)
{
    return &states[level == 0 ? 0 : stack[level].state];
}

/* The binding an input resolves to in the active state chain, or NULL. */
static const struct binding *lookup(enum input in)
{
    for (int level = depth; level >= 0; level--) {
        const struct state *s = state_at(level);
        if (s->bound[in])
            return &s->bind[in];
        if (level > 0 && !s->parent)
            break;
    }
    return NULL;
}

/* What a pointer source does in the active state chain: keys once any state binds its
 * directions, else the mode the nearest state sets. */
enum slot_mode slot_mode(enum slot slot)
{
    for (int level = depth; level >= 0; level--) {
        const struct state *s = state_at(level);
        for (int d = 0; d < 4; d++)
            if (s->bound[slot_first[slot] + d])
                return SLOT_KEYS;
        if (s->slot[slot] != SLOT_KEYS)
            return s->slot[slot];
        if (level > 0 && !s->parent)
            break;
    }
    return SLOT_KEYS;
}

/* Runs a builtin on the press or release of input in, NO_INPUT for a wheel plate. */
void builtin(enum builtin id, enum input in, bool down, double t)
{
    switch (id) {
    case BI_CONTEXT_CLICK:
        if (!down) {
            if (context_done[in] != ACT_NONE)
                click(context_done[in], false);
            context_done[in] = ACT_NONE;
        } else if (wheel_open >= 0 && in != NO_INPUT) {
            context_done[in] = ACT_NONE;
            wheel_act();
        } else {
            /* With crew selected and a room under the cursor, A sends them there, which
             * the game does on the right button. */
            int under = target_under_cursor();
            bool crew_order = crew_selected() && !aiming() && under >= 0
                              && (targets[under].kind == T_ROOM || targets[under].kind == T_ENEMY_ROOM);
            context_done[in] = crew_order ? ACT_RCLICK : ACT_LCLICK;
            click(context_done[in], true);
        }
        break;
    case BI_CANCEL:
        if (!down)
            break;
        if (wheel_open >= 0)
            wheel_close();
        else if (gui_ptr() && (window_open((char *)gui_ptr() + GUI_MENU_BOX)
                               || window_open((char *)gui_ptr() + GUI_OPTIONS_BOX)))
            tap_key(KEY_ESCAPE, false);
        else if (aiming()) {
            /* The right button cancels targeting. */
            click(ACT_RCLICK, true);
            click(ACT_RCLICK, false);
        } else if (crew_selected()) {
            game.clear_crew((char *)gui_ptr() + GUI_CREW_CONTROL);
        } else if (open_tabbed() || (gui_ptr() && window_open((char *)gui_ptr() + GUI_CHOICE_BOX))
                   || (gui_ptr() && window_open(deref(gui_ptr(), GUI_STAR_MAP)))) {
            tap_key(KEY_ESCAPE, false);
        }
        break;
    case BI_PAUSE_FF:
        if (down) {
            tap_key(KEY_SPACE, false);
            pause_ff_since = t;
        } else {
            if (fast_forward)
                set_fast(false);
            pause_ff_since = 0;
        }
        break;
    case BI_ARM_NEXT:
        if (down)
            arm_next();
        break;
    case BI_AIM_CREW:
        /* On a tabbed screen the shoulder tap walks its tabs instead. */
        if (down)
            open_tabbed() ? tab_step(1) : select_next_crew();
        break;
    case BI_REPEAT_ABILITY:
        if (down && last_ability_key)
            tap_key(last_ability_key, false);
        break;
    case BI_PRECISION:
        /* Held: doors are the zones for as long as the button is down. */
        precision = down;
        break;
    case BI_CENTER_SWAP:
        if (down)
            center_swap();
        break;
    case BI_PREV_TAB:
    case BI_NEXT_TAB:
        if (down)
            tab_step(id == BI_NEXT_TAB ? 1 : -1);
        break;
    case BI_HELP:
        if (down)
            help_shown = !help_shown;
        break;
    case BI_ROOM_DOORS:
        if (down)
            room_doors();
        break;
    case BI_PAUSE:
        if (down)
            tap_key(KEY_SPACE, false);
        break;
    case BI_DPAD_MODE:
        if (down)
            dpad_mouse = !dpad_mouse;
        break;
    case BI_FAST_FORWARD:
        if (down) {
            if (game_paused())
                tap_key(KEY_SPACE, false);
            set_fast(true);
        } else {
            set_fast(false);
        }
        break;
    case BI_COUNT:
        break;
    }
}

/* Performs a binding's press or release. */
static void perform(const struct binding *b, enum input in, bool down, double t)
{
    switch (b->kind) {
    case ACT_NONE:
        break;
    case ACT_KEY:
        if (down) {
            if (b->shift)
                set_shift(true);
            game.key_down(app, b->key);
        } else {
            game.key_up(app, b->key);
            if (b->shift)
                set_shift(false);
        }
        break;
    case ACT_LCLICK:
    case ACT_RCLICK:
        click(b->kind, down);
        break;
    case ACT_STATE:
        if (down) {
            if (depth + 1 < MAX_STATES) {
                depth++;
                stack[depth].state = b->state;
                stack[depth].by = in;
            }
        } else {
            for (int level = depth; level > 0; level--)
                if (stack[level].by == in) {
                    depth = level - 1;
                    break;
                }
        }
        break;
    case ACT_WHEEL:
        if (down)
            wheel_show(b->state, in);
        break;
    case ACT_BUILTIN:
        builtin((enum builtin)b->state, in, down, t);
        break;
    }
    if (debug && down)
        logmsg("%s: %s", input_ini_names[in],
               b->kind == ACT_KEY ? "key" : b->kind == ACT_BUILTIN ? builtin_names[b->state]
               : b->kind == ACT_WHEEL ? "wheel" : b->kind == ACT_STATE ? "state" : "click");
}

/* Whether a tap binding would do nothing in the current context. */
static bool tap_idle(const struct binding *tap)
{
    if (tap->kind != ACT_BUILTIN)
        return false;
    switch ((enum builtin)tap->state) {
    case BI_PREV_TAB:
    case BI_NEXT_TAB:
        return !open_tabbed();
    case BI_REPEAT_ABILITY:
        return !last_ability_key;
    default:
        return false;
    }
}

/* A press runs its binding at once, unless it carries a tap half that would act: then it
 * waits for the threshold (hold) or the release (tap). */
/* Performs a binding's press and records it for the release. */
void start(const struct binding *b, enum input in, double t)
{
    perform(b, in, true, t);
    active[in] = *b;
    active[in].tap = NULL;
}

/* Whether wheels may open: anywhere, or with wheels_ship_only only on the ship view with
 * nothing over it (no menu, ship or store screen, dialog or map). */
bool wheel_screen(void)
{
    const char *gui = gui_ptr();
    size_t size;
    if (!cfg.wheels_ship_only)
        return true;
    return gui && player_complete() && !*((const unsigned char *)app + CAPP_MAIN_MENU_OPEN)
           && !open_tabbed() && !modal_window(gui, &size) && !window_open(gui + GUI_CHOICE_BOX)
           && !window_open(deref(gui, GUI_STAR_MAP));
}

void press(enum input in, double t)
{
    const struct binding *b = lookup(in);
    active[in].kind = ACT_NONE;
    if (!b)
        return;
    /* Off the ship view a wheel button is its tap alone, at once. */
    if (b->kind == ACT_WHEEL && wheel_open < 0 && !wheel_screen()) {
        if (b->tap) {
            perform(b->tap, in, true, t);
            perform(b->tap, in, false, t);
        }
        return;
    }
    /* The open wheel's own button closes it, its tap half included. */
    if (b->kind == ACT_WHEEL && b->state == wheel_open) {
        wheel_close();
        return;
    }
    if (b->tap && !tap_idle(b->tap)) {
        pending[in].bind = b;
        pending[in].since = t;
        return;
    }
    start(b, in, t);
}

void release(enum input in, double t)
{
    const struct binding *b = pending[in].bind;
    if (b) {
        pending[in].bind = NULL;
        perform(b->tap, in, true, t);
        perform(b->tap, in, false, t);
        return;
    }
    perform(&active[in], in, false, t);
    active[in].kind = ACT_NONE;
}

/* Turns presses that outlived the tap threshold into holds. */
void settle_pending(double t)
{
    for (int in = 0; in < IN_COUNT; in++) {
        const struct binding *b = pending[in].bind;
        if (!b || t - pending[in].since < cfg.tap_hold)
            continue;
        pending[in].bind = NULL;
        start(b, (enum input)in, t);
    }
}
