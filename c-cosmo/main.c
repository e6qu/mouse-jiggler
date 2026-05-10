// mouse-jiggler — Actually Portable Executable build.
//
// Single APE that runs on Linux + macOS + Windows (+ BSDs) on x86_64 + arm64.
// At runtime we detect the host OS (Cosmopolitan IsLinux/IsXnu/IsWindows
// macros) and dlopen the correct native input API:
//   Linux:   libX11.so.6 + libXtst.so.6  (XTestFakeRelativeMotionEvent)
//   macOS:   CoreGraphics + CoreFoundation frameworks (CGEventPost)
//   Windows: user32.dll  (SendInput, called via Microsoft x64 ABI)

#include <cosmo.h>
#include <dlfcn.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#include "libc/dlopen/dlfcn.h"     // cosmo_dlopen / cosmo_dlsym
#include "libc/nt/thunk/msabi.h"   // __msabi

// ---------------------------------------------------------------- Config --

typedef enum { MODE_PIXEL = 0, MODE_CIRCLE, MODE_RANDOM } Mode;

typedef struct {
    Mode mode;
    long interval_ms;
    int  distance;
    long max_runtime_ms;   // -1 = unlimited
    int  once;
    int  verbose;
    int  quiet;
} Config;

static const char HELP[] =
    "mouse-jiggler — keep the cursor (and your session) alive\n"
    "\n"
    "USAGE:\n"
    "    mouse-jiggler [OPTIONS]\n"
    "\n"
    "OPTIONS:\n"
    "  -m, --mode <MODE>          pixel | circle | random   [default: pixel]\n"
    "  -i, --interval <DURATION>  time between jiggles      [default: 30s]\n"
    "  -d, --distance <PIXELS>    movement amplitude        [default: 1]\n"
    "      --max-runtime <DUR>    auto-stop after duration  [default: unlimited]\n"
    "      --once                 jiggle once and exit\n"
    "  -q, --quiet                suppress output\n"
    "  -v, --verbose              per-iteration logging\n"
    "  -h, --help / -V, --version\n"
    "\n"
    "DURATION: integer with optional s/m/h suffix (default seconds), e.g. 30s, 5m, 2h\n";

// ---------------------------------------------------------------- Common --

static void msleep(long ms) {
    if (ms <= 0) return;
    struct timespec ts;
    ts.tv_sec  =  ms / 1000;
    ts.tv_nsec = (ms % 1000) * 1000000L;
    nanosleep(&ts, NULL);
}

static long monotonic_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000L + ts.tv_nsec / 1000000L;
}

// Tiny LCG so we don't need to pull in any RNG.
static uint64_t rng_state;
static void rng_seed(void) {
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    rng_state = ((uint64_t)ts.tv_nsec) ^ ((uint64_t)ts.tv_sec << 32);
    rng_state = rng_state * 6364136223846793005ULL + 1442695040888963407ULL;
}
static uint64_t rng_next(void) {
    rng_state = rng_state * 6364136223846793005ULL + 1442695040888963407ULL;
    return rng_state;
}
static int rng_range_inclusive(int lo, int hi) {
    uint64_t span = (uint64_t)(hi - lo + 1);
    return lo + (int)(rng_next() % span);
}

// --------------------------------------------------------- Mouse interface --

typedef struct Mouse Mouse;
struct Mouse {
    int  (*move_relative)(Mouse *self, int dx, int dy);
    void (*close)(Mouse *self);
    void  *backend;
};

// =================================================== macOS  (CoreGraphics) ==

typedef struct { double x, y; } CGPoint;
typedef void *CGEventRef;
typedef void *CGEventSourceRef;

typedef CGEventRef (*Fn_CGEventCreate)(CGEventSourceRef);
typedef CGPoint    (*Fn_CGEventGetLocation)(CGEventRef);
typedef CGEventRef (*Fn_CGEventCreateMouseEvent)(CGEventSourceRef, uint32_t, CGPoint, uint32_t);
typedef void       (*Fn_CGEventPost)(uint32_t, CGEventRef);
typedef void       (*Fn_CFRelease)(void *);

