// SPDX-License-Identifier: Apache-2.0
attribute vec2 position;
attribute vec2 renderCoord;
attribute vec4 color;
attribute vec4 bandTransform;
attribute vec4 glyph;
uniform mat3 transform;
// Default authored sRGB tint; exact linear composition is an opt-in host mode.
uniform float linearTint;
varying highp vec2 sampleCoord;
varying highp vec4 banding;
varying highp vec4 glyphData;
varying mediump vec4 rgba;
void main() {
    gl_Position = vec4((transform * vec3(position, 1.0)).xy, 0.0, 1.0);
    sampleCoord = renderCoord;
    banding = bandTransform;
    glyphData = glyph;
    vec3 tint = color.rgb;
    if (linearTint > 0.5) {
        tint = mix(pow((color.rgb + 0.055) / 1.055, vec3(2.4)), color.rgb / 12.92, step(color.rgb, vec3(0.04045)));
    }
    rgba = vec4(tint * color.a, color.a);
}
