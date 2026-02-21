/*
 * main.c — Thin binary shim for Rustthon.
 *
 * This is the entire `rustthon` executable. It dlopen's librustthon.dylib
 * and calls rustthon_main(). This ensures there is exactly ONE copy of all
 * global state (type objects, singletons, etc.), eliminating the binary/dylib
 * split-brain problem that occurs when Rust compiles both a binary and a
 * cdylib from the same source.
 *
 * Build: cc -o rustthon csrc/main.c -ldl
 */

#include <stdio.h>
#include <stdlib.h>
#include <dlfcn.h>
#include <string.h>

typedef int (*rustthon_main_fn)(int argc, const char **argv);

int main(int argc, const char **argv) {
    /* Try to find librustthon.dylib in several locations */
    const char *candidates[] = {
        NULL, /* filled in below: same dir as executable */
        "librustthon.dylib",
        "target/release/librustthon.dylib",
        "target/debug/librustthon.dylib",
    };
    int ncandidates = sizeof(candidates) / sizeof(candidates[0]);

    /* Build path relative to executable */
    char exe_dir_path[4096] = {0};
    {
        /* On macOS, argv[0] or _NSGetExecutablePath could give us the path.
         * For simplicity, use argv[0] dirname. */
        const char *slash = strrchr(argv[0], '/');
        if (slash) {
            size_t dirlen = (size_t)(slash - argv[0]);
            if (dirlen < sizeof(exe_dir_path) - 32) {
                memcpy(exe_dir_path, argv[0], dirlen);
                strcat(exe_dir_path, "/librustthon.dylib");
                candidates[0] = exe_dir_path;
            }
        }
    }

    void *handle = NULL;
    for (int i = 0; i < ncandidates; i++) {
        if (!candidates[i]) continue;
        handle = dlopen(candidates[i], RTLD_NOW | RTLD_GLOBAL);
        if (handle) break;
    }

    if (!handle) {
        fprintf(stderr, "Fatal: cannot find librustthon.dylib\n");
        fprintf(stderr, "Searched:\n");
        for (int i = 0; i < ncandidates; i++) {
            if (candidates[i])
                fprintf(stderr, "  %s\n", candidates[i]);
        }
        fprintf(stderr, "Last error: %s\n", dlerror());
        return 1;
    }

    rustthon_main_fn entry = (rustthon_main_fn)dlsym(handle, "rustthon_main");
    if (!entry) {
        fprintf(stderr, "Fatal: librustthon.dylib has no rustthon_main symbol: %s\n",
                dlerror());
        dlclose(handle);
        return 1;
    }

    return entry(argc, argv);
}
