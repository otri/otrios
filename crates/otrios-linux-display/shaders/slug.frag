// SPDX-License-Identifier: MIT
// Slug analytic quadratic coverage, adapted to GLSL ES 1.00 from
// SlugPixelShader.hlsl. Copyright 2017 Eric Lengyel. Port: OTriOS contributors.
// https://github.com/EricLengyel/Slug
// The MIT license and attribution are reproduced in Slug-LICENSE.txt.
#extension GL_OES_standard_derivatives : require
precision highp float;
uniform highp sampler2D curveTexture;
uniform highp sampler2D bandTexture;
uniform vec2 curveSize;
uniform vec2 bandSize;
// 1 for direct rendering; atlas oversampling factor for destination-pixel prefiltering.
uniform float coverageScale;
varying highp vec2 sampleCoord;
varying highp vec4 banding;
varying highp vec4 glyphData;
varying mediump vec4 rgba;
const int MAX_BAND_CURVES = 128;
// Power-of-two local scaling keeps quadratic products normal on FP16 ALUs.
const float CURVE_SCALE = 16.0;
// Keep texture addresses as byte-sized x/y coordinates. Lima's FP16 ALU
// cannot preserve the low bits of large linear indices, even with highp.
vec2 advanceAddress(vec2 address, float offset, float width) {
    float x = address.x + offset;
    float row = floor(x / width);
    return vec2(x - row * width, address.y + row);
}
vec4 curveAt(vec2 address) {
    return texture2D(curveTexture, (address + 0.5) / curveSize);
}
vec4 bandAt(vec2 address) {
    return floor(texture2D(bandTexture, (address + 0.5) / bandSize) * 255.0 + 0.5);
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
    if (abs(a.y) < 1.0 / 16384.0) {
        t = vec2(abs(b.y) < 1.0 / 16384.0 ? 0.0 : p12.y / (2.0 * b.y));
    } else {
        float discriminant = b.y * b.y - a.y * p12.y;
        if (discriminant <= 0.0) {
            t = vec2(b.y / a.y);
        } else {
        float d = sqrt(discriminant);
        // Stable roots avoid subtracting nearly equal b and sqrt(d) in FP16.
        // Preserve the reference's minus-root / plus-root ordering.
        float q = b.y + (b.y >= 0.0 ? d : -d);
        if (abs(q) < 1.0 / 16384.0) t = vec2(b.y / a.y);
        else if (b.y >= 0.0) t = vec2(p12.y / q, q / a.y);
        else t = vec2(q / a.y, p12.y / q);
        }
    }
    return (a.x * t - 2.0 * b.x) * t + p12.x;
}
vec2 solveVertical(vec4 p12, vec2 p3) {
    vec2 a = p12.xy - 2.0 * p12.zw + p3;
    vec2 b = p12.xy - p12.zw;
    vec2 t;
    if (abs(a.x) < 1.0 / 16384.0) {
        t = vec2(abs(b.x) < 1.0 / 16384.0 ? 0.0 : p12.x / (2.0 * b.x));
    } else {
        float discriminant = b.x * b.x - a.x * p12.x;
        if (discriminant <= 0.0) {
            t = vec2(b.x / a.x);
        } else {
        float d = sqrt(discriminant);
        float q = b.x + (b.x >= 0.0 ? d : -d);
        if (abs(q) < 1.0 / 16384.0) t = vec2(b.x / a.x);
        else if (b.x >= 0.0) t = vec2(p12.x / q, q / a.x);
        else t = vec2(q / a.x, p12.x / q);
        }
    }
    return (a.y * t - 2.0 * b.y) * t + p12.y;
}
void main() {
    vec2 pixelsPerEm = 1.0 / (max(fwidth(sampleCoord) * max(coverageScale, 1.0), vec2(1.0 / 16384.0)) * CURVE_SCALE);
    vec2 bandIndex = clamp(floor(sampleCoord * banding.xy + banding.zw), vec2(0.0), glyphData.zw);
    vec4 horizontal = bandAt(glyphData.xy + vec2(bandIndex.y, 0.0));
    float xcov = 0.0;
    float xwgt = 0.0;
    for (int i = 0; i < MAX_BAND_CURVES; ++i) {
        if (float(i) >= horizontal.x) break;
        vec2 curve = bandAt(advanceAddress(horizontal.yz, float(i), bandSize.x)).xy;
        vec4 p12 = (curveAt(curve) - sampleCoord.xyxy) * CURVE_SCALE;
        vec2 p3 = (curveAt(advanceAddress(curve, 1.0, curveSize.x)).xy - sampleCoord) * CURVE_SCALE;
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
    vec4 vertical = bandAt(glyphData.xy + vec2(glyphData.w + 1.0 + bandIndex.x, 0.0));
    float ycov = 0.0;
    float ywgt = 0.0;
    for (int i = 0; i < MAX_BAND_CURVES; ++i) {
        if (float(i) >= vertical.x) break;
        vec2 curve = bandAt(advanceAddress(vertical.yz, float(i), bandSize.x)).xy;
        vec4 p12 = (curveAt(curve) - sampleCoord.xyxy) * CURVE_SCALE;
        vec2 p3 = (curveAt(advanceAddress(curve, 1.0, curveSize.x)).xy - sampleCoord) * CURVE_SCALE;
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
    // 2^-14 is the minimum normal FP16 value; smaller guards flush to zero on Lima.
    float coverage = max(abs(xcov * xwgt + ycov * ywgt) / max(xwgt + ywgt, 1.0 / 16384.0), min(abs(xcov), abs(ycov)));
    coverage = clamp(coverage, 0.0, 1.0);
    // Tint is already premultiplied; coverage scales RGB and alpha exactly once.
    gl_FragColor = rgba * coverage;
}
