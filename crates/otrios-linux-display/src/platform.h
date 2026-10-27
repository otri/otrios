/* SPDX-License-Identifier: Apache-2.0 */
#ifndef OTRIOS_PLATFORM_H
#define OTRIOS_PLATFORM_H
#include <stdint.h>
#include <stddef.h>
typedef struct ot_display ot_display;
#define OT_MAX_SCENE_SLOTS 131u
typedef struct { uint64_t usec; uint32_t sequence, crtc; } ot_flip;
typedef struct { int32_t phase; float x, y; uint64_t age_us; } ot_touch;
typedef struct { uint32_t width,height,format,stride; int32_t red,green,blue,alpha; } ot_info;
ot_display *ot_open(const char *card,const char *input,uint32_t width,uint32_t height,int rgb565,int headless,char *error,size_t capacity);
void ot_close(ot_display *d);
const char *ot_error(ot_display *d);
void ot_get_info(ot_display *d,ot_info *info);
/* Internal ABI: Rust validates finite scalar/colour/attribute contents before
 * these synchronous calls. C validates dimensions, ranges and owned allocations. */
int ot_upload(ot_display *d,uint32_t slot,const float *vertices,size_t floats);
int ot_set_font(ot_display *d,uint32_t cw,uint32_t ch,const float *curves,size_t curve_floats,uint32_t bw,uint32_t bh,const uint8_t *bands,size_t band_bytes);
int ot_upload_text(ot_display *d,uint32_t slot,const float *vertices,size_t floats,const uint32_t *draws,size_t draw_words);
int ot_linear_scene(ot_display *d);
void ot_text_stats(ot_display *d,uint64_t *stats);
int ot_draw(ot_display *d,const float *projection,int animated);
int ot_draw_slots(ot_display *d,const float *projection,const uint32_t *slots,size_t count);
int ot_retire_slot(ot_display *d,uint32_t slot);
int ot_present(ot_display *d,ot_flip *flip);
int ot_text_styles(ot_display *d,uint32_t slot,const float *styles,size_t floats);
int ot_text_views(ot_display *d,uint32_t slot,const float *views,size_t floats);
int ot_input(ot_display *d,ot_touch *events,size_t capacity);
int ot_pixels(ot_display *d,uint8_t *rgba,size_t length);
#endif
