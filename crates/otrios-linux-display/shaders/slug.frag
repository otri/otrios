// SPDX-License-Identifier: MIT
// Slug analytic quadratic coverage, adapted to GLSL ES 1.00 from
// SlugPixelShader.hlsl. Copyright 2017 Eric Lengyel. Port: OTriOS contributors.
// https://github.com/EricLengyel/Slug
// The MIT license and attribution are reproduced in Slug-LICENSE.txt.
#extension GL_OES_standard_derivatives : require
precision highp float;
uniform sampler2D curveTexture;
uniform sampler2D bandTexture;
uniform vec2 curveSize;
uniform vec2 bandSize;
varying highp vec2 sampleCoord;
varying highp vec4 banding;
varying highp vec4 glyphData;
varying mediump vec4 rgba;
const int MAX_BAND_CURVES = 128;
vec4 curveAt(float offset) {
    vec2 pixel = vec2(mod(offset, curveSize.x), floor(offset / curveSize.x));
    return texture2D(curveTexture, (pixel + 0.5) / curveSize);
}
vec2 bandAt(float offset) {
    vec2 pixel = vec2(mod(offset, bandSize.x), floor(offset / bandSize.x));
    vec4 bytes = floor(texture2D(bandTexture, (pixel + 0.5) / bandSize) * 255.0 + 0.5);
    return vec2(bytes.x + 256.0 * bytes.y, bytes.z + 256.0 * bytes.w);
}
vec2 eligible(vec3 coordinates) {
    // Exact truth table of (0x2E74 >> signBits) & 0x0101,
    // expressed without integer bit operations unavailable in GLES2.
    float index = (coordinates.x < 0.0 ? 1.0 : 0.0)
                + (coordinates.y < 0.0 ? 2.0 : 0.0)
                + (coordinates.z < 0.0 ? 4.0 : 0.0);
    return vec2(index == 2.0 || index == 4.0 || index == 5.0 || index == 6.0 ? 1.0 : 0.0,
                index == 1.0 || index == 2.0 || index == 3.0 || index == 5.0 ? 1.0 : 0.0);
}
vec2 solveHorizontal(vec4 p12, vec2 p3) {
    vec2 a = p12.xy - 2.0 * p12.zw + p3;
    vec2 b = p12.xy - p12.zw;
    vec2 t;
    if (abs(a.y) < 1.0 / 65536.0) {
        t = vec2(p12.y / (2.0 * b.y));
    } else {
        float d = sqrt(max(b.y * b.y - a.y * p12.y, 0.0));
        t = (vec2(b.y) + vec2(-d, d)) / a.y;
    }
    return (a.x * t - 2.0 * b.x) * t + p12.x;
}
vec2 solveVertical(vec4 p12, vec2 p3) {
    vec2 a = p12.xy - 2.0 * p12.zw + p3;
    vec2 b = p12.xy - p12.zw;
    vec2 t;
    if (abs(a.x) < 1.0 / 65536.0) {
        t = vec2(p12.x / (2.0 * b.x));
    } else {
        float d = sqrt(max(b.x * b.x - a.x * p12.x, 0.0));
        t = (vec2(b.x) + vec2(-d, d)) / a.x;
    }
    return (a.y * t - 2.0 * b.y) * t + p12.y;
}
void main() {
    vec2 pixelsPerEm = 1.0 / max(fwidth(sampleCoord), vec2(1.0 / 65536.0));
    vec2 bandIndex = clamp(floor(sampleCoord * banding.xy + banding.zw), vec2(0.0), vec2(glyphData.z, glyphData.y));
    vec2 horizontal = bandAt(glyphData.x + bandIndex.y);
    float xcov = 0.0;
    float xwgt = 0.0;
    for (int i = 0; i < MAX_BAND_CURVES; ++i) {
        if (float(i) >= horizontal.x) break;
        float curve = bandAt(horizontal.y + float(i)).x;
        vec4 p12 = curveAt(curve) - sampleCoord.xyxy;
        vec2 p3 = curveAt(curve + 1.0).xy - sampleCoord;
        if (max(max(p12.x, p12.z), p3.x) * pixelsPerEm.x < -0.5) break;
        vec2 code = eligible(vec3(p12.y, p12.w, p3.y));
        if (code.x + code.y > 0.0) {
            vec2 r = solveHorizontal(p12, p3) * pixelsPerEm.x;
            if (code.x > 0.0) {
                xcov += clamp(r.x + 0.5, 0.0, 1.0);
                xwgt = max(xwgt, clamp(1.0 - abs(r.x) * 2.0, 0.0, 1.0));
            }
            if (code.y > 0.0) {
                xcov -= clamp(r.y + 0.5, 0.0, 1.0);
                xwgt = max(xwgt, clamp(1.0 - abs(r.y) * 2.0, 0.0, 1.0));
            }
        }
    }
    vec2 vertical = bandAt(glyphData.x + glyphData.y + 1.0 + bandIndex.x);
    float ycov = 0.0;
    float ywgt = 0.0;
    for (int i = 0; i < MAX_BAND_CURVES; ++i) {
        if (float(i) >= vertical.x) break;
        float curve = bandAt(vertical.y + float(i)).x;
        vec4 p12 = curveAt(curve) - sampleCoord.xyxy;
        vec2 p3 = curveAt(curve + 1.0).xy - sampleCoord;
        if (max(max(p12.y, p12.w), p3.y) * pixelsPerEm.y < -0.5) break;
        vec2 code = eligible(vec3(p12.x, p12.z, p3.x));
        if (code.x + code.y > 0.0) {
            vec2 r = solveVertical(p12, p3) * pixelsPerEm.y;
            if (code.x > 0.0) {
                ycov -= clamp(r.x + 0.5, 0.0, 1.0);
                ywgt = max(ywgt, clamp(1.0 - abs(r.x) * 2.0, 0.0, 1.0));
            }
            if (code.y > 0.0) {
                ycov += clamp(r.y + 0.5, 0.0, 1.0);
                ywgt = max(ywgt, clamp(1.0 - abs(r.y) * 2.0, 0.0, 1.0));
            }
        }
    }
    float coverage = max(abs(xcov * xwgt + ycov * ywgt) / max(xwgt + ywgt, 1.0 / 65536.0), min(abs(xcov), abs(ycov)));
    coverage = clamp(coverage, 0.0, 1.0);
    // Tint is already premultiplied; coverage scales RGB and alpha exactly once.
    gl_FragColor = rgba * coverage;
}
