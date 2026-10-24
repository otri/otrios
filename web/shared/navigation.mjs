// SPDX-License-Identifier: Apache-2.0
// Screen navigation is independent of executable release identities and local interaction state.
const aliases={controls:'component-playground',keyboard:'input-lab',animation:'animation-lab','3d':'three-d-lab',layout:'layout-lab'};
export function parseRoute(value='/home'){
  const legacy=value.startsWith('#'),path=value.replace(/^#/, '').replace(/^\//, '').replace(/\/+$/, '');
  if(!path)return 'home';
  const parts=path.split('/'),screen=legacy?(aliases[parts[0]]||parts[0]):parts[0];
  if(!/^[a-z-]{1,64}$/.test(screen)||(parts.length!==1&&!(legacy&&parts.length===2&&parts[1]==='input')))return null;
  return screen;
}
export const routePath=screen=>`/${screen}`;
export const readRoute=location=>parseRoute(location.hash||location.pathname)||'home';
export const navigationUrl=location=>routePath(readRoute(location))+(location.search||'');
export const moduleRelease=moduleUrl=>new URL(moduleUrl).pathname.match(/^\/releases\/([a-f0-9]{64})\/web\//)?.[1]||null;
export class Navigation {
  constructor(location,history){this.location=location;this.history=history;this.pending=readRoute(location);this.current=this.pending;}
  restore(){this.pending=readRoute(this.location);return routePath(this.pending);}
  present(screen,ready){
    if(!ready)return;
    const url=routePath(screen)+(this.location.search||'');
    if(this.pending!==null){
      if(screen!==this.pending)return;
      this.current=screen;this.pending=null;this.history.replaceState(null,'',url);
    }else if(screen!==this.current){
      this.current=screen;this.history.pushState(null,'',url);
    }
  }
}
