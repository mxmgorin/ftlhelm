/* Symbol resolution from the executable's .symtab, the trampoline page near the
 * binary, inline hooks, and the redirected step cap of CFPS::OnLoop. */
#include "ftlpad.h"

#define EXE_ENV "FTLPAD_EXE"
#define EXE_FALLBACK "/proc/self/exe"

/* Distance between candidate addresses when the trampoline page must be placed by search. */
#define TRAMPOLINE_STEP 0x1000000

#define TRAMPOLINE_HINT ((void *)0x70000000)
#define TRAMPOLINE_PAGE 4096
#define FAR_JMP_LEN 12
#define NOP 0x90

/* CFPS::OnLoop caps the step with "movss cap(%rip),%xmm1; unpcklpd %xmm0,%xmm0" within
 * this many bytes of its start; the fast speed doubles the cap. */
#define STEP_CAP_SCAN 0x100

#define MAPS_LINE 512

struct joy_api joy;
struct game_api game;

static const struct symbol_slot symbols[] = {
    {"input_joystick_count", (void **)&joy.count},
    {"input_joystick_connected", (void **)&joy.connected},
    {"input_joystick_num_sticks", (void **)&joy.num_sticks},
    {"input_joystick_button_mapping", (void **)&joy.button_mapping},
    {"input_joystick_button_state", (void **)&joy.button_state},
    {"input_joystick_read_stick", (void **)&joy.read_stick},
    {"input_joystick_read_dpad", (void **)&joy.read_dpad},
    {"input_enable_unfocused_joystick", (void **)&joy.enable_unfocused},
    {"input_joystick_rumble", (void **)&joy.rumble},
    {"_ZN4CFPS10FPSControlE", (void **)&game.fps_control},
    {"_ZN4CFPS6OnLoopEv", (void **)&game.fps_loop},
    {"_ZN11TextLibrary7GetTextERKSsS1_", (void **)&game.get_text},
    {"_ZN7Globals7LibraryE", (void **)&game.text_library},
    {"_ZN4CApp6OnLoopEv", &game.onloop},
    {"_ZN12MouseControl8OnRenderEv", &game.mouse_render},
    {"_ZN6CEvent13GetShiftStateEv", &game.get_shift},
    {"_ZN4CApp11OnMouseMoveEiiiibbb", (void **)&game.mouse_move},
    {"_ZN4CApp13OnLButtonDownEii", (void **)&game.lbutton_down},
    {"_ZN4CApp11OnLButtonUpEii", (void **)&game.lbutton_up},
    {"_ZN4CApp13OnRButtonDownEii", (void **)&game.rbutton_down},
    {"_ZN4CApp11OnRButtonUpEii", (void **)&game.rbutton_up},
    {"_ZN4CApp9OnKeyDownEi", (void **)&game.key_down},
    {"_ZN4CApp7OnKeyUpEi", (void **)&game.key_up},
    {"_ZN4CApp13OnRequestExitEv", (void **)&game.request_exit},
    {"_ZN8CSurface18GL_DrawRectOutlineEiiii8GL_Colorf", (void **)&game.draw_rect_outline},
    {"_ZN4Ship17GetSelectedRoomIdEiib", (void **)&game.selected_room},
    {"_ZN8CSurface11GL_DrawRectEffff8GL_Color", (void **)&game.draw_rect},
    {"_ZN8CSurface11GL_SetColorE8GL_Color", (void **)&game.set_color},
    {"_ZN8CSurface12GL_BlitImageEP10GL_Texturefffff8GL_Colorb", (void **)&game.blit},
    {"_ZN8CSurface19GL_BlitImagePartialEP10GL_Texturefffffffff8GL_Colorb", (void **)&game.blit_partial},
    {"_ZN8CSurface13GL_PushMatrixEv", (void **)&game.push_matrix},
    {"_ZN8CSurface12GL_PopMatrixEv", (void **)&game.pop_matrix},
    {"_ZN8CSurface12GL_TranslateEfff", (void **)&game.translate},
    {"_ZN8CSurface8GL_ScaleEfff", (void **)&game.scale},
    {"_ZN15ResourceControl10GetImageIdERKSs", (void **)&game.image_id},
    {"_ZN15ResourceControl15GlobalResourcesE", (void **)&game.resources},
    {"_ZN13CrewAnimation10RenderIconEb", (void **)&game.crew_icon},
    {"_ZNSsC1EPKcRKSaIcE", (void **)&game.string_ctor},
    {"_ZNSsD1Ev", (void **)&game.string_dtor},
    {"_ZN8freetype16easy_printCenterEiffRKSs", (void **)&game.print_center},
    {"_ZN8freetype10easy_printEiffRKSs", (void **)&game.print_left},
    {"_ZN8freetype17easy_measureWidthEiRKSs", (void **)&game.measure_width},
    {"_ZN13GenericButton9MouseMoveEiib", &game.button_move},
    {"_ZN11ShipManager13GetSystemRoomEi", (void **)&game.system_room},
    {"_ZN11ShipManager15GetSystemInRoomEi", (void **)&game.system_in_room},
    {"_ZN12TabbedWindow6SetTabEj", (void **)&game.set_tab},
    {"_ZN11CrewControl17ClearSelectedCrewEv", (void **)&game.clear_crew},
    {"_ZN11CrewControl12DeselectCrewEP10CrewMember", (void **)&game.deselect_crew},
    {"_ZN12MouseControl5MouseE", (void **)&game.mouse_position},
};