typedef struct {
    Fn_CGEventCreate            CGEventCreate;
    Fn_CGEventGetLocation       CGEventGetLocation;
    Fn_CGEventCreateMouseEvent  CGEventCreateMouseEvent;
    Fn_CGEventPost              CGEventPost;
    Fn_CFRelease                CFRelease;
} MacBackend;

static int mac_move(Mouse *self, int dx, int dy) {
    MacBackend *b = (MacBackend*)self->backend;
    CGEventRef probe = b->CGEventCreate(NULL);
    if (!probe) return -1;
    CGPoint pt = b->CGEventGetLocation(probe);
    b->CFRelease(probe);
    CGPoint target;
    target.x = pt.x + (double)dx;
    target.y = pt.y + (double)dy;
    CGEventRef evt = b->CGEventCreateMouseEvent(NULL,
                                                /*kCGEventMouseMoved*/ 5,
                                                target,
                                                /*kCGMouseButtonLeft*/ 0);
    if (!evt) return -1;
    b->CGEventPost(/*kCGHIDEventTap*/ 0, evt);
    b->CFRelease(evt);
    return 0;
}

static void mac_close(Mouse *self) { free(self->backend); }

static int mac_init(Mouse *m) {
    void *cg = cosmo_dlopen(
        "/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics", RTLD_LAZY);
    void *cf = cosmo_dlopen(
        "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation", RTLD_LAZY);
    if (!cg || !cf) return -1;
    MacBackend *b = (MacBackend*)calloc(1, sizeof(*b));
    b->CGEventCreate           = (Fn_CGEventCreate)           cosmo_dlsym(cg, "CGEventCreate");
    b->CGEventGetLocation      = (Fn_CGEventGetLocation)      cosmo_dlsym(cg, "CGEventGetLocation");
    b->CGEventCreateMouseEvent = (Fn_CGEventCreateMouseEvent) cosmo_dlsym(cg, "CGEventCreateMouseEvent");
    b->CGEventPost             = (Fn_CGEventPost)             cosmo_dlsym(cg, "CGEventPost");
    b->CFRelease               = (Fn_CFRelease)               cosmo_dlsym(cf, "CFRelease");
    if (!b->CGEventCreate || !b->CGEventGetLocation || !b->CGEventCreateMouseEvent
        || !b->CGEventPost || !b->CFRelease) {
        free(b);
        return -1;
    }
    m->backend       = b;
    m->move_relative = mac_move;
    m->close         = mac_close;
    return 0;
}

// ====================================================== Linux  (X11+XTest) ==

typedef void  Display;
typedef Display *(*Fn_XOpenDisplay)(const char *);
typedef int      (*Fn_XFlush)(Display *);
typedef int      (*Fn_XCloseDisplay)(Display *);
typedef int      (*Fn_XTestFakeRelativeMotionEvent)(Display *, int, int, unsigned long);

typedef struct {
    Display                          *display;
    Fn_XFlush                         XFlush;
    Fn_XCloseDisplay                  XCloseDisplay;
    Fn_XTestFakeRelativeMotionEvent   XTestFakeRelativeMotionEvent;
} LinuxBackend;

static int linux_move(Mouse *self, int dx, int dy) {
    LinuxBackend *b = (LinuxBackend*)self->backend;
    b->XTestFakeRelativeMotionEvent(b->display, dx, dy, 0);
    b->XFlush(b->display);
    return 0;
}

static void linux_close(Mouse *self) {
    LinuxBackend *b = (LinuxBackend*)self->backend;
    if (b->XCloseDisplay && b->display) b->XCloseDisplay(b->display);
    free(b);
}

static void *try_dlopen(const char *const *names) {
    for (; *names; names++) {
        void *h = cosmo_dlopen(*names, RTLD_LAZY);
        if (h) return h;
    }
    return NULL;
}

