/* Clock, stderr log and small numeric helpers. */
#include "ftlpad.h"

#define LOG_PREFIX "ftlpad: "

double now_s(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec / 1e9;
}

void logmsg(const char *fmt, ...)
{
    static double start;
    if (start == 0)
        start = now_s();
    va_list ap;
    va_start(ap, fmt);
    fprintf(stderr, LOG_PREFIX "%7.2f ", now_s() - start);
    vfprintf(stderr, fmt, ap);
    fputc('\n', stderr);
    fflush(stderr);
    va_end(ap);
}

float clampf(float v, float lo, float hi)
{
    return fminf(fmaxf(v, lo), hi);
}
