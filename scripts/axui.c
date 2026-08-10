// axui - find and press accessibility elements inside a WKWebView app.
//
// Why this exists instead of `osascript`/System Events: a Tauri app's UI lives in
// a WKWebView, and System Events does not cross into the webview's remote AX
// element. `entire contents of front window` returns zero named elements even
// when the tree is fully populated. A raw ApplicationServices client sees it all.
//
// WebKit also keeps the web AX tree switched off until a client sets
// AXManualAccessibility on the webview's remote element (the window's
// grandchild, not the app and not the window). We do that on every run.
//
// Must run as a trusted AX client. On tron that means via `tcc-run`.
//
//   axui <pid> list [substring]     print role/label/position of matching elements
//   axui <pid> press <substring>    press the single best match, or refuse
//
// press refuses when a substring matches more than one element, so a test never
// acts on a guess. Exit 0 pressed, 1 not found, 2 usage, 3 ambiguous, 4 untrusted.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <ApplicationServices/ApplicationServices.h>

#define MAX_HITS 256

typedef struct { AXUIElementRef el; char role[64], label[192]; double x, y; } Hit;
static Hit hits[MAX_HITS];
static int nhits, truncated;

static void str_attr(AXUIElementRef e, CFStringRef a, char *buf, size_t n) {
    buf[0] = 0;
    CFTypeRef v = NULL;
    if (AXUIElementCopyAttributeValue(e, a, &v) == kAXErrorSuccess && v) {
        if (CFGetTypeID(v) == CFStringGetTypeID()) CFStringGetCString(v, buf, n, kCFStringEncodingUTF8);
        CFRelease(v);
    }
}

// An element's usable label is whichever of title/description/value is set.
// Web content puts button text in title or description and plain text in value.
static void label_of(AXUIElementRef e, char *out, size_t n) {
    char t[192];
    str_attr(e, kAXTitleAttribute, out, n);       if (out[0]) return;
    str_attr(e, kAXDescriptionAttribute, out, n); if (out[0]) return;
    str_attr(e, kAXValueAttribute, t, sizeof(t)); snprintf(out, n, "%s", t);
}

static int center_of(AXUIElementRef e, double *x, double *y) {
    CFTypeRef pv = NULL, sv = NULL;
    CGPoint p; CGSize s;
    int ok = AXUIElementCopyAttributeValue(e, kAXPositionAttribute, &pv) == kAXErrorSuccess
          && AXUIElementCopyAttributeValue(e, kAXSizeAttribute, &sv) == kAXErrorSuccess
          && AXValueGetValue((AXValueRef)pv, kAXValueCGPointType, &p)
          && AXValueGetValue((AXValueRef)sv, kAXValueCGSizeType, &s);
    if (ok) { *x = p.x + s.width / 2; *y = p.y + s.height / 2; }
    if (pv) CFRelease(pv);
    if (sv) CFRelease(sv);
    return ok;
}

static void enable_web_ax(AXUIElementRef app) {
    CFArrayRef ws = NULL;
    if (AXUIElementCopyAttributeValue(app, kAXWindowsAttribute, (CFTypeRef *)&ws) != kAXErrorSuccess) return;
    for (CFIndex i = 0; i < CFArrayGetCount(ws); i++) {
        CFArrayRef cs = NULL;
        if (AXUIElementCopyAttributeValue((AXUIElementRef)CFArrayGetValueAtIndex(ws, i),
                                          kAXChildrenAttribute, (CFTypeRef *)&cs) != kAXErrorSuccess) continue;
        for (CFIndex j = 0; j < CFArrayGetCount(cs); j++) {
            CFArrayRef gs = NULL;
            if (AXUIElementCopyAttributeValue((AXUIElementRef)CFArrayGetValueAtIndex(cs, j),
                                              kAXChildrenAttribute, (CFTypeRef *)&gs) != kAXErrorSuccess) continue;
            for (CFIndex k = 0; k < CFArrayGetCount(gs); k++)  // the remote element is in here
                AXUIElementSetAttributeValue((AXUIElementRef)CFArrayGetValueAtIndex(gs, k),
                                             CFSTR("AXManualAccessibility"), kCFBooleanTrue);
            CFRelease(gs);
        }
        CFRelease(cs);
    }
    CFRelease(ws);
}