static const unsigned char step_cap_load[] = {0xf3, 0x0f, 0x10, 0x0d};
static const unsigned char step_cap_next[] = {0x66, 0x0f, 0x14, 0xc0};
static unsigned char *trampolines;
static size_t trampolines_used;

static int read_at(int fd, void *buf, size_t len, off_t at)
{
    return pread(fd, buf, len, at) == (ssize_t)len;
}

/* Fills the symbol slots from the executable's .symtab; returns the number left unresolved. */
int resolve_symbols(void)
{
    const char *path = getenv(EXE_ENV);
    int fd = open(path ? path : EXE_FALLBACK, O_RDONLY);
    size_t wanted = sizeof symbols / sizeof *symbols;
    if (fd < 0) {
        logmsg("cannot open the executable");
        return (int)wanted;
    }
    Elf64_Ehdr eh;
    Elf64_Shdr *sh = NULL;
    if (!read_at(fd, &eh, sizeof eh, 0) || eh.e_shentsize != sizeof *sh)
        goto out;
    sh = calloc(eh.e_shnum, sizeof *sh);
    if (!sh || !read_at(fd, sh, eh.e_shnum * sizeof *sh, (off_t)eh.e_shoff))
        goto out;
    size_t left = wanted;
    for (int i = 0; i < eh.e_shnum && left > 0; i++) {
        if (sh[i].sh_type != SHT_SYMTAB || sh[i].sh_link >= eh.e_shnum)
            continue;
        Elf64_Shdr *st = &sh[sh[i].sh_link];
        char *strtab = malloc(st->sh_size);
        Elf64_Sym *syms = malloc(sh[i].sh_size);
        if (strtab && syms && read_at(fd, strtab, st->sh_size, (off_t)st->sh_offset)
                && read_at(fd, syms, sh[i].sh_size, (off_t)sh[i].sh_offset)) {
            for (size_t s = 0; s < sh[i].sh_size / sizeof *syms && left > 0; s++) {
                if (!syms[s].st_value || syms[s].st_name >= st->sh_size)
                    continue;
                const char *name = strtab + syms[s].st_name;
                for (size_t w = 0; w < wanted; w++)
                    if (!*symbols[w].slot && name[0] == symbols[w].name[0] && strcmp(name, symbols[w].name) == 0) {
                        *symbols[w].slot = (void *)syms[s].st_value;
                        left--;
                    }
            }
        }
        free(strtab);
        free(syms);
    }
out:
    free(sh);
    close(fd);
    int missing = 0;
    for (size_t w = 0; w < wanted; w++)
        if (!*symbols[w].slot) {
            logmsg("symbol missing: %s", symbols[w].name);
            missing++;
        }
    return missing;
}

/* Checks that addr lies in an executable mapping of this process; a preloaded library
 * also lands in the shell or wrapper that launches the game. */
int mapped_executable(const void *addr)
{
    FILE *f = fopen(MAPS, "r");
    if (!f)
        return 0;
    int found = 0;
    char line[MAPS_LINE];
    while (!found && fgets(line, sizeof line, f)) {
        unsigned long lo, hi;
        char perms[8];
        if (sscanf(line, "%lx-%lx %7s", &lo, &hi, perms) == 3 && perms[2] == 'x')
            found = lo <= (uintptr_t)addr && (uintptr_t)addr < hi;
    }
    fclose(f);
    return found;
}

static int protect(void *at, size_t len, int prot)
{
    uintptr_t page = (uintptr_t)sysconf(_SC_PAGESIZE);
    uintptr_t start = (uintptr_t)at & ~(page - 1);
    uintptr_t end = ((uintptr_t)at + len + page - 1) & ~(page - 1);
    return mprotect((void *)start, end - start, prot);
}

static int fits_rel32(const void *from, const void *to)
{
    intptr_t d = (intptr_t)to - (intptr_t)from;
    return d >= INT32_MIN && d <= INT32_MAX;
}

static void put_jmp(unsigned char *at, const void *to)
{
    int32_t rel = (int32_t)((intptr_t)to - (intptr_t)(at + JMP_LEN));
    at[0] = 0xe9;
    memcpy(at + 1, &rel, sizeof rel);
}

/* mov $to,%rax; jmp *%rax */
static void put_far_jmp(unsigned char *at, const void *to)
{
    uint64_t abs = (uint64_t)(uintptr_t)to;
    at[0] = 0x48;
    at[1] = 0xb8;
    memcpy(at + 2, &abs, sizeof abs);
    at[10] = 0xff;
    at[11] = 0xe0;
}

