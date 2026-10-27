/* SPDX-License-Identifier: Apache-2.0
 * Offscreen Lima shader microbenchmark. No scanout, app state, or live rendering
 * policy is changed. Inputs are already linear for the small transfer tables;
 * obtaining an arbitrary destination colour is a separate compositor cost.
 */
#define _POSIX_C_SOURCE 200809L
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES2/gl2.h>
#include <gbm.h>
#include <fcntl.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
static float decode(float x){return x<=.04045f?x/12.92f:powf((x+.055f)/1.055f,2.4f);}
static float encode(float x){return x<=.0031308f?x*12.92f:1.055f*powf(x,1.f/2.4f)-.055f;}
static double now(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec+t.tv_nsec*1e-9;}
static GLuint shader(GLenum kind,const char *s){GLuint n=glCreateShader(kind);glShaderSource(n,1,&s,NULL);glCompileShader(n);GLint good;glGetShaderiv(n,GL_COMPILE_STATUS,&good);if(!good){char log[4096];glGetShaderInfoLog(n,sizeof(log),NULL,log);fprintf(stderr,"%s\n",log);exit(2);}return n;}
static GLuint program(const char *body){
 const char *vs="attribute vec2 position;varying highp vec2 uv;void main(){gl_Position=vec4(position,0.,1.);uv=position*.5+.5;}";
 char fs[8192];snprintf(fs,sizeof(fs),"precision mediump float;varying highp vec2 uv;uniform sampler2D table;uniform vec3 sourceLinear,destLinear,sourceSrgb,destSrgb;void main(){float a=fract(uv.x*7.+uv.y*3.);vec3 s=sourceLinear*a,d=destLinear*(1.-a),v=s+d;vec3 c;%s gl_FragColor=vec4(c,1.);}",body);
 GLuint a=shader(GL_VERTEX_SHADER,vs),b=shader(GL_FRAGMENT_SHADER,fs),p=glCreateProgram();glAttachShader(p,a);glAttachShader(p,b);glBindAttribLocation(p,0,"position");glLinkProgram(p);GLint good;glGetProgramiv(p,GL_LINK_STATUS,&good);if(!good)exit(3);glDeleteShader(a);glDeleteShader(b);return p;
}
int main(void){
 int fd=open("/dev/dri/renderD128",O_RDWR);if(fd<0){perror("render node");return 1;}
 struct gbm_device *gbm=gbm_create_device(fd);PFNEGLGETPLATFORMDISPLAYEXTPROC get=(PFNEGLGETPLATFORMDISPLAYEXTPROC)eglGetProcAddress("eglGetPlatformDisplayEXT");
 EGLDisplay display=get(EGL_PLATFORM_GBM_KHR,gbm,NULL);if(!eglInitialize(display,NULL,NULL)||!eglBindAPI(EGL_OPENGL_ES_API)){fprintf(stderr,"EGL init 0x%x\n",eglGetError());return 1;}
 EGLint attrs[]={EGL_SURFACE_TYPE,EGL_WINDOW_BIT,EGL_RENDERABLE_TYPE,EGL_OPENGL_ES2_BIT,EGL_RED_SIZE,8,EGL_GREEN_SIZE,8,EGL_BLUE_SIZE,8,EGL_NONE};EGLConfig config;EGLint n;
 if(!eglChooseConfig(display,attrs,&config,1,&n)||n!=1)return 1;
 EGLint contextAttrs[]={EGL_CONTEXT_CLIENT_VERSION,2,EGL_NONE};EGLContext ctx=eglCreateContext(display,config,EGL_NO_CONTEXT,contextAttrs);
 struct gbm_surface *window=gbm_surface_create(gbm,800,1280,GBM_FORMAT_XRGB8888,GBM_BO_USE_RENDERING);EGLSurface surface=eglCreateWindowSurface(display,config,(EGLNativeWindowType)window,NULL);
 if(!eglMakeCurrent(display,surface,surface,ctx)){fprintf(stderr,"EGL context 0x%x\n",eglGetError());return 1;}
 printf("GPU %s; %s\n",glGetString(GL_RENDERER),glGetString(GL_VERSION));fflush(stdout);
 const float vertices[]={-1,-1,1,-1,-1,1,1,1};GLuint buffer;glGenBuffers(1,&buffer);glBindBuffer(GL_ARRAY_BUFFER,buffer);glBufferData(GL_ARRAY_BUFFER,sizeof(vertices),vertices,GL_STATIC_DRAW);glEnableVertexAttribArray(0);glVertexAttribPointer(0,2,GL_FLOAT,GL_FALSE,0,0);
 glViewport(0,0,800,1280);glDisable(GL_BLEND);glDisable(GL_DITHER);glPixelStorei(GL_UNPACK_ALIGNMENT,1);
 unsigned char *reference=malloc(800*1280*4),*pixels=malloc(800*1280*4);if(!reference||!pixels)return 1;
 const char *names[]={"pow-reference","encode-1d-64","encode-1d-256","add-2d-64x64","blend-3d-64"};
 const char *bodies[]={
  "c=mix(1.055*pow(v,vec3(1./2.4))-.055,v*12.92,step(v,vec3(.0031308)));",
  "vec3 t=(v*63.+.5)/64.;c=vec3(texture2D(table,vec2(t.r,.5)).r,texture2D(table,vec2(t.g,.5)).r,texture2D(table,vec2(t.b,.5)).r);",
  "vec3 t=(v*255.+.5)/256.;c=vec3(texture2D(table,vec2(t.r,.5)).r,texture2D(table,vec2(t.g,.5)).r,texture2D(table,vec2(t.b,.5)).r);",
  "vec3 x=(s*63.+.5)/64.,y=(d*63.+.5)/64.;c=vec3(texture2D(table,vec2(x.r,y.r)).r,texture2D(table,vec2(x.g,y.g)).r,texture2D(table,vec2(x.b,y.b)).r);",
  "float z=floor(a*63.+.5),tx=mod(z,8.)*64.,ty=floor(z/8.)*64.;vec3 x=(floor(sourceSrgb*63.+.5)+tx+.5)/512.,y=(floor(destSrgb*63.+.5)+ty+.5)/512.;c=vec3(texture2D(table,vec2(x.r,y.r)).r,texture2D(table,vec2(x.g,y.g)).r,texture2D(table,vec2(x.b,y.b)).r);"};
 const float src[]={.9f,.4f,.2f},dst[]={.1f,.3f,.8f};float sl[3],dl[3];for(int i=0;i<3;i++){sl[i]=decode(src[i]);dl[i]=decode(dst[i]);}
 for(int mode=0;mode<5;mode++){
  GLuint p=program(bodies[mode]);glUseProgram(p);glUniform3fv(glGetUniformLocation(p,"sourceLinear"),1,sl);glUniform3fv(glGetUniformLocation(p,"destLinear"),1,dl);glUniform3fv(glGetUniformLocation(p,"sourceSrgb"),1,src);glUniform3fv(glGetUniformLocation(p,"destSrgb"),1,dst);
  int w=mode==2?256:mode==4?512:64,h=mode==3?64:mode==4?512:1;unsigned char *data=malloc((size_t)w*h);if(!data)return 1;
  for(int y=0;y<h;y++)for(int x=0;x<w;x++){
   float v;if(mode==4){int a=(y/64)*8+x/64;v=decode((x%64)/63.f)*(a/63.f)+decode((y%64)/63.f)*(1.f-a/63.f);}
   else v=mode==3?(x+y)/63.f:(float)x/(w-1);
   data[y*w+x]=(unsigned char)(fminf(1.f,encode(v))*255.f+.5f);
  }
  GLuint texture;glGenTextures(1,&texture);glActiveTexture(GL_TEXTURE0);glBindTexture(GL_TEXTURE_2D,texture);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MIN_FILTER,mode==4?GL_NEAREST:GL_LINEAR);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MAG_FILTER,mode==4?GL_NEAREST:GL_LINEAR);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_S,GL_CLAMP_TO_EDGE);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_T,GL_CLAMP_TO_EDGE);glTexImage2D(GL_TEXTURE_2D,0,GL_LUMINANCE,w,h,0,GL_LUMINANCE,GL_UNSIGNED_BYTE,data);free(data);glUniform1i(glGetUniformLocation(p,"table"),0);
  for(int i=0;i<5;i++){glDrawArrays(GL_TRIANGLE_STRIP,0,4);glFinish();}
  double begin=now();for(int i=0;i<60;i++){glDrawArrays(GL_TRIANGLE_STRIP,0,4);glFinish();}double elapsed=now()-begin;
  glReadPixels(0,0,800,1280,GL_RGBA,GL_UNSIGNED_BYTE,pixels);if(mode==0)memcpy(reference,pixels,800*1280*4);
  unsigned long long error=0;unsigned maximum=0;for(int i=0;i<800*1280*4;i++)if(i%4!=3){unsigned e=(unsigned)abs((int)pixels[i]-reference[i]);error+=e;if(e>maximum)maximum=e;}
  printf("{\"mode\":\"%s\",\"table_bytes\":%d,\"gpu_ms\":%.6f,\"completed_fps\":%.4f,\"max_byte_error\":%u,\"mean_byte_error\":%.6f}\n",names[mode],mode?w*h:0,elapsed*1000./60.,60./elapsed,maximum,(double)error/(800*1280*3));fflush(stdout);glDeleteTextures(1,&texture);glDeleteProgram(p);
 }
 free(reference);free(pixels);glDeleteBuffers(1,&buffer);eglMakeCurrent(display,EGL_NO_SURFACE,EGL_NO_SURFACE,EGL_NO_CONTEXT);eglDestroySurface(display,surface);eglDestroyContext(display,ctx);eglTerminate(display);gbm_surface_destroy(window);gbm_device_destroy(gbm);close(fd);return 0;
}
