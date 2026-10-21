/* ftlpad.ini: option table, defaults, clamping, and the mtime poll that reloads
 * both files in the C numeric locale. */
#include "ftlpad.h"

#define CONFIG_ENV "FTLPAD_CONFIG"
#define LAYOUT_ENV "FTLPAD_LAYOUT"
#define FILE_POLL_S 1.0
#define SPEED_LEVEL_MIN 1
#define SPEED_LEVEL_MAX 6
#define RUMBLE_S_MIN 0.01f
#define RUMBLE_S_MAX 0.5f
#define STICK_DEADZONE_MAX 0.9f
#define CURSOR_SPEED_MIN 1.0f
#define CURSOR_CURVE_MIN 0.5f
#define CURSOR_CURVE_MAX 4.0f
#define HOLD_S_MIN 0.05f
#define HOLD_S_MAX 2.0f

/* The narrowest band a flick threshold keeps above the re-arm level. */
#define FLICK_GAP 0.1f

#define CONE_DEG_MIN 5.0f
#define CONE_DEG_MAX 89.0f
#define FOCUS_SIDE_WEIGHT_MAX 10.0f

/* What a key missing from the config file takes. */
const struct config cfg_defaults = {
    .cursor_speed = CURSOR_SPEED,
    .cursor_deadzone = STICK_DEADZONE,
    .cursor_curve = CURSOR_CURVE,
    .overlay = true,
    .snap_dpad = true,
    .autopause_wheel = true,
    .autopause_aim = false,
    .speed_level = SPEED_LEVEL_DEFAULT,
    .rumble = true,
    .flick_on = FLICK_ON,
    .flick_repeat = FLICK_REPEAT_S,
    .nintendo = -1,
    .swap_ab = true,
    .swap_xy = false,
    .tap_hold = TAP_HOLD_S,
    .pause_ff_hold = FF_HOLD_S,
    .focus_cone_narrow = CONE_NARROW_DEG,
    .focus_cone_wide = CONE_WIDE_DEG,
    .focus_side_weight = FOCUS_SIDE_WEIGHT,
    .focus_first = true,
    .crew_follow = true,
    .wheels_ship_only = true,
    .rumble_strength = RUMBLE_STRENGTH,
    .rumble_time = RUMBLE_S,
};

struct config cfg;

static const struct option_def options[] = {
    {"cursor_speed", OPT_FLOAT, offsetof(struct config, cursor_speed), CURSOR_SPEED_MIN, INFINITY},
    {"cursor_deadzone", OPT_FLOAT, offsetof(struct config, cursor_deadzone), 0.0f, STICK_DEADZONE_MAX},
    {"cursor_curve", OPT_FLOAT, offsetof(struct config, cursor_curve), CURSOR_CURVE_MIN, CURSOR_CURVE_MAX},
    {"overlay", OPT_BOOL, offsetof(struct config, overlay), 0, 0},
    {"snap_dpad", OPT_BOOL, offsetof(struct config, snap_dpad), 0, 0},
    {"autopause_wheel", OPT_BOOL, offsetof(struct config, autopause_wheel), 0, 0},
    {"autopause_aim", OPT_BOOL, offsetof(struct config, autopause_aim), 0, 0},
    {"speed_level", OPT_INT, offsetof(struct config, speed_level), SPEED_LEVEL_MIN, SPEED_LEVEL_MAX},
    {"rumble", OPT_BOOL, offsetof(struct config, rumble), 0, 0},
    {"nintendo_labels", OPT_INT, offsetof(struct config, nintendo), -1, 1},
    {"swap_ab", OPT_BOOL, offsetof(struct config, swap_ab), 0, 0},
    {"swap_xy", OPT_BOOL, offsetof(struct config, swap_xy), 0, 0},
    {"flick_threshold", OPT_FLOAT, offsetof(struct config, flick_on), FLICK_OFF + FLICK_GAP, 1.0f},
    {"flick_repeat", OPT_FLOAT, offsetof(struct config, flick_repeat), 0.0f, INFINITY},
    {"tap_hold", OPT_FLOAT, offsetof(struct config, tap_hold), HOLD_S_MIN, HOLD_S_MAX},
    {"pause_ff_hold", OPT_FLOAT, offsetof(struct config, pause_ff_hold), HOLD_S_MIN, HOLD_S_MAX},
    {"focus_cone_narrow", OPT_FLOAT, offsetof(struct config, focus_cone_narrow), CONE_DEG_MIN, CONE_DEG_MAX},
    {"focus_cone_wide", OPT_FLOAT, offsetof(struct config, focus_cone_wide), CONE_DEG_MIN, CONE_DEG_MAX},
    {"focus_side_weight", OPT_FLOAT, offsetof(struct config, focus_side_weight), 0.0f, FOCUS_SIDE_WEIGHT_MAX},
    {"focus_first", OPT_BOOL, offsetof(struct config, focus_first), 0, 0},
    {"crew_follow", OPT_BOOL, offsetof(struct config, crew_follow), 0, 0},
    {"wheels_ship_only", OPT_BOOL, offsetof(struct config, wheels_ship_only), 0, 0},
    {"rumble_strength", OPT_FLOAT, offsetof(struct config, rumble_strength), 0.0f, 1.0f},
    {"rumble_time", OPT_FLOAT, offsetof(struct config, rumble_time), RUMBLE_S_MIN, RUMBLE_S_MAX},
};