static int linux_init(Mouse *m) {
    static const char *const x11_names[]  = { "libX11.so.6",  "libX11.so",  NULL };
    static const char *const xtst_names[] = { "libXtst.so.6", "libXtst.so", NULL };
    void *libx11  = try_dlopen(x11_names);
    void *libxtst = try_dlopen(xtst_names);
    if (!libx11 || !libxtst) return -1;

    Fn_XOpenDisplay xopen = (Fn_XOpenDisplay)cosmo_dlsym(libx11, "XOpenDisplay");
    Fn_XFlush       xflush = (Fn_XFlush)      cosmo_dlsym(libx11, "XFlush");
    Fn_XCloseDisplay xclose = (Fn_XCloseDisplay)cosmo_dlsym(libx11, "XCloseDisplay");
    Fn_XTestFakeRelativeMotionEvent xtmotion =
        (Fn_XTestFakeRelativeMotionEvent)cosmo_dlsym(libxtst, "XTestFakeRelativeMotionEvent");
    if (!xopen || !xflush || !xclose || !xtmotion) return -1;

    Display *d = xopen(NULL);
    if (!d) return -1;

    LinuxBackend *b = (LinuxBackend*)calloc(1, sizeof(*b));
    b->display                      = d;
    b->XFlush                       = xflush;
    b->XCloseDisplay                = xclose;
    b->XTestFakeRelativeMotionEvent = xtmotion;
    m->backend       = b;
    m->move_relative = linux_move;
    m->close         = linux_close;
    return 0;
}

// =================================================== Windows  (SendInput) ==

// Microsoft INPUT struct layout, x64 (and arm64 Windows): 40 bytes total.
//   DWORD type;     // offset 0
//   <pad 4 bytes>   // offset 4
//   MOUSEINPUT mi:  // offset 8 (alignment 8)
//     LONG dx;          // 8
//     LONG dy;          // 12
//     DWORD mouseData;  // 16
//     DWORD dwFlags;    // 20
//     DWORD time;       // 24
//     <pad 4 bytes>     // 28
//     ULONG_PTR extra;  // 32 (8 bytes)
typedef struct {
    uint32_t  type;
    uint32_t  _pad0;
    int32_t   dx;
    int32_t   dy;
    uint32_t  mouseData;
    uint32_t  dwFlags;
    uint32_t  time;
    uint32_t  _pad1;
    uintptr_t dwExtraInfo;
} WinINPUT;
_Static_assert(sizeof(WinINPUT) == 40, "WinINPUT must be exactly 40 bytes");

#define WIN_INPUT_MOUSE       0u
#define WIN_MOUSEEVENTF_MOVE  0x0001u

typedef __msabi uint32_t (*Fn_SendInput)(uint32_t cInputs, WinINPUT *pInputs, int32_t cbSize);

typedef struct { Fn_SendInput SendInput; } WinBackend;

static int win_move(Mouse *self, int dx, int dy) {
    WinBackend *b = (WinBackend*)self->backend;
    WinINPUT in = {0};
    in.type    = WIN_INPUT_MOUSE;
    in.dx      = dx;
    in.dy      = dy;
    in.dwFlags = WIN_MOUSEEVENTF_MOVE;
    return b->SendInput(1, &in, (int32_t)sizeof(WinINPUT)) == 1 ? 0 : -1;
}

static void win_close(Mouse *self) { free(self->backend); }

static int win_init(Mouse *m) {
    void *user32 = cosmo_dlopen("user32.dll", RTLD_LAZY);
    if (!user32) return -1;
    Fn_SendInput sendinput = (Fn_SendInput)cosmo_dlsym(user32, "SendInput");
    if (!sendinput) return -1;
    WinBackend *b = (WinBackend*)calloc(1, sizeof(*b));
    b->SendInput = sendinput;
    m->backend       = b;
    m->move_relative = win_move;
    m->close         = win_close;
    return 0;
}

// ----------------------------------------------------------- Mouse driver --

static int mouse_init(Mouse *m) {
    memset(m, 0, sizeof(*m));
    if (IsXnu())     return mac_init(m);
    if (IsLinux())   return linux_init(m);
    if (IsWindows()) return win_init(m);
    return -1;
}

// ----------------------------------------------------------------- Modes --

static int do_pixel(Mouse *m, int d) {
    if (m->move_relative(m, d, 0)) return -1;
    msleep(50);
    return m->move_relative(m, -d, 0);
}

