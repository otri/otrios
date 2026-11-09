# SPDX-License-Identifier: Apache-2.0
"""Compile actual native draw helpers with a stateful GL stub; no GPU/device access.

Only Linux/EGL declarations and unrelated host functions are omitted. Structs and
all draw helper bodies come directly from platform.c, never a copied renderer.
Run with python3 scripts/test_linux_display_bindings.py (CC may select compiler).
This checks host command/state behavior, not driver correctness or frame rate.
"""
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/otrios-linux-display/src/platform.c"

PREAMBLE = r'''
#include <assert.h>
#include <math.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include "platform.h"
typedef unsigned int GLuint;
typedef int GLint, GLsizei, GLenum;
typedef ptrdiff_t GLsizeiptr;
typedef int EGLDisplay, EGLContext, EGLSurface, drmModeModeInfo, drmModeCrtc;
enum { GL_FALSE, GL_TRUE, GL_BLEND, GL_ARRAY_BUFFER, GL_ELEMENT_ARRAY_BUFFER,
       GL_FLOAT, GL_UNSIGNED_BYTE, GL_UNSIGNED_SHORT, GL_TRIANGLES,
       GL_TEXTURE0, GL_TEXTURE1, GL_TEXTURE_2D, GL_STATIC_DRAW, GL_SCISSOR_TEST };
'''

STUBS = r'''
struct attribute { int enabled,size,type,normalized,stride; float rgba[4]; GLuint vbo; uintptr_t offset; };
struct event {
    GLuint program,vbo,ibo,texture[2];
    int indexed,first,count,blend,scissor,clip[4];
    float projection[9]; struct attribute attributes[5];
};
static struct event state,events[128];
static int event_count,uses[4],binds,pointers,enables,disables,uploads,scissors,attribute_enables,attribute_disables;
static int active_texture;
static void glUseProgram(GLuint p){assert(p>0 && p<4);state.program=p;uses[p]++;}
static void glEnable(GLenum cap){assert(cap==GL_BLEND || cap==GL_SCISSOR_TEST);if(cap==GL_BLEND)state.blend=1;else state.scissor=1;enables++;}
static void glDisable(GLenum cap){assert(cap==GL_BLEND || cap==GL_SCISSOR_TEST);if(cap==GL_BLEND)state.blend=0;else state.scissor=0;disables++;}
static void glEnableVertexAttribArray(GLuint a){assert(a<5);state.attributes[a].enabled=1;attribute_enables++;}
static void glDisableVertexAttribArray(GLuint a){assert(a<5);state.attributes[a].enabled=0;attribute_disables++;}
static void glVertexAttrib4fv(GLuint a,const float *v){assert(a<5);memcpy(state.attributes[a].rgba,v,4*sizeof(float));}
static void glBindBuffer(GLenum target,GLuint b){assert(target==GL_ARRAY_BUFFER || target==GL_ELEMENT_ARRAY_BUFFER);if(target==GL_ARRAY_BUFFER)state.vbo=b;else state.ibo=b;binds++;}
static void glVertexAttribPointer(GLuint a,GLint size,GLenum type,int normalized,GLsizei stride,const void *offset){
    assert(a<5);struct attribute *v=&state.attributes[a];
    v->size=size;v->type=type;v->normalized=normalized;v->stride=stride;v->vbo=state.vbo;v->offset=(uintptr_t)offset;pointers++;
}
static void glUniformMatrix3fv(GLint location,GLsizei count,int transpose,const float *p){
    assert(location==(GLint)state.program+10 && count==1 && transpose==GL_FALSE);
    memcpy(state.projection,p,sizeof(state.projection));
}
static void record(int indexed,int first,int count){
    assert(event_count<128 && count>0);state.indexed=indexed;state.first=first;state.count=count;events[event_count++]=state;
}
static void glDrawArrays(GLenum mode,GLint first,GLsizei count){assert(mode==GL_TRIANGLES);record(0,first,count);}
static void glDrawElements(GLenum mode,GLsizei count,GLenum type,const void *offset){assert(mode==GL_TRIANGLES && type==GL_UNSIGNED_SHORT);record(1,(int)(uintptr_t)offset,count);}
static void glActiveTexture(GLenum unit){assert(unit==GL_TEXTURE0 || unit==GL_TEXTURE1);active_texture=unit-GL_TEXTURE0;}
static void glBindTexture(GLenum target,GLuint texture){assert(target==GL_TEXTURE_2D);state.texture[active_texture]=texture;}
static GLint glGetUniformLocation(GLuint program,const char *name){assert(program==2 && !strcmp(name,"coverageScale"));return 20;}
static void glUniform1f(GLint location,float value){assert(location==20 && value==1.f);}
static void glUniform2f(GLint location,float x,float y){assert((location==21 || location==22) && x>0 && y>0);}
static void glBufferData(GLenum target,GLsizeiptr size,const void *data,GLenum usage){assert(target==GL_ARRAY_BUFFER && size>0 && data && usage==GL_STATIC_DRAW);uploads++;}
static void glScissor(GLint x,GLint y,GLsizei w,GLsizei h){state.clip[0]=x;state.clip[1]=y;state.clip[2]=w;state.clip[3]=h;scissors++;}
'''

