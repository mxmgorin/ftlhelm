/* Drawing helpers over the game's renderer: text through its fonts, panels, discs,
 * game images by path, and the shared palette. */
#include "ftlpad.h"

#define IMAGE_CACHE_SIZE 128
#define DISC_SCANLINE 4.0f
#define WHEEL_BEVEL 2

/* The game's palette: dark teal panels, pale green frames, yellow for the selection,
 * green for powered items. */
const GL_Color plate_color = {0.05f, 0.11f, 0.1f, 0.9f};

const GL_Color plate_edge_color = {0.63f, 0.9f, 0.76f, 0.95f};
static const GL_Color plate_bevel_color = {0.0f, 0.05f, 0.05f, 0.9f};
const GL_Color text_color = {1.0f, 1.0f, 1.0f, 1.0f};
const GL_Color text_disabled_color = {0.42f, 0.46f, 0.45f, 1.0f};

/* The font of the wheel plates' text. */
int plate_font = WHEEL_FONT;

/* A std::string built by the game's own constructor; release it with free_string. */
static void *make_string(const char *text)
{
    void *str = NULL;
    char alloc = 0;
    game.string_ctor(&str, text, &alloc);
    return str;
}

static void free_string(void *str)
{
    game.string_dtor(&str);
}

void draw_text_left(int font, float x, float y, const char *text)
{
    void *str = make_string(text);
    game.print_left(font, x, y, &str);
    free_string(str);
}

/* Prints text centred at x,y with the game's font code. */
void draw_text_center(int font, float x, float y, const char *text)
{
    void *str = make_string(text);
    game.print_center(font, x, y, &str);
    free_string(str);
}

/* A panel in the game's style: a dark bevel around a framed fill. */
void draw_panel(float x, float y, float w, float h, GL_Color fill, GL_Color edge)
{
    game.draw_rect(x - WHEEL_BEVEL, y - WHEEL_BEVEL, w + 2 * WHEEL_BEVEL, h + 2 * WHEEL_BEVEL, plate_bevel_color);
    game.draw_rect(x, y, w, h, fill);
    game.draw_rect_outline((int)x, (int)y, (int)w, (int)h, edge, OVERLAY_LINE);
}

/* A filled disc out of scanlines: the game draws no curves. */
void draw_disc(float cx, float cy, float r, GL_Color color)
{
    for (float y = -r; y < r; y += DISC_SCANLINE) {
        float t = (y + DISC_SCANLINE / 2) / r;
        float half = r * sqrtf(fmaxf(0.0f, 1.0f - t * t));
        game.draw_rect(cx - half, cy + y, 2 * half, DISC_SCANLINE, color);
    }
}

/* Looks up a game image by its path under img/; returns its texture, or NULL when the
 * game has none. Results are cached, misses included. */
void *game_image(const char *path)
{
    static struct {
        char path[ICON_PATH_LEN];
        void *texture;
    } cache[IMAGE_CACHE_SIZE];
    static int ncache;
    for (int i = 0; i < ncache; i++)
        if (strcmp(cache[i].path, path) == 0)
            return cache[i].texture;
    void *str = make_string(path);
    void *texture = game.image_id(game.resources, &str);
    free_string(str);
    if (!texture)
        logmsg("image missing: %s", path);
    if (ncache < IMAGE_CACHE_SIZE) {
        strncpy(cache[ncache].path, path, ICON_PATH_LEN - 1);
        cache[ncache++].texture = texture;
    }
    return texture;
}

void upper_ascii(char *text)
{
    for (; *text; text++)
        *text = (char)toupper((unsigned char)*text);
}

int text_width(int font, const char *text)
{
    void *str = make_string(text);
    int w = game.measure_width(font, &str);
    free_string(str);
    return w;
}