static int do_circle(Mouse *m, int r) {
    enum { steps = 8 };
    double prev_x = 0.0, prev_y = 0.0;
    for (int i = 0; i <= steps; i++) {
        double theta = (double)i * (2.0 * M_PI) / (double)steps;
        // Center the circle at (-r, 0) so endpoints land on the cursor origin.
        double x = (double)r * cos(theta) - (double)r;
        double y = (double)r * sin(theta);
        int dx = (int)lround(x - prev_x);
        int dy = (int)lround(y - prev_y);
        if (dx || dy) {
            if (m->move_relative(m, dx, dy)) return -1;
        }
        prev_x = x;
        prev_y = y;
        msleep(20);
    }
    return 0;
}

static int do_random(Mouse *m, int d) {
    int dx = rng_range_inclusive(-d, d);
    int dy = rng_range_inclusive(-d, d);
    if (dx == 0 && dy == 0) return do_pixel(m, d);
    if (m->move_relative(m, dx, dy)) return -1;
    msleep(50);
    return m->move_relative(m, -dx, -dy);
}

static int do_jiggle(Mouse *m, const Config *cfg) {
    switch (cfg->mode) {
        case MODE_PIXEL:  return do_pixel(m, cfg->distance);
        case MODE_CIRCLE: return do_circle(m, cfg->distance);
        case MODE_RANDOM: return do_random(m, cfg->distance);
    }
    return -1;
}

// -------------------------------------------------------------- CLI parsing --

static int parse_duration_ms(const char *s, long *out) {
    if (!s || !*s) return -1;
    char *end;
    long n = strtol(s, &end, 10);
    if (end == s || n < 0) return -1;
    long mult_ms;
    if (*end == '\0' || *end == 's')      mult_ms = 1000;
    else if (*end == 'm' && end[1] == '\0') mult_ms = 60 * 1000;
    else if (*end == 'h' && end[1] == '\0') mult_ms = 3600 * 1000;
    else                                    return -1;
    if (*end != '\0' && end[1] != '\0' && *end != 's') return -1;
    *out = n * mult_ms;
    return 0;
}

// Returns 0 on success (Run), 1 for help, 2 for version, -1 on error.
static int parse_args(int argc, char **argv, Config *cfg) {
    cfg->mode           = MODE_PIXEL;
    cfg->interval_ms    = 30 * 1000;
    cfg->distance       = 1;
    cfg->max_runtime_ms = -1;
    cfg->once = cfg->verbose = cfg->quiet = 0;

    for (int i = 1; i < argc; i++) {
        char *raw = argv[i];
        char keybuf[64];
        const char *key = raw;
        const char *inline_val = NULL;
        char *eq = strchr(raw, '=');
        if (eq) {
            size_t klen = (size_t)(eq - raw);
            if (klen >= sizeof(keybuf)) {
                fprintf(stderr, "error: argument too long: %s\n", raw); return -1;
            }
            memcpy(keybuf, raw, klen); keybuf[klen] = 0;
            key = keybuf;
            inline_val = eq + 1;
        }

        #define VAL(out_var) do {                                              \
            if (inline_val) { (out_var) = inline_val; inline_val = NULL; }     \
            else if (i + 1 < argc) { (out_var) = argv[++i]; }                  \
            else { fprintf(stderr, "error: %s requires a value\n", key); return -1; } \
        } while (0)

        if (!strcmp(key, "-h") || !strcmp(key, "--help"))       return 1;
        if (!strcmp(key, "-V") || !strcmp(key, "--version"))    return 2;

        if (!strcmp(key, "-m") || !strcmp(key, "--mode")) {
            const char *v; VAL(v);
            if      (!strcmp(v, "pixel")  || !strcmp(v, "p")) cfg->mode = MODE_PIXEL;
            else if (!strcmp(v, "circle") || !strcmp(v, "c")) cfg->mode = MODE_CIRCLE;
            else if (!strcmp(v, "random") || !strcmp(v, "r")) cfg->mode = MODE_RANDOM;
            else { fprintf(stderr, "error: unknown mode: %s\n", v); return -1; }
        }
        else if (!strcmp(key, "-i") || !strcmp(key, "--interval")) {
            const char *v; VAL(v);
            if (parse_duration_ms(v, &cfg->interval_ms) < 0) {
                fprintf(stderr, "error: bad duration: %s\n", v); return -1;
            }
        }
        else if (!strcmp(key, "-d") || !strcmp(key, "--distance")) {
            const char *v; VAL(v);
            char *end; long n = strtol(v, &end, 10);
            if (*end || n < 1 || n > 10000) {
                fprintf(stderr, "error: bad distance: %s\n", v); return -1;
            }
            cfg->distance = (int)n;
        }
        else if (!strcmp(key, "--max-runtime")) {
            const char *v; VAL(v);
            if (parse_duration_ms(v, &cfg->max_runtime_ms) < 0) {
                fprintf(stderr, "error: bad duration: %s\n", v); return -1;
            }
        }
        else if (!strcmp(key, "--once"))    cfg->once    = 1;
        else if (!strcmp(key, "-q") || !strcmp(key, "--quiet"))   cfg->quiet   = 1;
        else if (!strcmp(key, "-v") || !strcmp(key, "--verbose")) cfg->verbose = 1;
        else { fprintf(stderr, "error: unknown argument: %s\n", key); return -1; }

        #undef VAL
    }

    if (cfg->quiet && cfg->verbose) {
        fprintf(stderr, "error: --quiet and --verbose are mutually exclusive\n");
        return -1;
    }
    return 0;
}

