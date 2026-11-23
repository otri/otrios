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
attribute vec3 line;
uniform mat3 projection;
uniform vec3 grid;
uniform vec2 extent;
uniform float axis;
varying mediump float edge;
void main(){
 float along=line.x*grid.x+mix(grid.y,grid.z,axis);
 vec2 a=mix(vec2(along,0.0),vec2(0.0,along),axis);
 vec2 b=mix(vec2(along,extent.y),vec2(extent.x,along),axis);
 vec2 normal=mix(vec2(1.0,0.0),vec2(0.0,1.0),axis);
 edge=line.z;vec3 q=projection*vec3(mix(a,b,line.y)+normal*edge,1.0);
 gl_Position=vec4(q.xy,0.0,q.z);
}`;
export const gridFragment=`
precision mediump float;
varying mediump float edge;
void main(){float a=0.30*clamp(1.0-abs(edge),0.0,1.0);gl_FragColor=vec4(vec3(0.44,0.47,0.51)*a,a);}
`;
