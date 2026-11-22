// SPDX-License-Identifier: Apache-2.0
// One GLSL ES 1.00 source consumed directly by WebGL1 and the native build script.
export const modelVertex=`
attribute vec3 position;
attribute vec2 texcoord;
attribute vec3 normal;
attribute vec4 color;
uniform mat3 transform;
uniform vec2 depth;
varying highp vec2 uv;
varying mediump vec3 surfaceNormal;
varying mediump vec4 tint;
void main() {
    vec3 p=transform*position;
    gl_Position=vec4(p.xy,depth.x*position.z+depth.y,position.z);
    uv=texcoord;
    surfaceNormal=normal;
    tint=color;
}
`;
export const modelFragment=`
precision mediump float;
uniform sampler2D image;
uniform float linearTarget;
varying highp vec2 uv;
varying mediump vec3 surfaceNormal;
varying mediump vec4 tint;
vec3 decodeSrgb(vec3 c) {
    return mix(pow((c+0.055)/1.055,vec3(2.4)),c/12.92,step(c,vec3(0.04045)));
}
vec3 encodeSrgb(vec3 c) {
    return mix(1.055*pow(c,vec3(1.0/2.4))-0.055,c*12.92,step(c,vec3(0.0031308)));
}
void main() {
    vec4 texel=texture2D(image,uv);
    vec3 n=normalize(surfaceNormal)*(gl_FrontFacing?1.0:-1.0);
    vec3 light=normalize(vec3(-0.45,0.8,1.0));
    float diffuse=0.36+0.64*max(dot(n,light),0.0);
    float shine=0.045*pow(max(dot(n,normalize(light+vec3(0.0,0.0,1.0))),0.0),24.0);
    vec3 c=clamp(decodeSrgb(texel.rgb)*tint.rgb*diffuse+shine,0.0,1.0);
    if(linearTarget<0.5)c=encodeSrgb(c);
    float alpha=texel.a*tint.a;
    gl_FragColor=vec4(c*alpha,alpha);
}
`;
