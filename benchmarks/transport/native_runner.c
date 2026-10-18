// SPDX-License-Identifier: Apache-2.0
#define _DARWIN_C_SOURCE
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <time.h>

static double now(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return (double)t.tv_sec + t.tv_nsec * 1e-9;
}
static void *symbol(void *lib, const char *name) {
    void *p = dlsym(lib, name);
    if (!p) { fprintf(stderr, "missing %s: %s\n", name, dlerror()); exit(1); }
    return p;
}
static void usage(void) { fprintf(stderr, "native_runner LIBRARY [ownership PREFIX|startup]\n"); exit(1); }
int main(int argc, char **argv) {
    if (argc < 2) usage();
    double start = now();
    void *lib = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!lib) { fprintf(stderr, "%s\n", dlerror()); return 1; }
    double load_ms = (now() - start) * 1000;
    if (argc > 2 && !strcmp(argv[2], "ownership")) {
        if (argc != 4) usage();
        char name[64];
        snprintf(name, sizeof(name), "%s_create", argv[3]);
        void *(*create)(uint32_t) = symbol(lib, name);
        snprintf(name, sizeof(name), "%s_retain", argv[3]);
        void *(*retain)(void *) = symbol(lib, name);
        snprintf(name, sizeof(name), "%s_release", argv[3]);
        void (*release)(void *) = symbol(lib, name);
        snprintf(name, sizeof(name), "%s_value", argv[3]);
        uint32_t (*value)(void *) = symbol(lib, name);
        void *p = create(123);
        printf("{\"load_ms\":%.6f,\"retain_release_ns\":[", load_ms);
        for (int trial = -2; trial < 7; ++trial) {
            start = now();
            for (int i = 0; i < 2000000; ++i) { void *q = retain(p); release(q); }
            double ns = (now() - start) * 1e9 / 2000000;
            if (trial >= 0) printf("%s%.6f", trial ? "," : "", ns);
        }
        if (value(p) != 123) abort();
        release(p);
        printf("],\"alloc_release_ns\":[");
        for (int trial = -2; trial < 7; ++trial) {
            start = now();
            for (int i = 0; i < 100000; ++i) { p = create((uint32_t)i); if (value(p) != (uint32_t)i) abort(); release(p); }
            double ns = (now() - start) * 1e9 / 100000;
            if (trial >= 0) printf("%s%.6f", trial ? "," : "", ns);
        }
        printf("]}\n"); dlclose(lib); return 0;
    }
    uint32_t (*kernel)(uint32_t, uint32_t) = symbol(lib, "kernel");
    uint32_t (*scan)(const uint32_t *, uint32_t, uint32_t) = symbol(lib, "scan");
    uint32_t (*noop)(uint32_t) = symbol(lib, "noop");
    if (argc > 2 && !strcmp(argv[2], "startup")) {
        printf("{\"load_ms\":%.6f,\"result\":%u}\n", load_ms, kernel(123, 1000));
        dlclose(lib); return 0;
    }
    uint32_t data[16384];
    for (uint32_t i = 0; i < 16384; ++i) data[i] = i * 2654435761u;
    printf("{\"load_ms\":%.6f,\"kernel_ms\":[", load_ms);
    uint32_t result = 0;
    for (int trial = -2; trial < 7; ++trial) {
        start = now(); result = kernel(123, 8000000);
        if (trial >= 0) printf("%s%.6f", trial ? "," : "", (now() - start) * 1000);
    }
    printf("],\"kernel_result\":%u,\"scan_ms\":[", result);
    for (int trial = -2; trial < 7; ++trial) {
        start = now(); result = scan(data, 16384, 64);
        if (trial >= 0) printf("%s%.6f", trial ? "," : "", (now() - start) * 1000);
    }
    printf("],\"scan_result\":%u,\"boundary_ns\":[", result);
    for (int trial = -2; trial < 7; ++trial) {
        start = now();
        for (uint32_t i = 0; i < 1000000; ++i) result = noop(i);
        if (trial >= 0) printf("%s%.6f", trial ? "," : "", (now() - start) * 1e9 / 1000000);
    }
    struct rusage usage; getrusage(RUSAGE_SELF, &usage);
    printf("],\"boundary_result\":%u,\"maxrss_raw\":%ld}\n", result, usage.ru_maxrss);
    dlclose(lib); return 0;
}
