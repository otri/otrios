/* SPDX-License-Identifier: Apache-2.0
 * Owned, single-threaded Linux display adapter. See RFC 0001. All Rust buffers
 * are borrowed only for the duration of the call. No callbacks enter Rust.
 */
#define _GNU_SOURCE
#include "platform.h"
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
struct mesh { GLuint vbo; GLsizei count; int opaque; };
struct framebuffer { struct gbm_bo *bo; uint32_t id; };
struct ot_display {
    int fd,input,headless,modeset,pending;
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
static int init_gl(ot_display *d) {
    const char *vs="attribute vec2 position;attribute vec4 color;uniform mat3 transform;varying lowp vec4 rgba;void main(){gl_Position=vec4((transform*vec3(position,1.0)).xy,0.0,1.0);rgba=color;}";
    const char *fs="precision mediump float;varying lowp vec4 rgba;void main(){gl_FragColor=rgba;}";
    GLuint a=shader(d,GL_VERTEX_SHADER,vs); if(!a)return -1;
    GLuint b=shader(d,GL_FRAGMENT_SHADER,fs); if(!b){glDeleteShader(a);return -1;}
    d->program=glCreateProgram(); glAttachShader(d->program,a); glAttachShader(d->program,b);
    glBindAttribLocation(d->program,0,"position"); glBindAttribLocation(d->program,1,"color");
    glLinkProgram(d->program); glDeleteShader(a); glDeleteShader(b);
    GLint ok=0; glGetProgramiv(d->program,GL_LINK_STATUS,&ok);
    if(!ok){glGetProgramInfoLog(d->program,sizeof(d->error),NULL,d->error);return -1;}
    glUseProgram(d->program); d->transform=glGetUniformLocation(d->program,"transform");
    glViewport(0,0,d->width,d->height); glDisable(GL_DEPTH_TEST); glDisable(GL_CULL_FACE); glDisable(GL_DITHER);
    glBlendFuncSeparate(GL_SRC_ALPHA,GL_ONE_MINUS_SRC_ALPHA,GL_ONE,GL_ONE_MINUS_SRC_ALPHA);
    for(int i=0;i<3;i++)glGenBuffers(1,&d->meshes[i].vbo);
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
    d->fd=d->input=-1;d->egl=EGL_NO_DISPLAY;d->context=EGL_NO_CONTEXT;d->surface=EGL_NO_SURFACE;
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
        for(int k=0;k<6;k++)if(!isfinite(v[k]) || (k<2?fabsf(v[k])>100000.f:(v[k]<0.f || v[k]>1.f))){free(packed);return fail(d,"Invalid mesh component");}
        packed[i].x=v[0];packed[i].y=v[1];
        for(int k=0;k<4;k++)packed[i].rgba[k]=(uint8_t)lroundf(v[k+2]*255.f);
        if(packed[i].rgba[3]!=255)opaque=0;
    }
    glBindBuffer(GL_ARRAY_BUFFER,d->meshes[slot].vbo);
    glBufferData(GL_ARRAY_BUFFER,(GLsizeiptr)(count*sizeof(*packed)),packed,slot==1?GL_STREAM_DRAW:GL_STATIC_DRAW);
    free(packed);d->meshes[slot].count=(GLsizei)count;d->meshes[slot].opaque=opaque;
    return gl_ok(d,"Upload retained vertex buffer");
}
static void draw_mesh(struct mesh *m) {
    if(!m->count)return;
    if(m->opaque)glDisable(GL_BLEND);else glEnable(GL_BLEND);
    glBindBuffer(GL_ARRAY_BUFFER,m->vbo);
    glVertexAttribPointer(0,2,GL_FLOAT,GL_FALSE,sizeof(struct vertex),(void*)offsetof(struct vertex,x));
    glVertexAttribPointer(1,4,GL_UNSIGNED_BYTE,GL_TRUE,sizeof(struct vertex),(void*)offsetof(struct vertex,rgba));
    glDrawArrays(GL_TRIANGLES,0,m->count);
}
int ot_draw(ot_display *d,const float *projection,int animated) {
    glUseProgram(d->program);glUniformMatrix3fv(d->transform,1,GL_FALSE,projection);
    glClearColor(16.f/255,28.f/255,41.f/255,1);glClear(GL_COLOR_BUFFER_BIT);
    glEnableVertexAttribArray(0);glEnableVertexAttribArray(1);
    draw_mesh(&d->meshes[0]);
    if(animated){draw_mesh(&d->meshes[1]);draw_mesh(&d->meshes[2]);}
    return gl_ok(d,"Draw retained primitives");
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
    if(d->fd>=0 && d->modeset) {
        if(d->pending)(void)wait_flip(d);
        int ok=-1;
        if(d->saved && d->saved->mode_valid)ok=drmModeSetCrtc(d->fd,d->saved->crtc_id,d->saved->buffer_id,d->saved->x,d->saved->y,d->saved_connectors,d->saved_count,&d->saved->mode);
        if(ok<0 && drmModeSetCrtc(d->fd,d->crtc,0,0,0,NULL,0,NULL)<0)fprintf(stderr,"OTriOS: CRTC cleanup failed; kernel retains scanout references\n");
    }
    if(d->egl!=EGL_NO_DISPLAY && d->context!=EGL_NO_CONTEXT) {
        if(eglMakeCurrent(d->egl,d->surface,d->surface,d->context)) {
            for(int i=0;i<3;i++)if(d->meshes[i].vbo)glDeleteBuffers(1,&d->meshes[i].vbo);
            if(d->program)glDeleteProgram(d->program);
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
    free(d);
}