static void *map_trampolines(void *at, int flags)
{
    void *page = mmap(at, TRAMPOLINE_PAGE, PROT_READ | PROT_WRITE | PROT_EXEC,
                      MAP_PRIVATE | MAP_ANONYMOUS | flags, -1, 0);
    return page == MAP_FAILED ? NULL : page;
}

/* Maps the trampoline page within a rel32 jump of near: at the hint, else at the first
 * free step around near; returns it, or NULL. */
static unsigned char *place_trampolines(const void *near)
{
    unsigned char *page = map_trampolines(TRAMPOLINE_HINT, 0);
    if (page && fits_rel32(near, page))
        return page;
    if (page)
        munmap(page, TRAMPOLINE_PAGE);
    uintptr_t base = (uintptr_t)near & ~(uintptr_t)(TRAMPOLINE_STEP - 1);
    for (uintptr_t d = TRAMPOLINE_STEP; d < INT32_MAX / 2; d += TRAMPOLINE_STEP) {
        const uintptr_t candidates[] = {base + d, base - d};
        for (size_t i = 0; i < sizeof candidates / sizeof *candidates; i++) {
            if (i == 1 && d > base)
                continue;
            page = map_trampolines((void *)candidates[i], MAP_FIXED_NOREPLACE);
            /* Kernels without MAP_FIXED_NOREPLACE take the address as a hint. */
            if (page && fits_rel32(near, page))
                return page;
            if (page)
                munmap(page, TRAMPOLINE_PAGE);
        }
    }
    return NULL;
}

/* Diverts fn to repl after checking its first bytes; returns an entry that runs the
 * original, or NULL with the reason logged under name. The prologue must be whole
 * instructions with no relative operands. */
void *hook(const char *name, void *fn, void *repl, const unsigned char *prologue, size_t len)
{
    if (len < JMP_LEN || memcmp(fn, prologue, len) != 0) {
        logmsg("%s: prologue differs", name);
        return NULL;
    }
    if (!trampolines)
        trampolines = place_trampolines(fn);
    size_t need = FAR_JMP_LEN + len + JMP_LEN;
    if (!trampolines || !fits_rel32(fn, trampolines)) {
        logmsg("%s: no trampoline page within reach", name);
        return NULL;
    }
    if (trampolines_used + need > TRAMPOLINE_PAGE) {
        logmsg("%s: trampoline page full", name);
        return NULL;
    }
    unsigned char *entry = trampolines + trampolines_used;
    unsigned char *orig = entry + FAR_JMP_LEN;
    put_far_jmp(entry, repl);
    memcpy(orig, prologue, len);
    put_jmp(orig + len, (unsigned char *)fn + len);
    trampolines_used += need;

    if (protect(fn, len, PROT_READ | PROT_WRITE | PROT_EXEC) != 0) {
        logmsg("%s: code not writable", name);
        return NULL;
    }
    put_jmp(fn, entry);
    memset((unsigned char *)fn + JMP_LEN, NOP, len - JMP_LEN);
    protect(fn, len, PROT_READ | PROT_EXEC);
    return orig;
}

/* Points CFPS::OnLoop's step cap load at a float in the trampoline page, so the speed
 * can raise the cap; returns that float, or NULL with the reason logged. */
float *redirect_step_cap(void)
{
    const size_t insn = sizeof step_cap_load + sizeof(int32_t);
    unsigned char *site = NULL;
    for (size_t i = 0; i + insn + sizeof step_cap_next <= STEP_CAP_SCAN; i++) {
        unsigned char *at = game.fps_loop + i;
        if (memcmp(at, step_cap_load, sizeof step_cap_load) != 0
                || memcmp(at + insn, step_cap_next, sizeof step_cap_next) != 0)
            continue;
        if (site) {
            logmsg("step cap: load not unique");
            return NULL;
        }
        site = at;
    }
    if (!site) {
        logmsg("step cap: load not found");
        return NULL;
    }
    unsigned char *disp_at = site + sizeof step_cap_load;
    unsigned char *next = site + insn;
    int32_t disp;
    memcpy(&disp, disp_at, sizeof disp);
    const float *cap = (const float *)(next + disp);
    size_t slot = (trampolines_used + sizeof(float) - 1) & ~(sizeof(float) - 1);
    if (!trampolines || slot + sizeof(float) > TRAMPOLINE_PAGE || !fits_rel32(next, trampolines + slot)) {
        logmsg("step cap: no room near the code");
        return NULL;
    }
    float *redirected = (float *)(trampolines + slot);
    trampolines_used = slot + sizeof(float);
    *redirected = *cap;
    if (protect(disp_at, sizeof disp, PROT_READ | PROT_WRITE | PROT_EXEC) != 0) {
        logmsg("step cap: code not writable");
        return NULL;
    }
    disp = (int32_t)((intptr_t)redirected - (intptr_t)next);
    memcpy(disp_at, &disp, sizeof disp);
    protect(disp_at, sizeof disp, PROT_READ | PROT_EXEC);
    return redirected;
}
