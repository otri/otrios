/* SPDX-License-Identifier: Apache-2.0
 * evdev owner. No graphics API, callbacks, or retained Rust buffers (RFC 0001).
 */
#define _GNU_SOURCE
#include "platform.h"
#include <linux/input.h>
#include <sys/ioctl.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <stdlib.h>
#include <stdio.h>
#include <time.h>
#include <string.h>
#define OT_CONTACTS 32
struct ot_contact { int x,y,down,reported,changed; };
struct ot_touch_source {
    int fd;
    clockid_t input_clock;
    struct input_absinfo xaxis,yaxis;
    int x,y,down,reported,dropped,suppressed;
    int mt,slot,slots,flush;
    uint64_t flush_age;
    struct ot_contact contacts[OT_CONTACTS];
    char error[512];
};
static int input_fail(ot_touch_source *d,const char *what) {
    snprintf(d->error,sizeof(d->error),"%s (errno=%d)",what,errno);
    return -1;
}
const char *ot_touch_error(ot_touch_source *d) {return d->error;}
void ot_touch_close(ot_touch_source *d) {
    if(!d)return;
    if(d->fd>=0)close(d->fd);
    free(d);
}
static int input_open(ot_touch_source *d,const char *path) {
    d->fd=open(path,O_RDONLY|O_NONBLOCK|O_CLOEXEC); if(d->fd<0)return input_fail(d,"Open configured touch device");
    d->input_clock=CLOCK_MONOTONIC;
    if(ioctl(d->fd,EVIOCSCLOCKID,&d->input_clock)<0)d->input_clock=CLOCK_REALTIME;
    if(ioctl(d->fd,EVIOCGABS(ABS_X),&d->xaxis)<0 || ioctl(d->fd,EVIOCGABS(ABS_Y),&d->yaxis)<0)
        return input_fail(d,"Touch requires ABS_X/ABS_Y");
    if(d->xaxis.maximum<=d->xaxis.minimum || d->yaxis.maximum<=d->yaxis.minimum)return input_fail(d,"Invalid touch axes");
    /* evdev reports only changed coordinates. A new reader must begin at the
     * device's current values, including a first tap at the previous position. */
    d->x=d->xaxis.value;d->y=d->yaxis.value;
    unsigned long keys[(KEY_MAX+8*sizeof(unsigned long))/(8*sizeof(unsigned long))]={0};
    if(ioctl(d->fd,EVIOCGKEY(sizeof(keys)),keys)<0)return input_fail(d,"Read current touch contact");
    d->suppressed=(keys[BTN_TOUCH/(8*sizeof(unsigned long))]>>(BTN_TOUCH%(8*sizeof(unsigned long))))&1;
    struct input_absinfo slots,mx,my;
    if(ioctl(d->fd,EVIOCGABS(ABS_MT_SLOT),&slots)==0 && slots.minimum==0 &&
       slots.maximum>=0 && slots.maximum<OT_CONTACTS &&
       ioctl(d->fd,EVIOCGABS(ABS_MT_POSITION_X),&mx)==0 &&
       ioctl(d->fd,EVIOCGABS(ABS_MT_POSITION_Y),&my)==0 &&
       mx.maximum>mx.minimum && my.maximum>my.minimum) {
        d->mt=1;d->slots=slots.maximum+1;d->slot=slots.value;
        d->xaxis=mx;d->yaxis=my;
    }
    return 0;
}
ot_touch_source *ot_touch_open(const char *path,char *error,size_t capacity) {
    ot_touch_source *d=calloc(1,sizeof(*d));
    if(!d){snprintf(error,capacity,"Input allocation failed");return NULL;}
    d->fd=-1;
    if(input_open(d,path)<0) {
        snprintf(error,capacity,"%s",d->error);ot_touch_close(d);return NULL;
    }
    return d;
}
static uint64_t input_age(ot_touch_source *d,const struct input_event *ev) {
    struct timespec now;clock_gettime(d->input_clock,&now);
    uint64_t current=(uint64_t)now.tv_sec*1000000ull+(uint64_t)now.tv_nsec/1000;
    uint64_t sample=(uint64_t)ev->time.tv_sec*1000000ull+(uint64_t)ev->time.tv_usec;
    return sample<=current?current-sample:0;
}
int ot_touch_read(ot_touch_source *d,ot_touch *events,size_t capacity) {
    if(!capacity || d->fd<0)return 0;
    size_t count=0;struct input_event ev;
    while(count<capacity) {
        /* Finish an entire SYN frame before reading another one. This also
         * preserves every contact edge when the caller has a one-event buffer. */
        if(d->flush) {
            int i=d->flush-1;
            if(i>=d->slots){d->flush=0;continue;}
            d->flush++;
            struct ot_contact *c=&d->contacts[i];
            if(c->down!=c->reported || (c->down && c->changed)) {
                events[count++]=(ot_touch){.phase=c->down?(c->reported?1:0):2,.id=(uint32_t)i+1,
                    .x=(float)(c->x-d->xaxis.minimum)/(d->xaxis.maximum-d->xaxis.minimum),
                    .y=(float)(c->y-d->yaxis.minimum)/(d->yaxis.maximum-d->yaxis.minimum),.age_us=d->flush_age};
                c->reported=c->down;
            }
            c->changed=0;continue;
        }
        ssize_t n=read(d->fd,&ev,sizeof(ev));
        if(n<0 && errno==EINTR)continue;
        if(n<0 && (errno==EAGAIN || errno==EWOULDBLOCK))break;
        if(n!=sizeof(ev))return input_fail(d,"Touch device disconnected/short read");
        if(ev.type==EV_SYN && ev.code==SYN_DROPPED){d->dropped=1;d->suppressed=1;d->reported=0;memset(d->contacts,0,sizeof(d->contacts));events[count++]=(ot_touch){.phase=3,.age_us=input_age(d,&ev)};continue;}
        if(d->dropped) {
            if(ev.type==EV_SYN && ev.code==SYN_REPORT) {
                unsigned long keys[(KEY_MAX+8*sizeof(unsigned long))/(8*sizeof(unsigned long))]={0};
                if(ioctl(d->fd,EVIOCGKEY(sizeof(keys)),keys)<0)return input_fail(d,"Resync touch contact");
                if(!d->mt && (ioctl(d->fd,EVIOCGABS(ABS_X),&d->xaxis)<0 || ioctl(d->fd,EVIOCGABS(ABS_Y),&d->yaxis)<0))return input_fail(d,"Resync touch coordinates");
                d->x=d->xaxis.value;d->y=d->yaxis.value;
                d->down=(keys[BTN_TOUCH/(8*sizeof(unsigned long))]>>(BTN_TOUCH%(8*sizeof(unsigned long))))&1;
                d->suppressed=d->down;d->dropped=0;
            }continue;
        }
        if(d->mt && ev.type==EV_ABS) {
            if(ev.code==ABS_MT_SLOT)d->slot=ev.value;
            else if(d->slot>=0 && d->slot<d->slots) {
                struct ot_contact *c=&d->contacts[d->slot];
                if(ev.code==ABS_MT_TRACKING_ID){c->down=ev.value>=0;c->changed=1;}
                if(ev.code==ABS_MT_POSITION_X){c->x=ev.value;c->changed=1;}
                if(ev.code==ABS_MT_POSITION_Y){c->y=ev.value;c->changed=1;}
            }
        }
        if(ev.type==EV_ABS && ev.code==ABS_X)d->x=ev.value;
        if(ev.type==EV_ABS && ev.code==ABS_Y)d->y=ev.value;
        if(ev.type==EV_KEY && ev.code==BTN_TOUCH)d->down=ev.value!=0;
        if(ev.type==EV_SYN && ev.code==SYN_REPORT) {
            if(d->suppressed){if(!d->down){d->suppressed=0;memset(d->contacts,0,sizeof(d->contacts));}continue;}
            if(d->mt){d->flush=1;d->flush_age=input_age(d,&ev);continue;}
            if(d->down || d->reported) {
                events[count++]=(ot_touch){d->down?(d->reported?1:0):2,1,(float)(d->x-d->xaxis.minimum)/(d->xaxis.maximum-d->xaxis.minimum),(float)(d->y-d->yaxis.minimum)/(d->yaxis.maximum-d->yaxis.minimum),input_age(d,&ev)};
                d->reported=d->down;
            }
        }
    }return (int)count;
}
