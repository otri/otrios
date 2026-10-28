/* SPDX-License-Identifier: Apache-2.0
 * Deterministic evdev syscall fixture: compile/run on Linux without a display,
 * input privileges, or a synthetic device installed in the user's session.
 */
#define _GNU_SOURCE
#include <linux/input.h>
#include <sys/ioctl.h>
#include <unistd.h>
#include <fcntl.h>
#include <stdarg.h>
#include <assert.h>
#include <string.h>
#include <errno.h>
#include <stdio.h>
static int fixture_open(const char *path,int flags,...);
static int fixture_close(int fd);
static int fixture_ioctl(int fd,unsigned long request,...);
static ssize_t fixture_read(int fd,void *buf,size_t size);
#define open fixture_open
#define close fixture_close
#define ioctl fixture_ioctl
#define read fixture_read
#include "../src/touch.c"
#undef open
#undef close
#undef ioctl
#undef read
static struct input_event queue[32];
static size_t begin,end;
static int opened,closed,held,axis_x=300,axis_y=700,bad_axis,disconnected;
static int fixture_open(const char *path,int flags,...) {
    assert(flags==(O_RDONLY|O_NONBLOCK|O_CLOEXEC));
    if(!strcmp(path,"missing")){errno=ENOENT;return -1;}
    opened++;return 99;
}
static int fixture_close(int fd){assert(fd==99);closed++;return 0;}
static int fixture_ioctl(int fd,unsigned long request,...) {
    assert(fd==99);va_list args;va_start(args,request);void *arg=va_arg(args,void *);va_end(args);
    if(request==EVIOCSCLOCKID)return 0;
    if(request==EVIOCGABS(ABS_X) || request==EVIOCGABS(ABS_Y)) {
        struct input_absinfo axis={.value=request==EVIOCGABS(ABS_X)?axis_x:axis_y,.minimum=100,.maximum=bad_axis?100:1100};
        memcpy(arg,&axis,sizeof(axis));return 0;
    }
    assert(_IOC_NR(request)==_IOC_NR(EVIOCGKEY(1)));
    memset(arg,0,_IOC_SIZE(request));
    if(held)((unsigned long *)arg)[BTN_TOUCH/(8*sizeof(unsigned long))]|=1ul<<(BTN_TOUCH%(8*sizeof(unsigned long)));
    return 0;
}
static ssize_t fixture_read(int fd,void *buf,size_t size) {
    assert(fd==99 && size==sizeof(struct input_event));
    if(disconnected)return 0;
    if(begin==end){errno=EAGAIN;return -1;}
    memcpy(buf,&queue[begin++],size);return (ssize_t)size;
}
static void event(unsigned short type,unsigned short code,int value) {
    assert(end<32);queue[end++]=(struct input_event){.type=type,.code=code,.value=value};
}
static void edge(int down){event(EV_KEY,BTN_TOUCH,down);event(EV_SYN,SYN_REPORT,0);}
int main(void) {
    char error[512];ot_touch out[64];
    assert(!ot_touch_open("missing",error,sizeof(error)) && opened==closed);
    bad_axis=1;assert(!ot_touch_open("touch",error,sizeof(error)) && opened==closed);bad_axis=0;
    ot_touch_source *source=ot_touch_open("touch",error,sizeof(error));assert(source);
    /* First contact can report neither ABS axis; initialize from EVIOCGABS. */
    edge(1);edge(0);assert(ot_touch_read(source,out,64)==2);
    assert(out[0].phase==0 && out[1].phase==2 && out[0].x==0.2f && out[0].y==0.6f);
    /* Only the changed axis is emitted on the next tap; retain the other. */
    event(EV_ABS,ABS_X,600);edge(1);edge(0);assert(ot_touch_read(source,out,1)==1);
    assert(out[0].phase==0 && out[0].x==0.5f && out[0].y==0.6f);
    assert(ot_touch_read(source,out,64)==1 && out[0].phase==2);
    /* A lost stream cancels capture, resyncs axes, and suppresses the old
     * physical contact until lift. Its release must never become a new tap. */
    begin=end=0;edge(1);event(EV_SYN,SYN_DROPPED,0);event(EV_ABS,ABS_X,100);event(EV_SYN,SYN_REPORT,0);
    axis_x=900;axis_y=200;held=1;assert(ot_touch_read(source,out,64)==2 && out[1].phase==3);
    edge(0);assert(ot_touch_read(source,out,64)==0);
    edge(1);edge(0);assert(ot_touch_read(source,out,64)==2);
    assert(out[0].x==0.8f && out[0].y==0.1f && out[0].phase==0 && out[1].phase==2);
    disconnected=1;assert(ot_touch_read(source,out,64)<0);
    ot_touch_close(source);assert(opened==closed);disconnected=0;
    /* Opening while a finger is already held also waits for a clean lift. */
    begin=end=0;source=ot_touch_open("touch",error,sizeof(error));assert(source);
    event(EV_SYN,SYN_REPORT,0);edge(0);assert(ot_touch_read(source,out,64)==0);
    edge(1);edge(0);assert(ot_touch_read(source,out,64)==2);
    ot_touch_close(source);assert(opened==closed);
    puts("evdev fixtures passed: partial init, unchanged axes, bounded batches, stream loss, held contact, disconnect, cleanup");
    return 0;
}