// ---------------------------------------------------------------- main --

static const char *host_name(void) {
    if (IsLinux())   return "Linux";
    if (IsXnu())     return "macOS";
    if (IsWindows()) return "Windows";
    if (IsFreebsd()) return "FreeBSD";
    if (IsOpenbsd()) return "OpenBSD";
    if (IsNetbsd())  return "NetBSD";
    return "unknown";
}

int main(int argc, char **argv) {
    Config cfg;
    int r = parse_args(argc, argv, &cfg);
    if (r == 1) { fputs(HELP, stdout); return 0; }
    if (r == 2) { puts("mouse-jiggler 0.1.0 (c-cosmo APE)"); return 0; }
    if (r != 0) { fputs(HELP, stderr);  return 2; }

    Mouse mouse;
    if (mouse_init(&mouse) != 0) {
        fprintf(stderr, "error: failed to initialize mouse driver on %s\n", host_name());
        if (IsLinux())
            fprintf(stderr, "       (Linux: needs libX11.so.6 + libXtst.so.6 — Wayland not natively supported)\n");
        else if (IsXnu())
            fprintf(stderr, "       (macOS: ensure CoreGraphics framework is reachable; you may need Accessibility/Input-Monitoring permission)\n");
        else if (IsWindows())
            fprintf(stderr, "       (Windows: failed to load user32.dll)\n");
        return 1;
    }

    rng_seed();

    if (!cfg.quiet) {
        const char *modename =
            cfg.mode == MODE_PIXEL  ? "pixel" :
            cfg.mode == MODE_CIRCLE ? "circle" : "random";
        fprintf(stderr,
            "mouse-jiggler [%s]: mode=%s interval=%lds distance=%dpx%s\n",
            host_name(), modename, cfg.interval_ms / 1000, cfg.distance,
            cfg.once ? " once=true" : "");
    }

    long start_ms = monotonic_ms();
    uint64_t iter = 0;
    for (;;) {
        if (cfg.max_runtime_ms >= 0 && (monotonic_ms() - start_ms) >= cfg.max_runtime_ms)
            break;

        if (cfg.verbose) fprintf(stderr, "[%llu] jiggle\n", (unsigned long long)iter);

        if (do_jiggle(&mouse, &cfg) != 0) {
            fprintf(stderr, "error: jiggle failed\n");
            mouse.close(&mouse);
            return 1;
        }
        iter++;
        if (cfg.once) break;
        msleep(cfg.interval_ms);
    }

    mouse.close(&mouse);
    return 0;
}
