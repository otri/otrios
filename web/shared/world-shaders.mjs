// SPDX-License-Identifier: Apache-2.0
// Endpoint pairs remain in immutable chunk-local VBOs; the camera is uniforms.
export const worldVertex=`
attribute vec4 segment;
attribute vec2 corner;
uniform mat3 projection;
uniform vec2 origin;
uniform float scale;
varying mediump float edge;
varying mediump float halfWidth;
void main(){
 vec2 delta=segment.zw-segment.xy;
 vec2 normal=vec2(-delta.y,delta.x)/max(length(delta),0.0001);
 halfWidth=clamp(3.0*scale,1.0,16.0)*0.5;
 edge=corner.y*(halfWidth+1.0);
 vec2 p=origin+mix(segment.xy,segment.zw,corner.x)*scale+normal*edge;
 vec3 q=projection*vec3(p,1.0);gl_Position=vec4(q.xy,0.0,q.z);
}`;
export const worldFragment=`
precision mediump float;
uniform vec4 tint;
varying mediump float edge;
varying mediump float halfWidth;
void main(){float alpha=tint.a*clamp(halfWidth+0.5-abs(edge),0.0,1.0);gl_FragColor=vec4(tint.rgb*alpha,alpha);}
`;
export const gridVertex=`
attribute vec2 position;
uniform mat3 projection;
uniform vec3 grid;
varying mediump vec2 cell;
void main(){cell=(position-grid.yz)/grid.x;vec3 q=projection*vec3(position,1.0);gl_Position=vec4(q.xy,0.0,q.z);}
`;
export const gridFragment=`
#extension GL_OES_standard_derivatives : enable
precision mediump float;
varying mediump vec2 cell;
void main(){vec2 d=abs(fract(cell+0.5)-0.5);vec2 w=max(fwidth(cell),vec2(0.0001));float a=0.30*(1.0-min(1.0,min(d.x/w.x,d.y/w.y)));gl_FragColor=vec4(vec3(0.44,0.47,0.51)*a,a);}
`;