static void set_option(size_t i, float value)
{
    char *field = (char *)&cfg + options[i].offset;
    if (options[i].type == OPT_BOOL) {
        *(bool *)field = value != 0;
    } else if (options[i].type == OPT_INT) {
        *(int *)field = (int)clampf(value, options[i].lo, options[i].hi);
    } else {
        *(float *)field = clampf(value, options[i].lo, options[i].hi);
    }
}

/* Logs every option's current value on one line. */
static void log_options(void)
{
    char text[CMD_BUF];
    size_t used = 0;
    for (size_t i = 0; i < sizeof options / sizeof *options && used < sizeof text; i++) {
        const char *field = (const char *)&cfg + options[i].offset;
        if (options[i].type == OPT_FLOAT)
            used += (size_t)snprintf(text + used, sizeof text - used, " %s=%.2f", options[i].name, *(const float *)field);
        else
            used += (size_t)snprintf(text + used, sizeof text - used, " %s=%d", options[i].name,
                                     options[i].type == OPT_INT ? *(const int *)field : *(const bool *)field);
    }
    logmsg("config:%s", text);
}

/* Reads the config file named by the environment: "key = value" lines, '#' comments. A
 * key the file leaves out takes its default. */
static void load_config(const char *path)
{
    FILE *f = fopen(path, "r");
    if (!f)
        return;
    cfg = cfg_defaults;
    char line[LINE_LEN];
    while (fgets(line, sizeof line, f)) {
        char *hash = strchr(line, '#');
        if (hash)
            *hash = '\0';
        char key[NAME_LEN];
        float value;
        if (sscanf(line, " %31[a-z_] = %f", key, &value) != 2)
            continue;
        size_t i = 0;
        while (i < sizeof options / sizeof *options && strcmp(options[i].name, key) != 0)
            i++;
        if (i < sizeof options / sizeof *options)
            set_option(i, value);
        else
            logmsg("config: unknown key %s", key);
    }
    fclose(f);
    log_options();
}

/* Re-reads a file when its mtime changes, at most once per poll interval. */
static void poll_file(const char *path, struct timespec *last_mtime, void (*load)(const char *))
{
    struct stat st;
    if (!path || stat(path, &st) != 0)
        return;
    if (st.st_mtim.tv_sec != last_mtime->tv_sec || st.st_mtim.tv_nsec != last_mtime->tv_nsec) {
        *last_mtime = st.st_mtim;
        load(path);
    }
}

void poll_files(double t)
{
    static double last_poll;
    static struct timespec config_mtime, layout_mtime;
    if (t - last_poll < FILE_POLL_S)
        return;
    last_poll = t;
    /* Numbers in the files use a dot whatever locale the game has set. */
    static locale_t c_numeric;
    if (!c_numeric)
        c_numeric = newlocale(LC_NUMERIC_MASK, "C", (locale_t)0);
    locale_t previous = c_numeric ? uselocale(c_numeric) : (locale_t)0;
    poll_file(getenv(CONFIG_ENV), &config_mtime, load_config);
    poll_file(getenv(LAYOUT_ENV), &layout_mtime, load_layout);
    if (previous)
        uselocale(previous);
}