TESTS = r'''
static const float projection[9]={.02f,0,0,0,.02f,0,-1,-1,1};
static ot_display display;
static void reset(void){
    memset(&display,0,sizeof(display));memset(&state,0,sizeof(state));memset(uses,0,sizeof(uses));
    event_count=binds=pointers=enables=disables=uploads=scissors=active_texture=attribute_enables=attribute_disables=0;
    display.width=display.height=100;display.program=1;display.transform=11;
    display.text_program=2;display.text_transform=12;display.cache_program=3;display.cache_transform=13;
    display.text_curve_size=21;display.text_band_size=22;
    display.curve_width=display.curve_height=display.band_width=display.band_height=8;
    display.curve_texture=41;display.band_texture=42;display.cache_texture=43;
    for(int i=0;i<2;i++){
        display.meshes[i]=(struct mesh){.vbo=100+(GLuint)i,.count=30};
        display.text[i].vbo=200+(GLuint)i;display.text[i].cached_vbo=300+(GLuint)i;display.text[i].ibo=400+(GLuint)i;
    }
}
static void plan(uint32_t *runs,size_t words,float *views){display.text[0].draws=runs;display.text[0].draw_words=words;display.text[0].views=views;}
static void matrix(int i,float dx,float dy){
    float moved[9];memcpy(moved,projection,sizeof(moved));moved[6]+=.02f*dx;moved[7]+=.02f*dy;
    for(int j=0;j<9;j++)assert(fabsf(events[i].projection[j]-moved[j])<.000001f);
}
static void geometry(int i,int slot,int first,int count,int opaque){
    const struct event *e=&events[i];assert(e->program==1 && !e->indexed && e->vbo==100+(GLuint)slot && e->first==first && e->count==count && e->blend==!opaque);
    for(int a=0;a<5;a++)assert(e->attributes[a].enabled==(a<2));
    assert(e->attributes[0].size==2 && e->attributes[0].type==GL_FLOAT && !e->attributes[0].normalized && e->attributes[0].offset==offsetof(struct vertex,x));
    assert(e->attributes[1].size==4 && e->attributes[1].type==GL_UNSIGNED_BYTE && e->attributes[1].normalized && e->attributes[1].offset==offsetof(struct vertex,rgba));
    for(int a=0;a<2;a++)assert(e->attributes[a].stride==sizeof(struct vertex) && e->attributes[a].vbo==e->vbo);
}
static void text(int i,int cached,int first,int count){
    const struct event *e=&events[i];assert(e->program==(cached?3u:2u) && e->indexed && e->blend && e->vbo==(cached?300u:200u) && e->ibo==400 && e->first==first && e->count==count);
    assert(e->texture[0]==(cached?43u:41u));if(!cached)assert(e->texture[1]==42);
    for(int a=0;a<5;a++)assert(e->attributes[a].enabled==(cached?a<3:1));
    const int sizes[5]={2,2,4,4,4};const uintptr_t offsets[5]={0,2*sizeof(float),4*sizeof(float),8*sizeof(float),12*sizeof(float)};
    for(int a=0;a<(cached?3:5);a++){
        const struct attribute *v=&e->attributes[a];assert(v->vbo==e->vbo && v->size==(cached?(a==2?4:2):sizes[a]));
        assert(v->stride==(cached?sizeof(struct cached_vertex):16*sizeof(float)));
        assert(v->type==(cached && a==2?GL_UNSIGNED_BYTE:GL_FLOAT) && v->normalized==(cached && a==2));
        assert(v->offset==(cached?(a==2?offsetof(struct cached_vertex,rgba):a==1?offsetof(struct cached_vertex,u):0):offsets[a]));
    }
}
static void consecutive(void){
    for(int opaque=0;opaque<2;opaque++){
        reset();display.meshes[0].opaque=opaque;
        uint32_t runs[]={0,0,0, 0,3,6, 1,0,0, 0,9,0, 0,9,3, 0,12,6};plan(runs,sizeof(runs)/sizeof(*runs),NULL);
        draw_slot(&display,0,projection);
        assert(event_count==3 && uses[1]==1 && binds==1 && pointers==2 && enables==!opaque && disables==opaque);
        assert(attribute_enables==2 && attribute_disables==3);
        geometry(0,0,3,6,opaque);geometry(1,0,9,3,opaque);geometry(2,0,12,6,opaque);
        for(int i=0;i<3;i++)matrix(i,0,0);
    }
    puts("consecutive geometry: one bind for three draws; zero runs preserve bindings; opaque/blended state");
}
static void clipping(void){
    reset();uint32_t runs[]={0,0,6, 0,6,6, 0,12,3, 1,0,4, 0,15,3, 0,18,3};
    float views[]={0,3,2,4,10,20,60,80, 6,3,5,7,10,20,60,80,
        12,3,9,11,20,30,70,90, 0,4,0,0,200,200,210,210,
        0,0,0,0,0,0,0,0, 18,3,1,3,20,30,70,90};
    plan(runs,sizeof(runs)/sizeof(*runs),views);draw_slot(&display,0,projection);
    assert(event_count==5 && uses[1]==1 && binds==1 && pointers==2 && scissors==3 && !state.scissor);
    int first[]={0,6,12,15,18};float dx[]={2,5,9,0,1},dy[]={4,7,11,0,3};
    for(int i=0;i<5;i++){geometry(i,0,first[i],3,0);matrix(i,dx[i],dy[i]);assert(events[i].scissor==(i!=3));}
    assert(events[0].clip[0]==10 && events[0].clip[1]==20 && events[0].clip[2]==50 && events[0].clip[3]==60);
    assert(!memcmp(events[0].clip,events[1].clip,sizeof(state.clip)));
    assert(events[2].clip[0]==20 && events[2].clip[1]==30 && events[2].clip[2]==50 && events[2].clip[3]==60);
    assert(!memcmp(events[2].clip,events[4].clip,sizeof(state.clip)));
    puts("clipping: order, changed ranges/projections, shared/changed clips, offscreen skip and scissor reset");
}
static void transitions(void){
    for(int clipped=0;clipped<2;clipped++)for(int mode=0;mode<3;mode++){
        reset();display.cache_ready=mode!=0;display.atlas_scale=1;display.meshes[0].opaque=1;
        unsigned char valid[]={1,0,1};float vertices[12*16]={0};
        display.text[0].cached_valid=valid;display.text[0].cache_all=mode==1;
        display.text[0].vertices=vertices;display.text[0].count=12;display.text[0].raw_dirty=1;
        uint32_t runs[]={0,0,3, 1,0,12, 1,8,4, 0,3,6};
        float views[]={0,3,1,2,10,10,90,90, 0,12,3,4,10,10,90,90,
            8,4,5,6,20,20,80,80, 3,6,7,8,20,20,80,80};
        plan(runs,sizeof(runs)/sizeof(*runs),clipped?views:NULL);
        draw_slot(&display,0,projection);
        assert(uses[1]==2 && uses[2]==(mode==0?1:mode==1?0:1) && uses[3]==(mode==0?0:mode==1?1:2));
        assert(uploads==(mode==1?0:1));geometry(0,0,0,3,1);
        if(mode==2){assert(event_count==6);text(1,1,0,6);text(2,0,12,6);text(3,1,24,6);text(4,1,24,6);}
        else{assert(event_count==4);text(1,mode==1,0,18);text(2,mode==1,24,6);}
        geometry(event_count-1,0,3,6,1);
        for(int i=0;i<event_count;i++){
            int run=i==0?0:i==event_count-1?3:mode==2?(i==4?2:1):i;
            matrix(i,clipped?views[run*8+2]:0,clipped?views[run*8+3]:0);
            assert(events[i].scissor==clipped);
        }
        assert(!state.scissor);
    }
    /* Cached pixel atlas deliberately uses identity, unlike geometry/analytic. */
    reset();display.cache_ready=1;display.text[0].cache_all=1;
    uint32_t runs[]={0,0,3, 1,0,4, 0,3,3};plan(runs,9,NULL);draw_slot(&display,0,projection);
    const float identity[]={1,0,0,0,1,0,0,0,1};assert(!memcmp(events[1].projection,identity,sizeof(identity)));matrix(2,0,0);
    puts("transitions: geometry -> analytic/cached/mixed text -> geometry restores all bindings; text reuse and atlas projections");
}
static void slots(void){
    reset();uint32_t runs[]={0,0,3,0,3,3};plan(runs,6,NULL);
    display.text[1].draws=runs;display.text[1].draw_words=6;display.meshes[1].opaque=1;
    draw_slot(&display,0,projection);draw_slot(&display,1,projection);draw_slot(&display,0,projection);
    assert(uses[1]==3 && binds==3 && pointers==6 && event_count==6);
    for(int i=0;i<6;i++)geometry(i,i/2==1?1:0,(i%2)*3,3,i/2==1);
    /* Simulate arbitrary prior/cache preparation state before a fresh slot. */
    state.program=2;state.vbo=999;state.blend=0;memset(state.attributes,0,sizeof(state.attributes));
    draw_slot(&display,0,projection);assert(uses[1]==4);geometry(6,0,0,3,0);
    reset();display.meshes[0].count=0;draw_slot(&display,0,projection);assert(!event_count && !binds && !uses[1]);
    display.meshes[0].count=9;draw_slot(&display,0,projection);draw_slot(&display,0,projection);
    assert(event_count==2 && uses[1]==2 && binds==2 && pointers==4);geometry(0,0,0,9,0);geometry(1,0,0,9,0);
    puts("slots: first draw always binds, separate slots/calls reset, empty and draw-without-plan paths");
}
static void paints(void){
    for(int cached=0;cached<2;cached++){
        reset();display.cache_ready=cached;display.text[0].cache_all=cached;display.meshes[0].opaque=1;
        uint32_t runs[]={0,0,3,1,0,4,0,3,3};float rgba[]={1,0,0,.5f, 0,1,0,.7f, 0,0,1,1};
        plan(runs,9,NULL);display.text[0].paints=rgba;draw_slot(&display,0,projection);
        assert(event_count==3 && uploads==0);
        for(int i=0;i<3;i++){int attr=i==1?2:1;assert(!events[i].attributes[attr].enabled);assert(!memcmp(events[i].attributes[attr].rgba,rgba+i*4,4*sizeof(float)));}
        assert(events[0].blend && events[1].blend && !events[2].blend);
        display.text[0].paints=NULL;draw_slot(&display,0,projection);
        geometry(3,0,0,3,1);text(4,cached,0,6);geometry(5,0,3,3,1);
    }
    puts("paints: per-draw geometry/analytic/cached RGBA, alpha blend, zero uploads, legacy attributes restored");
}
int main(void){consecutive();clipping();transitions();slots();paints();return 0;}
'''


def main():
    source = SOURCE.read_text()
    # Fail loudly if these boundaries move; declarations and helper bodies are
    # compiled verbatim, so renderer changes cannot silently leave a stale copy.
    declarations = source[source.index("#define MAX_VERTICES"):source.index("static int fail(")]
    helpers = source[source.index("// Binding state belongs"):source.index("static int prepare_glyph_cache(")]
    with tempfile.TemporaryDirectory(prefix="otrios-display-bindings-") as directory:
        harness = Path(directory) / "bindings.c"
        binary = Path(directory) / "bindings"
        harness.write_text(PREAMBLE + declarations + STUBS + helpers + TESTS)
        subprocess.run(shlex.split(os.environ.get("CC", "cc")) + [
            "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
            "-I", str(SOURCE.parent), str(harness), "-lm", "-o", str(binary),
        ], check=True)
        subprocess.run([str(binary)], check=True)


if __name__ == "__main__":
    main()