// Only these can be meaningfully pressed. Matching plain text would make every
// heading containing "Settings" a candidate and turn press into a coin flip.
static int pressable(const char *role) {
    return !strcmp(role, "AXButton") || !strcmp(role, "AXLink") || !strcmp(role, "AXMenuItem")
        || !strcmp(role, "AXRadioButton") || !strcmp(role, "AXCheckBox") || !strcmp(role, "AXPopUpButton")
        || !strcmp(role, "AXTab") || !strcmp(role, "AXMenuButton");
}

static void walk(AXUIElementRef e, const char *needle, int want_pressable, int depth) {
    if (depth > 40 || nhits >= MAX_HITS) { if (nhits >= MAX_HITS) truncated = 1; return; }
    char role[64], label[192];
    str_attr(e, kAXRoleAttribute, role, sizeof(role));
    label_of(e, label, sizeof(label));
    if (label[0] && (!needle || strcasestr(label, needle)) && (!want_pressable || pressable(role))) {
        Hit *h = &hits[nhits];
        h->el = e; h->x = h->y = -1;
        snprintf(h->role, sizeof(h->role), "%s", role);
        snprintf(h->label, sizeof(h->label), "%s", label);
        center_of(e, &h->x, &h->y);
        CFRetain(e);
        nhits++;
    }
    CFArrayRef ch = NULL;
    if (AXUIElementCopyAttributeValue(e, kAXChildrenAttribute, (CFTypeRef *)&ch) == kAXErrorSuccess) {
        for (CFIndex i = 0; i < CFArrayGetCount(ch); i++)
            walk((AXUIElementRef)CFArrayGetValueAtIndex(ch, i), needle, want_pressable, depth + 1);
        CFRelease(ch);
    }
}

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: axui <pid> list [substring]\n       axui <pid> press <substring>\n");
        return 2;
    }
    if (!AXIsProcessTrusted()) { fprintf(stderr, "axui: not a trusted AX client\n"); return 4; }

    AXUIElementRef app = AXUIElementCreateApplication((pid_t)atoi(argv[1]));
    if (!app) { fprintf(stderr, "axui: no process %s\n", argv[1]); return 1; }
    enable_web_ax(app);

    int press = !strcmp(argv[2], "press");
    if (press && argc < 4) { fprintf(stderr, "axui: press needs a substring\n"); return 2; }
    const char *needle = argc > 3 ? argv[3] : NULL;

    walk(app, needle, press, 0);

    if (!press) {
        for (int i = 0; i < nhits; i++)
            printf("%-16s %-40s %.0f,%.0f\n", hits[i].role, hits[i].label, hits[i].x, hits[i].y);
        if (truncated) printf("(truncated at %d)\n", MAX_HITS);
        printf("-- %d match%s\n", nhits, nhits == 1 ? "" : "es");
        return nhits ? 0 : 1;
    }

    if (nhits == 0) { fprintf(stderr, "axui: no pressable element matching '%s'\n", needle); return 1; }
    if (nhits > 1) {
        fprintf(stderr, "axui: '%s' is ambiguous, refusing to guess:\n", needle);
        for (int i = 0; i < nhits; i++) fprintf(stderr, "  %s %s\n", hits[i].role, hits[i].label);
        return 3;
    }
    AXError e = AXUIElementPerformAction(hits[0].el, kAXPressAction);
    if (e != kAXErrorSuccess) { fprintf(stderr, "axui: press failed, AXError %d\n", (int)e); return 1; }
    printf("pressed %s '%s' at %.0f,%.0f\n", hits[0].role, hits[0].label, hits[0].x, hits[0].y);
    return 0;
}
