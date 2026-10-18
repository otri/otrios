// SPDX-License-Identifier: Apache-2.0
#define _DARWIN_C_SOURCE
#include <wasmtime.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <time.h>

static double now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return (double)t.tv_sec + t.tv_nsec * 1e-9; }
static void check(wasmtime_error_t *error, wasm_trap_t *trap) {
    if (!error && !trap) return;
    wasm_byte_vec_t text;
    if (error) wasmtime_error_message(error, &text); else wasm_trap_message(trap, &text);
    fprintf(stderr, "%.*s\n", (int)text.size, text.data); exit(1);
}
static wasmtime_extern_t exported(wasmtime_context_t *context, wasmtime_instance_t *instance, const char *name, uint8_t kind) {
    wasmtime_extern_t item;
    if (!wasmtime_instance_export_get(context, instance, name, strlen(name), &item) || item.kind != kind) {
        fprintf(stderr, "missing or invalid export %s\n", name); exit(1);
    }
    return item;
}
static uint32_t call(wasmtime_context_t *context, wasmtime_func_t *func, uint32_t a, uint32_t b, uint32_t c, size_t count) {
    wasmtime_val_t args[3] = {{.kind=WASMTIME_I32,.of.i32=(int32_t)a},{.kind=WASMTIME_I32,.of.i32=(int32_t)b},{.kind=WASMTIME_I32,.of.i32=(int32_t)c}};
    wasmtime_val_t result; wasm_trap_t *trap = NULL;
    wasmtime_error_t *error = wasmtime_func_call(context, func, args, count, &result, 1, &trap);
    check(error, trap); return (uint32_t)result.of.i32;
}
int main(int argc, char **argv) {
    if (argc < 3) { fprintf(stderr, "wasm_runner compile|run|cached|startup|cached-startup INPUT [CACHE_OUTPUT]\n"); return 1; }
    double start = now();
    wasm_config_t *config = wasm_config_new();
    wasmtime_config_cranelift_opt_level_set(config, WASMTIME_OPT_LEVEL_SPEED);
    // Avoid a worker pool changing tiny-module compilation and RSS between hosts.
    wasmtime_config_parallel_compilation_set(config, false);
    wasm_engine_t *engine = wasm_engine_new_with_config(config);
    double engine_ms = (now() - start) * 1000;
    FILE *file = fopen(argv[2], "rb"); if (!file) { perror(argv[2]); return 1; }
    fseek(file, 0, SEEK_END); long length = ftell(file); rewind(file);
    if (length <= 0) return 1;
    uint8_t *bytes = malloc((size_t)length);
    if (!bytes || fread(bytes, 1, (size_t)length, file) != (size_t)length) return 1;
    fclose(file);
    wasmtime_module_t *module; start = now();
    // Cached input MUST be a trusted local output of this harness's serialize call.
    // Never deserialize an untrusted downloaded native-cache artifact.
    wasmtime_error_t *error = strstr(argv[1], "cached")
        ? wasmtime_module_deserialize(engine, bytes, (size_t)length, &module)
        : wasmtime_module_new(engine, bytes, (size_t)length, &module);
    check(error, NULL); double module_ms = (now() - start) * 1000; free(bytes);
    if (!strcmp(argv[1], "compile")) {
        if (argc != 4) return 1;
        wasm_byte_vec_t serialized; start = now();
        check(wasmtime_module_serialize(module, &serialized), NULL);
        file = fopen(argv[3], "wb"); if (!file) return 1;
        if (fwrite(serialized.data, 1, serialized.size, file) != serialized.size) return 1;
        fclose(file);
        printf("{\"engine_ms\":%.6f,\"compile_ms\":%.6f,\"serialize_write_ms\":%.6f,\"cache_bytes\":%zu}\n", engine_ms, module_ms, (now()-start)*1000, serialized.size);
        wasm_byte_vec_delete(&serialized); wasmtime_module_delete(module); wasm_engine_delete(engine); return 0;
    }
    wasmtime_store_t *store = wasmtime_store_new(engine, NULL, NULL);
    wasmtime_context_t *context = wasmtime_store_context(store);
    wasmtime_instance_t instance; wasm_trap_t *trap = NULL; start = now();
    error = wasmtime_instance_new(context, module, NULL, 0, &instance, &trap); check(error, trap);
    double instantiate_ms = (now() - start) * 1000;
    wasmtime_extern_t kernel = exported(context, &instance, "kernel", WASMTIME_EXTERN_FUNC);
    if (strstr(argv[1], "startup")) {
        printf("{\"engine_ms\":%.6f,\"module_ms\":%.6f,\"instantiate_ms\":%.6f,\"result\":%u}\n", engine_ms, module_ms, instantiate_ms, call(context, &kernel.of.func, 123, 1000, 0, 2));
    } else {
        wasmtime_extern_t scan = exported(context, &instance, "scan", WASMTIME_EXTERN_FUNC);
        wasmtime_extern_t noop = exported(context, &instance, "noop", WASMTIME_EXTERN_FUNC);
        wasmtime_extern_t memory = exported(context, &instance, "memory", WASMTIME_EXTERN_MEMORY);
        wasmtime_extern_t heap = exported(context, &instance, "__heap_base", WASMTIME_EXTERN_GLOBAL);
        wasmtime_val_t heap_value; wasmtime_global_get(context, &heap.of.global, &heap_value);
        uint32_t offset = (uint32_t)heap_value.of.i32;
        size_t needed = (size_t)offset + 65536;
        size_t current = wasmtime_memory_data_size(context, &memory.of.memory);
        if (needed > current) { uint64_t previous; check(wasmtime_memory_grow(context, &memory.of.memory, (needed-current+65535)/65536, &previous), NULL); }
        uint32_t data[16384]; for (uint32_t i=0;i<16384;++i) data[i] = i * 2654435761u;
        uint8_t *destination = wasmtime_memory_data(context, &memory.of.memory) + offset;
        memcpy(destination, data, sizeof(data));
        printf("{\"engine_ms\":%.6f,\"module_ms\":%.6f,\"instantiate_ms\":%.6f,\"linear_memory_bytes\":%zu,\"kernel_ms\":[", engine_ms, module_ms, instantiate_ms, wasmtime_memory_data_size(context, &memory.of.memory));
        uint32_t result = 0;
        for (int trial=-2;trial<7;++trial) { start=now(); result=call(context,&kernel.of.func,123,8000000,0,2); if(trial>=0) printf("%s%.6f",trial?",":"",(now()-start)*1000); }
        printf("],\"kernel_result\":%u,\"scan_ms\":[",result);
        for (int trial=-2;trial<7;++trial) { start=now(); result=call(context,&scan.of.func,offset,16384,64,3); if(trial>=0) printf("%s%.6f",trial?",":"",(now()-start)*1000); }
        printf("],\"scan_result\":%u,\"boundary_ns\":[",result);
        for(int trial=-2;trial<7;++trial) { start=now(); for(uint32_t i=0;i<1000000;++i) result=call(context,&noop.of.func,i,0,0,1); if(trial>=0) printf("%s%.6f",trial?",":"",(now()-start)*1e9/1000000); }
        struct rusage usage; getrusage(RUSAGE_SELF,&usage);
        printf("],\"boundary_result\":%u,\"maxrss_raw\":%ld}\n",result,usage.ru_maxrss);
    }
    wasmtime_store_delete(store); wasmtime_module_delete(module); wasm_engine_delete(engine); return 0;
}
