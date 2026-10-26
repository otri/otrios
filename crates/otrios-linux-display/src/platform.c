/* SPDX-License-Identifier: Apache-2.0
 * Owned, single-threaded Linux display adapter. See RFC 0001. All Rust buffers
 * are borrowed only for the duration of the call. No callbacks enter Rust.
 */
#define _GNU_SOURCE
#include "platform.h"
#include "slug_shaders.h"
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES2/gl2.h>
#include <xf86drm.h>
#include <xf86drmMode.h>
#include <drm_fourcc.h>
#include <gbm.h>
#include <linux/input.h>
#include <sys/ioctl.h>
#include <poll.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <math.h>
#include <time.h>
#include <stddef.h>
#define MAX_VERTICES 2000000u
#define MAX_BO 16
struct vertex { float x,y; uint8_t rgba[4]; };
struct cached_vertex { float x,y,u,v; uint8_t rgba[4]; };
struct mesh { GLuint vbo; GLsizei count; int opaque; };
#define CACHE_SIZE 1024
#define CACHE_ENTRIES 4096
#define CACHE_BUCKETS 8192
struct text_mesh { GLuint vbo,ibo,cached_vbo; GLsizei count; uint32_t *draws; size_t draw_words; float *vertices,*views; struct cached_vertex *cached_vertices; unsigned char *cached_valid; size_t vertex_capacity,cached_capacity,valid_capacity; uint64_t cached_generation; float cached_projection[9]; size_t index_capacity; int raw_dirty,cache_all; };
struct glyph_entry { float key[24],uv_scale[2],uv_bias[2]; int x,y,w,h,next; };
struct framebuffer { struct gbm_bo *bo; uint32_t id; };
struct ot_display {
    int fd,input,headless,modeset,pending,linear_scene,atlas_scale;
    uint32_t connector,crtc,width,height,format;
    drmModeModeInfo mode;
    drmModeCrtc *saved;
    uint32_t *saved_connectors; int saved_count;
    struct gbm_device *gbm;
    struct gbm_surface *window;
    struct gbm_bo *front,*next;
    struct framebuffer fb[MAX_BO]; size_t fb_count;
    EGLDisplay egl; EGLContext context; EGLSurface surface;
    GLuint program; GLint transform;
    struct mesh meshes[3];
    struct text_mesh text[3];
    GLuint text_program,curve_texture,band_texture;
    GLint text_transform,text_curve_size,text_band_size;
    uint32_t curve_width,curve_height,band_width,band_height;
    uint64_t text_hits,text_misses,cache_generation;
    int profile_text,stable_text;uint64_t profile_keys_ns,profile_upload_ns,profile_recent;
    GLuint scene_fbo,scene_texture,resolve_program,resolve_vbo,encode_texture;
    GLuint cache_fbo,cache_texture,cache_program,cache_upload;
    GLint cache_sampler,cache_transform;
    float sampled_pixel_transform[4];
    struct glyph_entry *glyph_cache;size_t glyph_cache_count;
    int glyph_buckets[CACHE_BUCKETS];
    int glyph_recent[CACHE_ENTRIES];
    int cache_x,cache_y,cache_row,cache_ready,cache_dirty;
    ot_info info; ot_flip flip;
    struct input_absinfo xaxis,yaxis;
    int x,y,down,reported,slot,dropped,suppressed,contact;
    char error[512];
};
static int fail(ot_display *d,const char *what) { snprintf(d->error,sizeof(d->error),"%s (errno=%d, EGL=0x%x)",what,errno,eglGetError()); return -1; }
const char *ot_error(ot_display *d) { return d->error; }
static int gl_ok(ot_display *d,const char *what) { return glGetError()==GL_NO_ERROR?0:fail(d,what); }
static GLuint shader(ot_display *d,GLenum kind,const char *source) {
    GLuint s=glCreateShader(kind); glShaderSource(s,1,&source,NULL); glCompileShader(s);
    GLint ok=0; glGetShaderiv(s,GL_COMPILE_STATUS,&ok);
    if(!ok) { glGetShaderInfoLog(s,sizeof(d->error),NULL,d->error); glDeleteShader(s); return 0; } return s;
}
static int init_color_target(ot_display *d);
static int init_gl(ot_display *d) {
    const char *vs="attribute vec2 position;attribute vec4 color;uniform mat3 transform;uniform float linearTint;varying mediump vec4 rgba;void main(){gl_Position=vec4((transform*vec3(position,1.0)).xy,0.0,1.0);vec3 c=color.rgb;if(linearTint>0.5)c=mix(pow((color.rgb+0.055)/1.055,vec3(2.4)),color.rgb/12.92,step(color.rgb,vec3(0.04045)));rgba=vec4(c*color.a,color.a);}";
    const char *fs="precision mediump float;varying mediump vec4 rgba;void main(){gl_FragColor=rgba;}";
    GLuint a=shader(d,GL_VERTEX_SHADER,vs); if(!a)return -1;
    GLuint b=shader(d,GL_FRAGMENT_SHADER,fs); if(!b){glDeleteShader(a);return -1;}
    d->program=glCreateProgram(); glAttachShader(d->program,a); glAttachShader(d->program,b);
    glBindAttribLocation(d->program,0,"position"); glBindAttribLocation(d->program,1,"color");
    glLinkProgram(d->program); glDeleteShader(a); glDeleteShader(b);
    GLint ok=0; glGetProgramiv(d->program,GL_LINK_STATUS,&ok);
    if(!ok){glGetProgramInfoLog(d->program,sizeof(d->error),NULL,d->error);return -1;}
    glUseProgram(d->program); d->transform=glGetUniformLocation(d->program,"transform");
    glUniform1f(glGetUniformLocation(d->program,"linearTint"),d->linear_scene?1.f:0.f);
    glViewport(0,0,d->width,d->height); glDisable(GL_DEPTH_TEST); glDisable(GL_CULL_FACE); glDisable(GL_DITHER);
    glBlendFuncSeparate(GL_ONE,GL_ONE_MINUS_SRC_ALPHA,GL_ONE,GL_ONE_MINUS_SRC_ALPHA);
    for(int i=0;i<3;i++) {
        glGenBuffers(1,&d->meshes[i].vbo);
        glGenBuffers(1,&d->text[i].vbo);
        glGenBuffers(1,&d->text[i].ibo);
        glGenBuffers(1,&d->text[i].cached_vbo);
    }
    if(d->linear_scene && init_color_target(d)<0)return -1;
    return gl_ok(d,"GLES2 initialization");
}
static int select_display(ot_display *d) {
    drmModeRes *res=drmModeGetResources(d->fd); if(!res)return fail(d,"DRM resources");
    for(int i=0;i<res->count_connectors && !d->connector;i++) {
        drmModeConnector *c=drmModeGetConnector(d->fd,res->connectors[i]); if(!c)continue;
        if(c->connection==DRM_MODE_CONNECTED) {
            for(int j=0;j<c->count_modes;j++)if(c->modes[j].hdisplay==d->width && c->modes[j].vdisplay==d->height) {
                for(int k=0;k<c->count_encoders && !d->crtc;k++) {
                    drmModeEncoder *e=drmModeGetEncoder(d->fd,c->encoders[k]); if(!e)continue;
                    if(e->crtc_id)d->crtc=e->crtc_id;
                    else for(int n=0;n<res->count_crtcs;n++)if(e->possible_crtcs & (1u<<n)){d->crtc=res->crtcs[n];break;}
                    drmModeFreeEncoder(e);
                }
                if(d->crtc){d->connector=c->connector_id;d->mode=c->modes[j];break;}
            }
        }
        drmModeFreeConnector(c);
    }
    if(d->crtc) {
        d->saved=drmModeGetCrtc(d->fd,d->crtc);
        d->saved_connectors=calloc((size_t)res->count_connectors,sizeof(uint32_t));
        if(!d->saved_connectors){drmModeFreeResources(res);return fail(d,"Save connector allocation");}
        for(int i=0;i<res->count_connectors;i++) {
            drmModeConnector *c=drmModeGetConnector(d->fd,res->connectors[i]); if(!c)continue;
            drmModeEncoder *e=c->encoder_id?drmModeGetEncoder(d->fd,c->encoder_id):NULL;
            if(e && e->crtc_id==d->crtc)d->saved_connectors[d->saved_count++]=c->connector_id;
            if(e)drmModeFreeEncoder(e);
            drmModeFreeConnector(c);
        }
    }
    drmModeFreeResources(res);
    if(!d->connector || !d->saved)return fail(d,"No matching connected display mode");
    return 0;
}
static int input_open(ot_display *d,const char *path) {
    d->input=open(path,O_RDONLY|O_NONBLOCK|O_CLOEXEC); if(d->input<0)return fail(d,"Open configured touch device");
    if(ioctl(d->input,EVIOCGABS(ABS_X),&d->xaxis)<0 || ioctl(d->input,EVIOCGABS(ABS_Y),&d->yaxis)<0)
        return fail(d,"Touch requires ABS_X/ABS_Y");
    if(d->xaxis.maximum<=d->xaxis.minimum || d->yaxis.maximum<=d->yaxis.minimum)return fail(d,"Invalid touch axes");
    unsigned long keys[(KEY_MAX+8*sizeof(unsigned long))/(8*sizeof(unsigned long))]={0};
    if(ioctl(d->input,EVIOCGKEY(sizeof(keys)),keys)<0)return fail(d,"Read current touch contact");
    d->suppressed=(keys[BTN_TOUCH/(8*sizeof(unsigned long))]>>(BTN_TOUCH%(8*sizeof(unsigned long))))&1;
    return 0;
}
ot_display *ot_open(const char *card,const char *input,uint32_t width,uint32_t height,int rgb565,int headless,char *error,size_t capacity) {
    ot_display *d=calloc(1,sizeof(*d)); if(!d){snprintf(error,capacity,"Display allocation failed");return NULL;}
    const char *linear=getenv("OTRIOS_LINEAR_SCENE");d->linear_scene=linear && !strcmp(linear,"1");
    d->fd=d->input=-1;d->egl=EGL_NO_DISPLAY;d->context=EGL_NO_CONTEXT;d->surface=EGL_NO_SURFACE;
    d->profile_text=getenv("OTRIOS_TEXT_PROFILE")!=NULL;
    const char *stable=getenv("OTRIOS_TEXT_STABLE_QUADS");d->stable_text=stable && !strcmp(stable,"1");
    const char *sampling=getenv("OTRIOS_TEXT_ATLAS_SCALE");
    d->atlas_scale=sampling?atoi(sampling):2;
    if(d->atlas_scale!=0 && d->atlas_scale!=2 && d->atlas_scale!=3){snprintf(error,capacity,"Text atlas scale must be 0 (exact reference), 2 or 3");free(d);return NULL;}
    d->width=width;d->height=height;d->headless=headless;d->format=rgb565?GBM_FORMAT_RGB565:GBM_FORMAT_XRGB8888;
    if(!width || !height || width>4096 || height>4096){fail(d,"Invalid display dimensions");goto bad;}
    if(!headless) {
        d->fd=open(card,O_RDWR|O_CLOEXEC);if(d->fd<0){fail(d,"Open DRM device");goto bad;}
        if(drmSetMaster(d->fd)<0 || !drmIsMaster(d->fd)){fail(d,"Acquire DRM master");goto bad;}
        if(select_display(d)<0 || input_open(d,input)<0)goto bad;
        d->gbm=gbm_create_device(d->fd);if(!d->gbm){fail(d,"Create GBM device");goto bad;}
        d->window=gbm_surface_create(d->gbm,width,height,d->format,GBM_BO_USE_SCANOUT|GBM_BO_USE_RENDERING);
        if(!d->window){fail(d,"Create scanout GBM surface");goto bad;}
    }
    PFNEGLGETPLATFORMDISPLAYEXTPROC get_platform=(PFNEGLGETPLATFORMDISPLAYEXTPROC)eglGetProcAddress("eglGetPlatformDisplayEXT");
    if(!get_platform){fail(d,"EGL platform display unavailable");goto bad;}
    d->egl=get_platform(headless?EGL_PLATFORM_SURFACELESS_MESA:EGL_PLATFORM_GBM_KHR,d->gbm,NULL);
    if(d->egl==EGL_NO_DISPLAY || !eglInitialize(d->egl,NULL,NULL) || !eglBindAPI(EGL_OPENGL_ES_API)){fail(d,"Initialize GBM EGL GLES API");goto bad;}
    EGLint attrs[]={EGL_SURFACE_TYPE,headless?EGL_PBUFFER_BIT:EGL_WINDOW_BIT,EGL_RENDERABLE_TYPE,EGL_OPENGL_ES2_BIT,EGL_RED_SIZE,rgb565?5:8,EGL_GREEN_SIZE,rgb565?6:8,EGL_BLUE_SIZE,rgb565?5:8,EGL_ALPHA_SIZE,0,EGL_DEPTH_SIZE,0,EGL_STENCIL_SIZE,0,EGL_NONE};
    EGLConfig configs[128],config=NULL;EGLint count=0;
    if(!eglChooseConfig(d->egl,attrs,configs,128,&count)){fail(d,"Enumerate EGL configs");goto bad;}
    for(int i=0;i<count;i++) {
        EGLint visual=0,r=0,g=0,b=0;eglGetConfigAttrib(d->egl,configs[i],EGL_NATIVE_VISUAL_ID,&visual);
        eglGetConfigAttrib(d->egl,configs[i],EGL_RED_SIZE,&r);eglGetConfigAttrib(d->egl,configs[i],EGL_GREEN_SIZE,&g);eglGetConfigAttrib(d->egl,configs[i],EGL_BLUE_SIZE,&b);
        if((headless || (uint32_t)visual==d->format) && r==(rgb565?5:8) && g==(rgb565?6:8) && b==(rgb565?5:8)){config=configs[i];break;}
    }
    if(!config){fail(d,"No EGL config matches requested GBM colour format");goto bad;}
    EGLint ctx[]={EGL_CONTEXT_CLIENT_VERSION,2,EGL_NONE};d->context=eglCreateContext(d->egl,config,EGL_NO_CONTEXT,ctx);
    EGLint pb[]={EGL_WIDTH,(EGLint)width,EGL_HEIGHT,(EGLint)height,EGL_NONE};
    d->surface=headless?eglCreatePbufferSurface(d->egl,config,pb):eglCreateWindowSurface(d->egl,config,(EGLNativeWindowType)d->window,NULL);
    if(d->context==EGL_NO_CONTEXT || d->surface==EGL_NO_SURFACE || !eglMakeCurrent(d->egl,d->surface,d->surface,d->context)){fail(d,"Create GLES2 window context");goto bad;}
    if(!eglSwapInterval(d->egl,0)){fail(d,"Disable implicit EGL pacing");goto bad;}
    d->info=(ot_info){width,height,d->format,0,0,0,0,0};
    eglGetConfigAttrib(d->egl,config,EGL_RED_SIZE,&d->info.red);eglGetConfigAttrib(d->egl,config,EGL_GREEN_SIZE,&d->info.green);eglGetConfigAttrib(d->egl,config,EGL_BLUE_SIZE,&d->info.blue);eglGetConfigAttrib(d->egl,config,EGL_ALPHA_SIZE,&d->info.alpha);
    if(init_gl(d)<0)goto bad;
    if(d->profile_text)fprintf(stderr,"Text probe GL renderer: %s\n",glGetString(GL_RENDERER));
    return d;
bad:
    snprintf(error,capacity,"%s",d->error);ot_close(d);return NULL;
}
void ot_get_info(ot_display *d,ot_info *info){*info=d->info;}
int ot_upload(ot_display *d,uint32_t slot,const float *vertices,size_t floats) {
    if(slot>=3 || floats%18 || floats/6>MAX_VERTICES)return fail(d,"Invalid mesh length");
    size_t count=floats/6;
    struct vertex *packed=count?malloc(count*sizeof(*packed)):NULL;
    if(count && !packed)return fail(d,"Mesh packing allocation");
    int opaque=1;
    for(size_t i=0;i<count;i++) {
        const float *v=vertices+i*6;
        packed[i].x=v[0];packed[i].y=v[1];
        for(int k=0;k<4;k++)packed[i].rgba[k]=(uint8_t)(v[k+2]*255.f+0.5f);
        if(packed[i].rgba[3]!=255)opaque=0;
    }
    glBindBuffer(GL_ARRAY_BUFFER,d->meshes[slot].vbo);
    glBufferData(GL_ARRAY_BUFFER,(GLsizeiptr)(count*sizeof(*packed)),packed,slot==1?GL_STREAM_DRAW:GL_STATIC_DRAW);
    free(d->text[slot].views);d->text[slot].views=NULL;free(d->text[slot].draws);d->text[slot].draws=NULL;d->text[slot].draw_words=0;d->text[slot].count=0;
    free(packed);d->meshes[slot].count=(GLsizei)count;d->meshes[slot].opaque=opaque;
    return gl_ok(d,"Upload retained vertex buffer");
}
static uint16_t to_half(float value) {
    uint32_t bits;memcpy(&bits,&value,sizeof(bits));
    uint32_t sign=(bits>>16)&0x8000u,mantissa=bits&0x7fffffu;
    int exponent=(int)((bits>>23)&255u)-127+15;
    if(exponent<=0) {
        if(exponent < -10)return (uint16_t)sign;
        mantissa|=0x800000u;
        uint32_t shift=(uint32_t)(14-exponent);
        return (uint16_t)(sign+((mantissa+(1u<<(shift-1)))>>shift));
    }
    uint32_t rounded=(mantissa+0x1000u)>>13;
    if(rounded==0x400u){rounded=0;exponent++;}
    return (uint16_t)(sign|((uint32_t)exponent<<10)|rounded);
}
static int text_program(ot_display *d) {
    if(d->text_program)return 0;
    const char *extensions=(const char*)glGetString(GL_EXTENSIONS);
    GLint range[2],precision;glGetShaderPrecisionFormat(GL_FRAGMENT_SHADER,GL_HIGH_FLOAT,range,&precision);
    if(!extensions || !strstr(extensions,"GL_OES_texture_half_float") || !strstr(extensions,"GL_OES_standard_derivatives") || precision<16)
        return fail(d,"Slug requires half-float textures, derivatives and fragment highp");
    GLuint a=shader(d,GL_VERTEX_SHADER,ot_slug_vs);if(!a)return -1;
    GLuint b=shader(d,GL_FRAGMENT_SHADER,ot_slug_fs);if(!b){glDeleteShader(a);return -1;}
    GLuint program=glCreateProgram();glAttachShader(program,a);glAttachShader(program,b);
    const char *attributes[]={"position","renderCoord","color","bandTransform","glyph"};
    for(GLuint i=0;i<5;i++)glBindAttribLocation(program,i,attributes[i]);
    glLinkProgram(program);glDeleteShader(a);glDeleteShader(b);
    GLint ok=0;glGetProgramiv(program,GL_LINK_STATUS,&ok);
    if(!ok){glGetProgramInfoLog(program,sizeof(d->error),NULL,d->error);glDeleteProgram(program);return -1;}
    d->text_program=program;d->text_transform=glGetUniformLocation(program,"transform");
    d->text_curve_size=glGetUniformLocation(program,"curveSize");d->text_band_size=glGetUniformLocation(program,"bandSize");
    glUseProgram(program);glUniform1f(glGetUniformLocation(program,"linearTint"),d->linear_scene?1.f:0.f);glUniform1i(glGetUniformLocation(program,"curveTexture"),0);glUniform1i(glGetUniformLocation(program,"bandTexture"),1);
    return 0;
}
static void texture_parameters(void) {
    glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MIN_FILTER,GL_NEAREST);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MAG_FILTER,GL_NEAREST);
    glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_S,GL_CLAMP_TO_EDGE);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_T,GL_CLAMP_TO_EDGE);
}
int ot_set_font(ot_display *d,uint32_t cw,uint32_t ch,const float *curves,size_t cf,uint32_t bw,uint32_t bh,const uint8_t *bands,size_t bb) {
    if(!cw || !ch || !bw || !bh || cw>4096 || ch>4096 || bw>4096 || bh>4096 || (uint64_t)cw*ch>65536 || (uint64_t)bw*bh>65536 || cf!=(size_t)cw*ch*4 || bb!=(size_t)bw*bh*4)
        return fail(d,"Invalid Slug font texture dimensions");
    uint16_t *half=malloc(cf*sizeof(*half));if(!half)return fail(d,"Font texture staging allocation");
    for(size_t i=0;i<cf;i++) {
        if(!isfinite(curves[i]) || fabsf(curves[i])>4.f){free(half);return fail(d,"Font coordinate outside bounded em range");}
        half[i]=to_half(curves[i]);
    }
    if(text_program(d)<0){free(half);return -1;}
    GLuint textures[2];glGenTextures(2,textures);glPixelStorei(GL_UNPACK_ALIGNMENT,1);
    glActiveTexture(GL_TEXTURE0);glBindTexture(GL_TEXTURE_2D,textures[0]);texture_parameters();
    glTexImage2D(GL_TEXTURE_2D,0,GL_RGBA,(GLsizei)cw,(GLsizei)ch,0,GL_RGBA,0x8D61,half);free(half);
    glActiveTexture(GL_TEXTURE1);glBindTexture(GL_TEXTURE_2D,textures[1]);texture_parameters();
    glTexImage2D(GL_TEXTURE_2D,0,GL_RGBA,(GLsizei)bw,(GLsizei)bh,0,GL_RGBA,GL_UNSIGNED_BYTE,bands);
    if(gl_ok(d,"Upload Slug font textures")<0){glDeleteTextures(2,textures);return -1;}
    if(d->curve_texture)glDeleteTextures(1,&d->curve_texture);
    if(d->band_texture)glDeleteTextures(1,&d->band_texture);
    d->cache_generation++;d->cache_dirty=1;d->glyph_cache_count=0;d->cache_x=d->cache_y=d->cache_row=0;
    d->curve_texture=textures[0];d->band_texture=textures[1];d->curve_width=cw;d->curve_height=ch;d->band_width=bw;d->band_height=bh;
    return 0;
}
int ot_upload_text(ot_display *d,uint32_t slot,const float *vertices,size_t floats,const uint32_t *draws,size_t words) {
    if(slot>=3 || floats%64 || floats/16>65536 || words%3 || words>12288)return fail(d,"Invalid indexed text mesh dimensions");
    size_t count=floats/16;
    // Scalar contents are validated once by the safe Rust wrapper before FFI.
    // Keep all C-side dimensions, ordered ranges and allocation checks here.
    for(size_t i=0;i<words;i+=3) {
        uint32_t kind=draws[i],first=draws[i+1],length=draws[i+2];
        size_t limit=kind==0?(size_t)d->meshes[slot].count:count;
        if(kind>1 || first>limit || length>limit-first || (kind==1 && (first%4 || length%4)) || (kind==0 && (first%3 || length%3)))return fail(d,"Invalid ordered scene draw range");
    }
    struct text_mesh *m=&d->text[slot];
    int grow=count>m->index_capacity;
    if(floats>m->vertex_capacity){
        float *next=realloc(m->vertices,floats*sizeof(*next));
        if(!next)return fail(d,"Text staging allocation");
        m->vertices=next;m->vertex_capacity=floats;
    }
    uint16_t *indices=(grow && count)?malloc(count/4*6*sizeof(*indices)):NULL;
    uint32_t *plan=words?malloc(words*sizeof(*plan)):NULL;
    if((grow && count && !indices)||(words && !plan)){free(indices);free(plan);return fail(d,"Text staging allocation");}
    if(floats)memcpy(m->vertices,vertices,floats*sizeof(*vertices));
    if(grow)for(size_t q=0;q<count/4;q++) {
        uint16_t base=(uint16_t)(q*4);uint16_t *ix=indices+q*6;
        ix[0]=base;ix[1]=base+1;ix[2]=base+2;ix[3]=base+2;ix[4]=base+1;ix[5]=base+3;
    }
    if(words)memcpy(plan,draws,words*sizeof(*plan));
    // Cached draws consume only their compact UV buffer. Defer uploading the
    // larger analytic stream until a fallback actually needs it; indices grow once.
    m->raw_dirty=1;
    if(grow){glBindBuffer(GL_ELEMENT_ARRAY_BUFFER,m->ibo);glBufferData(GL_ELEMENT_ARRAY_BUFFER,(GLsizeiptr)(count/4*6*sizeof(*indices)),indices,GL_STATIC_DRAW);m->index_capacity=count;}
    free(indices);m->cached_generation=0;free(m->draws);m->draws=plan;m->draw_words=words;m->count=(GLsizei)count;
    return gl_ok(d,"Upload retained glyph quads");
}
int ot_linear_scene(ot_display *d) { return d->linear_scene; }
int ot_text_views(ot_display *d,uint32_t slot,const float *views,size_t floats) {
    if(slot>=3)return fail(d,"Invalid text viewport slot");
    struct text_mesh *m=&d->text[slot];
    if(!floats){if(m->views && d->stable_text)m->cached_generation=0;free(m->views);m->views=NULL;return 0;}
    if(floats!=m->draw_words/3*8)return fail(d,"Invalid text viewport dimensions");
    for(size_t i=0;i<m->draw_words/3;i++)if(m->draws[i*3]==1){
        const float *v=views+i*8;
        if(v[0]<0 || v[1]<0 || v[0]>65536 || v[1]>65536)return fail(d,"Invalid culled text range");
        uint32_t first=(uint32_t)v[0],count=(uint32_t)v[1];
        uint32_t begin=m->draws[i*3+1],end=begin+m->draws[i*3+2];
        if(v[0]<0 || v[1]<0 || v[0]!=(float)first || v[1]!=(float)count || first%4 || count%4 || first<begin || first>end || count>end-first)return fail(d,"Invalid culled text range");
    }
    if(!m->views){m->views=malloc(floats*sizeof(float));if(!m->views)return fail(d,"Text viewport allocation");if(d->stable_text)m->cached_generation=0;}
    memcpy(m->views,views,floats*sizeof(float));return 0;
}
void ot_text_stats(ot_display *d,uint64_t *stats) { stats[0]=d->text_hits;stats[1]=d->text_misses;stats[2]=d->cache_texture?(uint64_t)CACHE_SIZE*CACHE_SIZE*4:0; }
static void draw_geometry(ot_display *d,struct mesh *m,const float *projection,GLint first,GLsizei count) {
    if(!count)return;
    glUseProgram(d->program);glUniformMatrix3fv(d->transform,1,GL_FALSE,projection);
    if(m->opaque)glDisable(GL_BLEND);else glEnable(GL_BLEND);
    glEnableVertexAttribArray(0);glEnableVertexAttribArray(1);
    for(int i=2;i<5;i++)glDisableVertexAttribArray((GLuint)i);
    glBindBuffer(GL_ARRAY_BUFFER,m->vbo);
    glVertexAttribPointer(0,2,GL_FLOAT,GL_FALSE,sizeof(struct vertex),(void*)offsetof(struct vertex,x));
    glVertexAttribPointer(1,4,GL_UNSIGNED_BYTE,GL_TRUE,sizeof(struct vertex),(void*)offsetof(struct vertex,rgba));
    glDrawArrays(GL_TRIANGLES,first,count);
}
static void draw_text_part(ot_display *d,struct text_mesh *m,const float *projection,uint32_t first,uint32_t count,int cached,int rebind) {
    if(!count)return;
    if(cached) {
        if(rebind){
        glUseProgram(d->cache_program);glEnable(GL_BLEND);
        glActiveTexture(GL_TEXTURE0);glBindTexture(GL_TEXTURE_2D,d->cache_texture);
        glBindBuffer(GL_ARRAY_BUFFER,m->cached_vbo);glBindBuffer(GL_ELEMENT_ARRAY_BUFFER,m->ibo);
        for(GLuint i=0;i<3;i++)glEnableVertexAttribArray(i);
        glDisableVertexAttribArray(3);glDisableVertexAttribArray(4);
        glVertexAttribPointer(0,2,GL_FLOAT,GL_FALSE,sizeof(struct cached_vertex),(void*)offsetof(struct cached_vertex,x));
        glVertexAttribPointer(1,2,GL_FLOAT,GL_FALSE,sizeof(struct cached_vertex),(void*)offsetof(struct cached_vertex,u));
        glVertexAttribPointer(2,4,GL_UNSIGNED_BYTE,GL_TRUE,sizeof(struct cached_vertex),(void*)offsetof(struct cached_vertex,rgba));
        }
        const float identity[]={1,0,0,0,1,0,0,0,1};
        glUniformMatrix3fv(d->cache_transform,1,GL_FALSE,d->atlas_scale?projection:identity);
        glDrawElements(GL_TRIANGLES,(GLsizei)(count/4*6),GL_UNSIGNED_SHORT,(void*)((size_t)first/4*6*sizeof(uint16_t)));
        return;
    }
    if(rebind){
    glUseProgram(d->text_program);
    glUniform1f(glGetUniformLocation(d->text_program,"coverageScale"),1.f);
    glUniform2f(d->text_curve_size,(float)d->curve_width,(float)d->curve_height);glUniform2f(d->text_band_size,(float)d->band_width,(float)d->band_height);
    glActiveTexture(GL_TEXTURE0);glBindTexture(GL_TEXTURE_2D,d->curve_texture);glActiveTexture(GL_TEXTURE1);glBindTexture(GL_TEXTURE_2D,d->band_texture);
    glEnable(GL_BLEND);glBindBuffer(GL_ARRAY_BUFFER,m->vbo);glBindBuffer(GL_ELEMENT_ARRAY_BUFFER,m->ibo);
    if(m->raw_dirty){glBufferData(GL_ARRAY_BUFFER,(GLsizeiptr)((size_t)m->count*16*sizeof(float)),m->vertices,GL_STATIC_DRAW);m->raw_dirty=0;}
    const GLint size[]={2,2,4,4,4};const size_t offset[]={0,2,4,8,12};
    for(GLuint i=0;i<5;i++){glEnableVertexAttribArray(i);glVertexAttribPointer(i,size[i],GL_FLOAT,GL_FALSE,16*sizeof(float),(void*)(offset[i]*sizeof(float)));}
    }
    glUniformMatrix3fv(d->text_transform,1,GL_FALSE,projection);
    glDrawElements(GL_TRIANGLES,(GLsizei)(count/4*6),GL_UNSIGNED_SHORT,(void*)((size_t)first/4*6*sizeof(uint16_t)));
}
// Preserve painter order and culling when only part of a large-font working set
// fits the bounded atlas. Normal all-cached runs retain one draw and bind.
static int draw_text(ot_display *d,struct text_mesh *m,const float *projection,uint32_t first,uint32_t count,int bound) {
    if(!count)return bound;
    if(!d->cache_ready || m->cache_all){
        int kind=d->cache_ready?2:1;
        draw_text_part(d,m,projection,first,count,kind==2,bound!=kind);return kind;
    }
    uint32_t end=(first+count)/4;
    for(uint32_t q=first/4;q<end;){
        int cached=m->cached_valid[q]!=0,kind=cached?2:1;uint32_t begin=q++;
        while(q<end && (m->cached_valid[q]!=0)==cached)q++;
        draw_text_part(d,m,projection,begin*4,(q-begin)*4,cached,bound!=kind);bound=kind;
    }
    return bound;
}
static void draw_slot(ot_display *d,uint32_t slot,const float *projection) {
    struct text_mesh *text=&d->text[slot];
    if(!text->draw_words){draw_geometry(d,&d->meshes[slot],projection,0,d->meshes[slot].count);return;}
    int text_bound=0;
    for(size_t i=0;i<text->draw_words;i+=3) {
        uint32_t *r=text->draws+i;
        if(r[0]==0){draw_geometry(d,&d->meshes[slot],projection,(GLint)r[1],(GLsizei)r[2]);text_bound=0;}
        else if(text->views){
            const float *v=text->views+i/3*8;if(v[1]==0 || v[6]<=v[4] || v[7]<=v[5])continue;
            float lo[2]={1e20f,1e20f},hi[2]={-1e20f,-1e20f};
            for(int corner=0;corner<4;corner++){
                float x=v[4+(corner&1)*2],y=v[5+(corner>>1)*2];
                float p[2]={(projection[0]*x+projection[3]*y+projection[6]+1.f)*(float)d->width*.5f,(projection[1]*x+projection[4]*y+projection[7]+1.f)*(float)d->height*.5f};
                for(int axis=0;axis<2;axis++){lo[axis]=fminf(lo[axis],p[axis]);hi[axis]=fmaxf(hi[axis],p[axis]);}
            }
            int x0=(int)ceilf(fmaxf(0.f,fminf((float)d->width,lo[0]))-.5f),y0=(int)ceilf(fmaxf(0.f,fminf((float)d->height,lo[1]))-.5f);
            int x1=(int)ceilf(fmaxf(0.f,fminf((float)d->width,hi[0]))-.5f),y1=(int)ceilf(fmaxf(0.f,fminf((float)d->height,hi[1]))-.5f);
            if(x1<=x0 || y1<=y0)continue;
            float moved[9];memcpy(moved,projection,sizeof(moved));
            moved[6]+=projection[0]*v[2]+projection[3]*v[3];moved[7]+=projection[1]*v[2]+projection[4]*v[3];
            glEnable(GL_SCISSOR_TEST);glScissor(x0,y0,x1-x0,y1-y0);
            text_bound=draw_text(d,text,moved,(uint32_t)v[0],(uint32_t)v[1],text_bound);glDisable(GL_SCISSOR_TEST);
        }else{text_bound=draw_text(d,text,projection,r[1],r[2],text_bound);}
    }
}
static int prepare_glyph_cache(ot_display *d,const float *projection,int animated);
int ot_draw(ot_display *d,const float *projection,int animated) {
    if(prepare_glyph_cache(d,projection,animated)<0)return -1;
    glBindFramebuffer(GL_FRAMEBUFFER,d->linear_scene?d->scene_fbo:0);glViewport(0,0,d->width,d->height);
    if(d->linear_scene)glClearColor(0.0051815167f,0.0116122452f,0.0221738848f,1);
    else glClearColor(16.f/255,28.f/255,41.f/255,1);
    glClear(GL_COLOR_BUFFER_BIT);
    draw_slot(d,0,projection);
    if(animated){draw_slot(d,1,projection);draw_slot(d,2,projection);}
    if(!d->linear_scene)return gl_ok(d,"Draw direct cached glyphs and primitives");
    glBindFramebuffer(GL_FRAMEBUFFER,0);glViewport(0,0,d->width,d->height);
    glUseProgram(d->resolve_program);glDisable(GL_BLEND);
    glActiveTexture(GL_TEXTURE0);glBindTexture(GL_TEXTURE_2D,d->scene_texture);
    glActiveTexture(GL_TEXTURE1);glBindTexture(GL_TEXTURE_2D,d->encode_texture);
    glBindBuffer(GL_ARRAY_BUFFER,d->resolve_vbo);
    glEnableVertexAttribArray(0);glEnableVertexAttribArray(1);
    for(GLuint i=2;i<5;i++)glDisableVertexAttribArray(i);
    glVertexAttribPointer(0,2,GL_FLOAT,GL_FALSE,4*sizeof(float),(void*)0);
    glVertexAttribPointer(1,2,GL_FLOAT,GL_FALSE,4*sizeof(float),(void*)(2*sizeof(float)));
    glDrawArrays(GL_TRIANGLE_STRIP,0,4);
    return gl_ok(d,"Draw linear scene and resolve sRGB");
}
static GLuint simple_program(ot_display *d,const char *vs,const char *fs,int color) {
    GLuint a=shader(d,GL_VERTEX_SHADER,vs);if(!a)return 0;
    GLuint b=shader(d,GL_FRAGMENT_SHADER,fs);if(!b){glDeleteShader(a);return 0;}
    GLuint p=glCreateProgram();glAttachShader(p,a);glAttachShader(p,b);
    glBindAttribLocation(p,0,"position");glBindAttribLocation(p,1,"texcoord");if(color)glBindAttribLocation(p,2,"color");
    glLinkProgram(p);glDeleteShader(a);glDeleteShader(b);GLint ok=0;glGetProgramiv(p,GL_LINK_STATUS,&ok);
    if(!ok){glGetProgramInfoLog(p,sizeof(d->error),NULL,d->error);glDeleteProgram(p);return 0;}return p;
}
static int rgba_target(ot_display *d,int width,int height,GLuint *texture,GLuint *fbo) {
    glGenTextures(1,texture);glActiveTexture(GL_TEXTURE0);glBindTexture(GL_TEXTURE_2D,*texture);texture_parameters();
    glTexImage2D(GL_TEXTURE_2D,0,GL_RGBA,width,height,0,GL_RGBA,GL_UNSIGNED_BYTE,NULL);
    glGenFramebuffers(1,fbo);glBindFramebuffer(GL_FRAMEBUFFER,*fbo);glFramebufferTexture2D(GL_FRAMEBUFFER,GL_COLOR_ATTACHMENT0,GL_TEXTURE_2D,*texture,0);
    if(glCheckFramebufferStatus(GL_FRAMEBUFFER)!=GL_FRAMEBUFFER_COMPLETE)return fail(d,"Linear RGBA framebuffer incomplete");
    return gl_ok(d,"Allocate bounded linear RGBA target");
}
static int init_color_target(ot_display *d) {
    const char *vs="attribute vec2 position;attribute vec2 texcoord;varying highp vec2 uv;void main(){gl_Position=vec4(position,0.0,1.0);uv=texcoord;}";
    const char *fs="precision mediump float;uniform sampler2D image;uniform sampler2D transfer;varying highp vec2 uv;void main(){vec4 c=texture2D(image,uv);vec3 t=(c.rgb*255.0+0.5)/256.0;vec3 s=vec3(texture2D(transfer,vec2(t.r,0.5)).r,texture2D(transfer,vec2(t.g,0.5)).r,texture2D(transfer,vec2(t.b,0.5)).r);gl_FragColor=vec4(s,c.a);}";
    d->resolve_program=simple_program(d,vs,fs,0);if(!d->resolve_program)return -1;
    glUseProgram(d->resolve_program);glUniform1i(glGetUniformLocation(d->resolve_program,"image"),0);
    glUniform1i(glGetUniformLocation(d->resolve_program,"transfer"),1);
    // The scene is RGBA8: all256 possible linear channel values map exactly
    // through this once-built IEC61966-2-1 table, avoiding fragment pow.
    uint8_t encode[256*4];
    for(int i=0;i<256;i++) {
        float linear=(float)i/255.f;
        float srgb=linear<=0.0031308f?linear*12.92f:1.055f*powf(linear,1.f/2.4f)-0.055f;
        uint8_t value=(uint8_t)lroundf(srgb*255.f);
        encode[i*4]=encode[i*4+1]=encode[i*4+2]=value;encode[i*4+3]=255;
    }
    glGenTextures(1,&d->encode_texture);glActiveTexture(GL_TEXTURE1);glBindTexture(GL_TEXTURE_2D,d->encode_texture);texture_parameters();
    glTexImage2D(GL_TEXTURE_2D,0,GL_RGBA,256,1,0,GL_RGBA,GL_UNSIGNED_BYTE,encode);
    const float quad[]={-1,-1,0,0,1,-1,1,0,-1,1,0,1,1,1,1,1};
    glGenBuffers(1,&d->resolve_vbo);glBindBuffer(GL_ARRAY_BUFFER,d->resolve_vbo);glBufferData(GL_ARRAY_BUFFER,sizeof(quad),quad,GL_STATIC_DRAW);
    if(rgba_target(d,(int)d->width,(int)d->height,&d->scene_texture,&d->scene_fbo)<0)return -1;
    glBindFramebuffer(GL_FRAMEBUFFER,0);return 0;
}
static void clear_glyph_cache(ot_display *d) {
    memset(d->glyph_buckets,0,sizeof(d->glyph_buckets));
    memset(d->glyph_recent,0,sizeof(d->glyph_recent));
    d->glyph_cache_count=0;d->cache_x=d->cache_y=1;d->cache_row=0;d->cache_generation++;d->cache_dirty=0;
    glBindFramebuffer(GL_FRAMEBUFFER,d->cache_fbo);glViewport(0,0,CACHE_SIZE,CACHE_SIZE);
    glClearColor(0,0,0,0);glClear(GL_COLOR_BUFFER_BIT);glBindFramebuffer(GL_FRAMEBUFFER,0);
}
static int init_glyph_cache(ot_display *d) {
    if(d->cache_texture)return 0;
    const char *vs="attribute vec2 position;attribute vec2 texcoord;attribute vec4 color;uniform mat3 transform;uniform float linearTint;varying highp vec2 uv;varying mediump vec4 rgba;void main(){gl_Position=vec4((transform*vec3(position,1.0)).xy,0.0,1.0);uv=texcoord;vec3 c=color.rgb;if(linearTint>0.5)c=mix(pow((color.rgb+0.055)/1.055,vec3(2.4)),color.rgb/12.92,step(color.rgb,vec3(0.04045)));rgba=vec4(c*color.a,color.a);}";
    const char *fs="precision mediump float;uniform sampler2D image;varying highp vec2 uv;varying mediump vec4 rgba;void main(){gl_FragColor=rgba*texture2D(image,uv);}";
    d->cache_program=simple_program(d,vs,fs,1);if(!d->cache_program)return -1;
    d->cache_transform=glGetUniformLocation(d->cache_program,"transform");
    glUseProgram(d->cache_program);glUniform1f(glGetUniformLocation(d->cache_program,"linearTint"),d->linear_scene?1.f:0.f);d->cache_sampler=glGetUniformLocation(d->cache_program,"image");glUniform1i(d->cache_sampler,0);
    if(rgba_target(d,CACHE_SIZE,CACHE_SIZE,&d->cache_texture,&d->cache_fbo)<0)return -1;
    if(d->atlas_scale){glBindTexture(GL_TEXTURE_2D,d->cache_texture);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MIN_FILTER,GL_LINEAR);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MAG_FILTER,GL_LINEAR);}
    d->glyph_cache=calloc(CACHE_ENTRIES,sizeof(*d->glyph_cache));if(!d->glyph_cache)return fail(d,"Glyph cache metadata allocation");
    glGenBuffers(1,&d->cache_upload);clear_glyph_cache(d);return 0;
}
// Internal, bounded cache indexing only; immutable resource identity remains SHA-256.
static uint32_t glyph_bucket(const float *key,int w,int h){
    uint32_t hash=2166136261u;
    for(int i=0;i<24;i++){uint32_t bits;memcpy(&bits,key+i,sizeof(bits));hash=(hash^bits)*16777619u;}
    return (hash^(uint32_t)w*31u^(uint32_t)h*131u)&(CACHE_BUCKETS-1u);
}
static struct glyph_entry *find_glyph(ot_display *d,const float *key,int w,int h){
    for(int i=d->glyph_buckets[glyph_bucket(key,w,h)];i;i=d->glyph_cache[i-1].next){
        struct glyph_entry *e=&d->glyph_cache[i-1];
        if(e->w==w && e->h==h && !memcmp(e->key,key,sizeof(e->key)))return e;
    }
    return NULL;
}
static void index_glyph(ot_display *d,struct glyph_entry *e){
    uint32_t bucket=glyph_bucket(e->key,e->w,e->h);
    e->next=d->glyph_buckets[bucket];d->glyph_buckets[bucket]=(int)(e-d->glyph_cache)+1;
}
static int cache_quad_exact(ot_display *d,const float *q,const float *projection,struct cached_vertex *out) {
    float physical[8],minx=1e20f,miny=1e20f,maxx=-1e20f,maxy=-1e20f;
    for(int i=0;i<4;i++) {
        float x=q[i*16],y=q[i*16+1];
        physical[i*2]=(projection[0]*x+projection[3]*y+projection[6]+1.f)*(float)d->width*.5f;
        physical[i*2+1]=(projection[1]*x+projection[4]*y+projection[7]+1.f)*(float)d->height*.5f;
        minx=fminf(minx,physical[i*2]);maxx=fmaxf(maxx,physical[i*2]);miny=fminf(miny,physical[i*2+1]);maxy=fmaxf(maxy,physical[i*2+1]);
        if(i && memcmp(q+4,q+i*16+4,12*sizeof(float)))return 1;
    }
    float ox=floorf(minx)-1,oy=floorf(miny)-1,fw=ceilf(maxx)-ox+1,fh=ceilf(maxy)-oy+1;
    if(!isfinite(fw) || !isfinite(fh) || fw<1 || fh<1 || fw>CACHE_SIZE-2 || fh>CACHE_SIZE-2 || fabsf(ox)>1e8f || fabsf(oy)>1e8f)return 1;
    int w=(int)fw,h=(int)fh;float key[24];
    for(int i=0;i<4;i++) {
        // Preserve exact physical phase; the atlas remains bounded by eviction.
        key[i*2]=physical[i*2]-ox;key[i*2+1]=physical[i*2+1]-oy;
        key[8+i*2]=q[i*16+2];key[8+i*2+1]=q[i*16+3];
    }
    memcpy(key+16,q+8,8*sizeof(float));
    struct glyph_entry *entry=find_glyph(d,key,w,h);
    if(entry)d->text_hits++;
    else {
        if(d->cache_x+w+1>CACHE_SIZE){d->cache_x=1;d->cache_y+=d->cache_row+1;d->cache_row=0;}
        if(d->cache_y+h+1>CACHE_SIZE || d->glyph_cache_count>=CACHE_ENTRIES)return 2;
        entry=&d->glyph_cache[d->glyph_cache_count++];memcpy(entry->key,key,sizeof(key));
        entry->x=d->cache_x;entry->y=d->cache_y;entry->w=w;entry->h=h;index_glyph(d,entry);d->cache_x+=w+1;if(h>d->cache_row)d->cache_row=h;
        float quad[64];memcpy(quad,q,sizeof(quad));
        for(int i=0;i<4;i++) {
            quad[i*16]=key[i*2]+(float)entry->x;quad[i*16+1]=key[i*2+1]+(float)entry->y;
            for(int j=4;j<8;j++)quad[i*16+j]=1;
        }
        const float matrix[]={2.f/CACHE_SIZE,0,0,0,2.f/CACHE_SIZE,0,-1,-1,1};
        glBindFramebuffer(GL_FRAMEBUFFER,d->cache_fbo);glViewport(0,0,CACHE_SIZE,CACHE_SIZE);glDisable(GL_BLEND);
        glUseProgram(d->text_program);glUniformMatrix3fv(d->text_transform,1,GL_FALSE,matrix);
        glUniform1f(glGetUniformLocation(d->text_program,"coverageScale"),1.f);
    glUniform2f(d->text_curve_size,(float)d->curve_width,(float)d->curve_height);glUniform2f(d->text_band_size,(float)d->band_width,(float)d->band_height);
        glActiveTexture(GL_TEXTURE0);glBindTexture(GL_TEXTURE_2D,d->curve_texture);glActiveTexture(GL_TEXTURE1);glBindTexture(GL_TEXTURE_2D,d->band_texture);
        glBindBuffer(GL_ARRAY_BUFFER,d->cache_upload);glBufferData(GL_ARRAY_BUFFER,sizeof(quad),quad,GL_STREAM_DRAW);
        const GLint size[]={2,2,4,4,4};const size_t offset[]={0,2,4,8,12};
        for(GLuint i=0;i<5;i++){glEnableVertexAttribArray(i);glVertexAttribPointer(i,size[i],GL_FLOAT,GL_FALSE,16*sizeof(float),(void*)(offset[i]*sizeof(float)));}
        const uint16_t ix[]={0,1,2,2,1,3};glBindBuffer(GL_ELEMENT_ARRAY_BUFFER,0);glDrawElements(GL_TRIANGLES,6,GL_UNSIGNED_SHORT,ix);d->text_misses++;
    }
    const float corner[]={0,0,1,0,0,1,1,1};
    uint8_t tint[4];for(int j=0;j<4;j++)tint[j]=(uint8_t)(q[j+4]*255.f+0.5f);
    for(int i=0;i<4;i++) {
        struct cached_vertex *v=out+i;float cx=corner[i*2],cy=corner[i*2+1];
        v->x=(ox+cx*fw)*2.f/(float)d->width-1;v->y=(oy+cy*fh)*2.f/(float)d->height-1;
        v->u=((float)entry->x+cx*fw)/CACHE_SIZE;v->v=((float)entry->y+cy*fh)/CACHE_SIZE;
        memcpy(v->rgba,tint,sizeof(tint));
    }
    return 0;
}
// Phase-independent, prefiltered coverage: raster once at 2x/3x, with
// analytic coverage still integrated over one destination pixel. Linear sampling
// then reconstructs fractional placement without changing the glyph cache key.
static int cache_quad_sampled(ot_display *d,const float *q,struct cached_vertex *out,int retained) {
    uint8_t tint[4];
    for(int i=1;i<4;i++)if(memcmp(q+4,q+i*16+4,12*sizeof(float)))return 1;
    float du=q[18]-q[2],dv=q[35]-q[3];
    if(fabsf(du)<1e-8f || fabsf(dv)<1e-8f || q[8]<=0 || q[9]<=0)return 1;
    if(retained && d->stable_text && (q[16]<=q[0] || q[33]<=q[1]))return 1;
    // Band addresses are already bounded by the safe Rust admission layer.
    // This is only a hint: compare the complete metadata and physical size
    // before reusing the entry, including when the visible quad is clipped.
    unsigned recent=(((unsigned)q[12]+(unsigned)q[13]*256u)/16u)&(CACHE_ENTRIES-1u);
    struct glyph_entry *entry=d->glyph_recent[recent]?&d->glyph_cache[d->glyph_recent[recent]-1]:NULL;
    const float *p=d->sampled_pixel_transform;
    float dx=q[16]-q[0],dy=q[17]-q[1];
    float ux_span=p[0]*dx+p[2]*dy,uy_span=p[1]*dx+p[3]*dy;
    dx=q[32]-q[0];dy=q[33]-q[1];
    float vx_span=p[0]*dx+p[2]*dy,vy_span=p[1]*dx+p[3]*dy;
    if(entry && !memcmp(entry->key+2,q+8,8*sizeof(float)) &&
       (ux_span==0.f || uy_span==0.f) && (vx_span==0.f || vy_span==0.f) &&
       ((ux_span==0.f && vy_span==0.f) || (uy_span==0.f && vx_span==0.f)) &&
       fabsf(fabsf(ux_span)+fabsf(uy_span)-fabsf(du)*entry->key[0])<=0.001f+fabsf(du)/512.f &&
       fabsf(fabsf(vx_span)+fabsf(vy_span)-fabsf(dv)*entry->key[1])<=0.001f+fabsf(dv)/512.f){
        d->text_hits++;
        if(d->profile_text)d->profile_recent++;
        goto emit_sampled;
    }
    float ux=ux_span/du,uy=uy_span/du;
    float vx=vx_span/dv,vy=vy_span/dv;
    if(fabsf(ux*vx+uy*vy)>0.01f)return 1;
    // The shared hosts use axis-aligned quads (including 90-degree scanout).
    // Avoid two general libm hypot calls per glyph on the ARMv7 fast path.
    float su=(ux==0.f || uy==0.f)?fabsf(ux)+fabsf(uy):hypotf(ux,uy);
    float sv=(vx==0.f || vy==0.f)?fabsf(vx)+fabsf(vy):hypotf(vx,vy);
    if(retained && d->stable_text){
        // Retained font strips scale uniformly in logical space. Y stays local,
        // unlike a long X pen whose f32 subtraction can invent cache sizes.
        su=fabsf((q[33]-q[1])/dv)*(fabsf(p[0])+fabsf(p[1]));
    }
    if(!isfinite(su)||!isfinite(sv)||su<=0||sv<=0||su>65536.f||sv>65536.f)return 1;
    float sx=(float)(uint32_t)(su*256.f+0.5f)/256.f,sy=(float)(uint32_t)(sv*256.f+0.5f)/256.f;
    if(sx<=0 || sy<=0)return 1;
    float x0=-q[10]/q[8],y0=-q[11]/q[9],x1=(q[14]+1.f-q[10])/q[8],y1=(q[15]+1.f-q[11])/q[9];
    float ss=(float)d->atlas_scale,pad=2.f*ss;
    float fw=(x1-x0)*sx*ss+2.f*pad,fh=(y1-y0)*sy*ss+2.f*pad;
    if(!isfinite(fw)||!isfinite(fh)||fw<1||fh<1||fw>CACHE_SIZE-2||fh>CACHE_SIZE-2)return 1;
    int w=(int)fw,h=(int)fh;w+=((float)w<fw);h+=((float)h<fh);
    float key[24]={sx,sy};memcpy(key+2,q+8,8*sizeof(float));
    entry=find_glyph(d,key,w,h);
    if(entry)d->text_hits++;
    else {
        if(d->cache_x+w+1>CACHE_SIZE){d->cache_x=1;d->cache_y+=d->cache_row+1;d->cache_row=0;}
        if(d->cache_y+h+1>CACHE_SIZE || d->glyph_cache_count>=CACHE_ENTRIES)return 2;
        entry=&d->glyph_cache[d->glyph_cache_count++];memcpy(entry->key,key,sizeof(key));entry->x=d->cache_x;entry->y=d->cache_y;entry->w=w;entry->h=h;index_glyph(d,entry);
        entry->uv_scale[0]=sx*ss/CACHE_SIZE;entry->uv_scale[1]=sy*ss/CACHE_SIZE;
        entry->uv_bias[0]=((float)entry->x+pad)/CACHE_SIZE-x0*entry->uv_scale[0];
        entry->uv_bias[1]=((float)entry->y+pad)/CACHE_SIZE-y0*entry->uv_scale[1];
        d->cache_x+=w+1;if(h>d->cache_row)d->cache_row=h;
        float quad[64];memcpy(quad,q,sizeof(quad));
        const float corners[]={0,1,1,1,0,0,1,0};
        for(int i=0;i<4;i++){
            float x=corners[i*2]*(float)w,y=corners[i*2+1]*(float)h;
            quad[i*16]=x+(float)entry->x;quad[i*16+1]=y+(float)entry->y;
            quad[i*16+2]=x0+(x-pad)/(sx*ss);quad[i*16+3]=y0+(y-pad)/(sy*ss);
            for(int j=4;j<8;j++)quad[i*16+j]=1;
        }
        const float matrix[]={2.f/CACHE_SIZE,0,0,0,2.f/CACHE_SIZE,0,-1,-1,1};
        glBindFramebuffer(GL_FRAMEBUFFER,d->cache_fbo);glViewport(0,0,CACHE_SIZE,CACHE_SIZE);glDisable(GL_BLEND);
        glUseProgram(d->text_program);glUniformMatrix3fv(d->text_transform,1,GL_FALSE,matrix);
        glUniform1f(glGetUniformLocation(d->text_program,"coverageScale"),ss);
        glUniform2f(d->text_curve_size,(float)d->curve_width,(float)d->curve_height);glUniform2f(d->text_band_size,(float)d->band_width,(float)d->band_height);
        glActiveTexture(GL_TEXTURE0);glBindTexture(GL_TEXTURE_2D,d->curve_texture);glActiveTexture(GL_TEXTURE1);glBindTexture(GL_TEXTURE_2D,d->band_texture);
        glBindBuffer(GL_ARRAY_BUFFER,d->cache_upload);glBufferData(GL_ARRAY_BUFFER,sizeof(quad),quad,GL_STREAM_DRAW);
        const GLint size[]={2,2,4,4,4};const size_t offset[]={0,2,4,8,12};
        for(GLuint i=0;i<5;i++){glEnableVertexAttribArray(i);glVertexAttribPointer(i,size[i],GL_FLOAT,GL_FALSE,16*sizeof(float),(void*)(offset[i]*sizeof(float)));}
        const uint16_t ix[]={0,1,2,2,1,3};glBindBuffer(GL_ELEMENT_ARRAY_BUFFER,0);glDrawElements(GL_TRIANGLES,6,GL_UNSIGNED_SHORT,ix);d->text_misses++;
    }
    d->glyph_recent[recent]=(int)(entry-d->glyph_cache)+1;
emit_sampled:
    for(int j=0;j<4;j++)tint[j]=(uint8_t)(q[j+4]*255.f+0.5f);
    float scale_x=fabsf(p[0])+fabsf(p[1]),scale_y=fabsf(p[2])+fabsf(p[3]);
    float left=q[0]*scale_x,right=q[16]*scale_x,top=q[1]*scale_y,bottom=q[33]*scale_y;
    for(int i=0;i<4;i++){
        struct cached_vertex *v=out+i;float u=q[i*16+2],w=q[i*16+3];v->x=q[i*16];v->y=q[i*16+1];
        if(retained && d->stable_text){
            // The viewport scissor preserves the clip while transparent quad
            // bounds expand to pixel edges. Correct UVs rather than moving ink.
            float x=(i&1)?ceilf(right):floorf(left),y=(i<2)?floorf(top):ceilf(bottom);
            u=q[2]+(x-left)*du/(right-left);w=q[3]+(y-top)*dv/(bottom-top);
            v->x=x/scale_x;v->y=y/scale_y;
        }
        v->u=entry->uv_bias[0]+u*entry->uv_scale[0];
        v->v=entry->uv_bias[1]+w*entry->uv_scale[1];
        memcpy(v->rgba,tint,sizeof(tint));
    }
    return 0;
}
static int cache_quad(ot_display *d,const float *q,const float *projection,struct cached_vertex *out,int retained) {
    return d->atlas_scale?cache_quad_sampled(d,q,out,retained):cache_quad_exact(d,q,projection,out);
}
static uint64_t monotonic_ns(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return (uint64_t)t.tv_sec*1000000000ull+(uint64_t)t.tv_nsec;}
static int prepare_glyph_cache(ot_display *d,const float *projection,int animated) {
    d->sampled_pixel_transform[0]=projection[0]*(float)d->width*.5f;
    d->sampled_pixel_transform[1]=projection[1]*(float)d->height*.5f;
    d->sampled_pixel_transform[2]=projection[3]*(float)d->width*.5f;
    d->sampled_pixel_transform[3]=projection[4]*(float)d->height*.5f;
    d->cache_ready=0;int slots=animated?3:1,has_text=0;
    for(int i=0;i<slots;i++)if(d->text[i].count)has_text=1;
    if(!has_text)return 0;
    if(!d->atlas_scale)for(int i=0;i<slots;i++)if(d->text[i].views)return 0;
    if(!d->curve_texture || !d->text_program)return fail(d,"Glyph scene has no admitted font resource");
    if(init_glyph_cache(d)<0)return -1;
    if(d->cache_dirty)clear_glyph_cache(d);
    for(int attempt=0;attempt<2;attempt++) {
        int restart=0;
        for(int slot=0;slot<slots && !restart;slot++) {
            struct text_mesh *m=&d->text[slot];if(!m->count)continue;
            if(m->cached_generation==d->cache_generation && !memcmp(m->cached_projection,projection,9*sizeof(float))){d->text_hits+=(uint64_t)m->count/4;continue;}
            if((size_t)m->count>m->cached_capacity){
                struct cached_vertex *next=realloc(m->cached_vertices,(size_t)m->count*sizeof(*next));
                if(!next)return fail(d,"Glyph cache mesh staging allocation");
                m->cached_vertices=next;m->cached_capacity=(size_t)m->count;
            }
            struct cached_vertex *cached=m->cached_vertices;
            if((size_t)m->count/4>m->valid_capacity){
                unsigned char *next=realloc(m->cached_valid,(size_t)m->count/4);
                if(!next)return fail(d,"Glyph cache validity allocation");
                m->cached_valid=next;m->valid_capacity=(size_t)m->count/4;
            }
            uint64_t profile_start=d->profile_text?monotonic_ns():0;
            int status=0;m->cache_all=1;
            for(int q=0;q<m->count/4;q++){
                status=cache_quad(d,m->vertices+q*64,projection,cached+q*4,m->views!=NULL);
                if(status==2 && attempt==0)break;
                m->cached_valid[q]=(status==0);
                if(status){m->cache_all=0;memset(cached+q*4,0,4*sizeof(*cached));}
                status=0;
            }
            if(d->profile_text)d->profile_keys_ns+=monotonic_ns()-profile_start;
            profile_start=d->profile_text?monotonic_ns():0;
            if(status){clear_glyph_cache(d);restart=1;break;}
            glBindBuffer(GL_ARRAY_BUFFER,m->cached_vbo);glBufferData(GL_ARRAY_BUFFER,(GLsizeiptr)((size_t)m->count*sizeof(*cached)),cached,GL_STATIC_DRAW);
            if(d->profile_text)d->profile_upload_ns+=monotonic_ns()-profile_start;
            m->cached_generation=d->cache_generation;memcpy(m->cached_projection,projection,9*sizeof(float));
        }
        if(!restart){d->cache_ready=1;glBindFramebuffer(GL_FRAMEBUFFER,0);return gl_ok(d,"Prepare cached Slug coverage");}
    }
    // A working set larger than the bounded atlas falls back to direct Slug.
    glBindFramebuffer(GL_FRAMEBUFFER,0);return 0;
}
static uint32_t framebuffer(ot_display *d,struct gbm_bo *bo) {
    for(size_t i=0;i<d->fb_count;i++)if(d->fb[i].bo==bo)return d->fb[i].id;
    if(d->fb_count==MAX_BO || gbm_bo_get_format(bo)!=d->format){fail(d,"Unexpected scanout buffer format/count");return 0;}
    uint32_t id=0,stride=gbm_bo_get_stride(bo);
    if(drmModeAddFB(d->fd,d->width,d->height,d->format==GBM_FORMAT_RGB565?16:24,d->format==GBM_FORMAT_RGB565?16:32,stride,gbm_bo_get_handle(bo).u32,&id)<0){fail(d,"Register DRM framebuffer");return 0;}
    d->fb[d->fb_count++]=(struct framebuffer){bo,id};
    drmModeFB2 *actual=drmModeGetFB2(d->fd,id);
    if(!actual){fail(d,"Inspect registered DRM framebuffer");return 0;}
    int match=actual->pixel_format==d->format && actual->width==d->width && actual->height==d->height && actual->pitches[0]==stride;
    drmModeFreeFB2(actual);
    if(!match){fail(d,"Kernel framebuffer format/stride mismatch");return 0;}
    d->info.stride=stride;return id;
}
static void flipped(int fd,unsigned seq,unsigned sec,unsigned usec,void *data) {
    (void)fd;ot_display *d=data;d->flip=(ot_flip){(uint64_t)sec*1000000+usec,seq,d->crtc};d->pending=0;
}
static int wait_flip(ot_display *d) {
    struct timespec begin,now;clock_gettime(CLOCK_MONOTONIC,&begin);
    while(d->pending) {
        clock_gettime(CLOCK_MONOTONIC,&now);
        if(now.tv_sec-begin.tv_sec>=3)return fail(d,"Timed out waiting for kernel page flip");
        struct pollfd p={d->fd,POLLIN,0};int n=poll(&p,1,100);
        if(n<0 && errno==EINTR)continue;
        if(n<0 || (n>0 && (p.revents&(POLLERR|POLLHUP|POLLNVAL))))return fail(d,"DRM event poll");
        if(n>0) {drmEventContext ev={0};ev.version=2;ev.page_flip_handler=flipped;
            if(drmHandleEvent(d->fd,&ev)<0 && errno!=EINTR)return fail(d,"DRM flip events");}
    }return 0;
}
int ot_present(ot_display *d,ot_flip *flip) {
    memset(flip,0,sizeof(*flip));
    if(d->pending)return fail(d,"Previous flip is still pending");
    if(!eglSwapBuffers(d->egl,d->surface))return fail(d,"Swap EGL surface");
    if(d->headless)return 0;
    d->next=gbm_surface_lock_front_buffer(d->window);if(!d->next)return fail(d,"Lock GBM front buffer");
    uint32_t fb=framebuffer(d,d->next);if(!fb)return -1;
    if(!d->modeset) {
        if(drmModeSetCrtc(d->fd,d->crtc,fb,0,0,&d->connector,1,&d->mode)<0)return fail(d,"Set CRTC scanout");
        d->modeset=1;
        if(drmModeSetCursor(d->fd,d->crtc,0,0,0)<0)return fail(d,"Disable legacy cursor plane");
    } else {
        d->pending=1;
        if(drmModePageFlip(d->fd,d->crtc,fb,DRM_MODE_PAGE_FLIP_EVENT,d)<0){d->pending=0;return fail(d,"Submit page flip");}
        if(wait_flip(d)<0)return -1;
        *flip=d->flip;
    }
    if(d->front)gbm_surface_release_buffer(d->window,d->front);
    d->front=d->next;d->next=NULL;return 0;
}
int ot_input(ot_display *d,ot_touch *events,size_t capacity) {
    if(!capacity || d->input<0)return 0;
    size_t count=0;struct input_event ev;
    while(count<capacity) {
        ssize_t n=read(d->input,&ev,sizeof(ev));
        if(n<0 && errno==EINTR)continue;
        if(n<0 && (errno==EAGAIN || errno==EWOULDBLOCK))break;
        if(n!=sizeof(ev))return fail(d,"Touch device disconnected/short read");
        if(ev.type==EV_SYN && ev.code==SYN_DROPPED){d->dropped=1;d->suppressed=1;d->reported=0;events[count++]=(ot_touch){3,0,0};continue;}
        if(d->dropped) {
            if(ev.type==EV_SYN && ev.code==SYN_REPORT) {
                unsigned long keys[(KEY_MAX+8*sizeof(unsigned long))/(8*sizeof(unsigned long))]={0};
                if(ioctl(d->input,EVIOCGKEY(sizeof(keys)),keys)<0)return fail(d,"Resync touch contact");
                d->down=(keys[BTN_TOUCH/(8*sizeof(unsigned long))]>>(BTN_TOUCH%(8*sizeof(unsigned long))))&1;
                d->suppressed=d->down;d->dropped=0;
            }continue;
        }
        if(ev.type==EV_ABS && ev.code==ABS_X)d->x=ev.value;
        if(ev.type==EV_ABS && ev.code==ABS_Y)d->y=ev.value;
        if(ev.type==EV_KEY && ev.code==BTN_TOUCH)d->down=ev.value!=0;
        if(ev.type==EV_SYN && ev.code==SYN_REPORT) {
            if(d->suppressed){if(!d->down)d->suppressed=0;continue;}
            if(d->down || d->reported) {
                events[count++]=(ot_touch){d->down?(d->reported?1:0):2,(float)(d->x-d->xaxis.minimum)/(d->xaxis.maximum-d->xaxis.minimum),(float)(d->y-d->yaxis.minimum)/(d->yaxis.maximum-d->yaxis.minimum)};
                d->reported=d->down;
            }
        }
    }return (int)count;
}
int ot_pixels(ot_display *d,uint8_t *rgba,size_t length) {
    if(length!=(size_t)d->width*d->height*4)return fail(d,"Readback size mismatch");
    glPixelStorei(GL_PACK_ALIGNMENT,1);glReadPixels(0,0,d->width,d->height,GL_RGBA,GL_UNSIGNED_BYTE,rgba);
    return gl_ok(d,"Read GLES pixels");
}
void ot_close(ot_display *d) {
    if(!d)return;
    if(d->profile_text)fprintf(stderr,"Text cache key preparation %.3f ms; GPU buffer upload %.3f ms total; recent hits %llu\n",d->profile_keys_ns/1e6,d->profile_upload_ns/1e6,(unsigned long long)d->profile_recent);
    if(d->fd>=0 && d->modeset) {
        if(d->pending)(void)wait_flip(d);
        int ok=-1;
        if(d->saved && d->saved->mode_valid)ok=drmModeSetCrtc(d->fd,d->saved->crtc_id,d->saved->buffer_id,d->saved->x,d->saved->y,d->saved_connectors,d->saved_count,&d->saved->mode);
        if(ok<0 && drmModeSetCrtc(d->fd,d->crtc,0,0,0,NULL,0,NULL)<0)fprintf(stderr,"OTriOS: CRTC cleanup failed; kernel retains scanout references\n");
    }
    if(d->egl!=EGL_NO_DISPLAY && d->context!=EGL_NO_CONTEXT) {
        if(eglMakeCurrent(d->egl,d->surface,d->surface,d->context)) {
            for(int i=0;i<3;i++) {
                if(d->meshes[i].vbo)glDeleteBuffers(1,&d->meshes[i].vbo);
                glDeleteBuffers(1,&d->text[i].vbo);glDeleteBuffers(1,&d->text[i].ibo);glDeleteBuffers(1,&d->text[i].cached_vbo);
            }
            if(d->text_program)glDeleteProgram(d->text_program);
            if(d->curve_texture)glDeleteTextures(1,&d->curve_texture);
            if(d->band_texture)glDeleteTextures(1,&d->band_texture);
            if(d->program)glDeleteProgram(d->program);
            if(d->resolve_program)glDeleteProgram(d->resolve_program);
            if(d->cache_program)glDeleteProgram(d->cache_program);
            glDeleteBuffers(1,&d->resolve_vbo);glDeleteBuffers(1,&d->cache_upload);
            glDeleteTextures(1,&d->scene_texture);glDeleteTextures(1,&d->cache_texture);glDeleteTextures(1,&d->encode_texture);
            glDeleteFramebuffers(1,&d->scene_fbo);glDeleteFramebuffers(1,&d->cache_fbo);
        }
        eglMakeCurrent(d->egl,EGL_NO_SURFACE,EGL_NO_SURFACE,EGL_NO_CONTEXT);
    }
    if(d->fd>=0)for(size_t i=0;i<d->fb_count;i++)drmModeRmFB(d->fd,d->fb[i].id);
    if(d->window && d->front)gbm_surface_release_buffer(d->window,d->front);
    if(d->window && d->next && d->next!=d->front)gbm_surface_release_buffer(d->window,d->next);
    if(d->egl!=EGL_NO_DISPLAY) {
        if(d->surface!=EGL_NO_SURFACE)eglDestroySurface(d->egl,d->surface);
        if(d->context!=EGL_NO_CONTEXT)eglDestroyContext(d->egl,d->context);
        eglTerminate(d->egl);
    }
    if(d->window)gbm_surface_destroy(d->window);
    if(d->gbm)gbm_device_destroy(d->gbm);
    if(d->saved)drmModeFreeCrtc(d->saved);
    free(d->saved_connectors);
    if(d->input>=0)close(d->input);
    if(d->fd>=0){drmDropMaster(d->fd);close(d->fd);}
    for(int i=0;i<3;i++){free(d->text[i].views);free(d->text[i].draws);free(d->text[i].vertices);free(d->text[i].cached_vertices);free(d->text[i].cached_valid);}
    free(d->glyph_cache);
    free(d);
}
