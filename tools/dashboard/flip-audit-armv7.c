// SPDX-License-Identifier: Apache-2.0
// Diagnostic only: count existing libdrm event reads without ptrace stops.
#if !defined(__arm__)
#error "This diagnostic audit module targets 32-bit ARM glibc"
#endif
#define _GNU_SOURCE
#include <errno.h>
#include <link.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#define LIMIT 8192
static struct { uint32_t seq, sec, usec, crtc; } samples[LIMIT];
static unsigned count, bindings, calls;
static int event_fd=-1;
static char report_path[4096];
static ssize_t (*original_read)(int, void *, size_t);
static void load_path(void) {
    const char *path=getenv("OTRIOS_FLIP_REPORT");
    if(path){snprintf(report_path,sizeof(report_path),"%s",path);return;}
    // An audit namespace can have a separate libc environ. Retain only this
    // output path from our own startup environment; never log other variables.
    FILE *in=fopen("/proc/self/environ","rb");if(!in)return;
    char buf[65536];size_t n=fread(buf,1,sizeof(buf)-1,in);fclose(in);buf[n]=0;
    for(size_t at=0;at<n;){size_t len=strnlen(buf+at,n-at);
        if(len>=19 && !strncmp(buf+at,"OTRIOS_FLIP_REPORT=",19)){snprintf(report_path,sizeof(report_path),"%s",buf+at+19);return;}
        at+=len+1;
    }
}
static ssize_t counted_read(int fd, void *buffer, size_t size) {
    ssize_t result=original_read(fd,buffer,size);int saved_errno=errno;++calls;
    if(result>=32) {
        const unsigned char *bytes=buffer;uint32_t head[2];memcpy(head,bytes,8);
        if(event_fd<0 && head[0]==2 && head[1]==32) {
            char link[64],path[128];snprintf(link,sizeof(link),"/proc/self/fd/%d",fd);
            ssize_t n=readlink(link,path,sizeof(path)-1);
            if(n>0){path[n]=0;if(!strcmp(path,"/dev/dri/card0"))event_fd=fd;}
        }
        if(fd==event_fd) for(size_t at=0;at+8<=(size_t)result;){
            uint32_t fields[8];memcpy(head,bytes+at,8);
            if(head[1]<8 || head[1]>(size_t)result-at)break;
            if(head[0]==2 && head[1]==32 && count<LIMIT) {
                memcpy(fields,bytes+at,32);
                if(fields[5]<1000000){samples[count].sec=fields[4];samples[count].usec=fields[5];samples[count].seq=fields[6];samples[count].crtc=fields[7];++count;}
            }
            at+=head[1];
        }
    }
    errno=saved_errno;return result;
}
unsigned int la_version(unsigned int version) { load_path();return version>=LAV_CURRENT ? LAV_CURRENT : version; }
unsigned int la_objopen(struct link_map *map, Lmid_t lmid, uintptr_t *cookie) {
    (void)map;(void)lmid;(void)cookie;return LA_FLG_BINDTO|LA_FLG_BINDFROM;
}
uintptr_t la_symbind32(Elf32_Sym *sym,unsigned int ndx,uintptr_t *ref,uintptr_t *def,unsigned int *flags,const char *name) {
    (void)ndx;(void)def;const struct link_map *map=(void *)*ref;
    *flags|=LA_SYMB_NOPLTENTER|LA_SYMB_NOPLTEXIT;
    if (!strcmp(name,"read") && map && strstr(map->l_name,"libdrm.so")) { ++bindings;original_read=(void *)sym->st_value; return (uintptr_t)counted_read; }
    return sym->st_value;
}
__attribute__((destructor)) static void save_samples(void) {
    if (!report_path[0])load_path();
    if (!report_path[0])return;
    FILE *out=fopen(report_path,"wx");if(!out)return;
    fprintf(out,"{\"bindings\":%u,\"calls\":%u,\"measurement\":\"kernel DRM completion records counted in existing libdrm reads via loader audit; no per-frame logging\",\"events\":[",bindings,calls);
    for(unsigned i=0;i<count;i++)fprintf(out,"%s[%u.%06u,%u,%u]",i?",":"",samples[i].sec,samples[i].usec,samples[i].seq,samples[i].crtc);
    fprintf(out,"]}\n");fclose(out);
}
