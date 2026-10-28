#version 100
PARK_SHADER_DEFINES
#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif
precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif
uniform float alpha;
uniform vec4 color_red;
uniform vec4 color_green;
uniform vec4 color_blue;
uniform vec4 color_curve;
varying vec2 v_coords;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif
void main() {
    vec4 sample_color = texture2D(tex, v_coords);
#if defined(NO_ALPHA)
    float opacity = 1.0;
#else
    float opacity = color_curve.z > 0.5 ? 1.0 : sample_color.a;
#endif
    vec3 encoded = sample_color.rgb / max(opacity, 0.000001);
    if (color_curve.x < 2.5) encoded = clamp(encoded, 0.0, 1.0);
    vec3 decoded;
    if (color_curve.x > 2.5) decoded = encoded;
    else if (color_curve.x > 1.5) {
        vec3 power = pow(encoded, vec3(32.0 / 2523.0));
        vec3 numerator = max(power - vec3(3424.0 / 4096.0), vec3(0.0));
        vec3 denominator = vec3(2413.0 / 128.0) - (2392.0 / 128.0) * power;
        decoded = 125.0 * pow(numerator / denominator, vec3(16384.0 / 2610.0));
    }
    else if (color_curve.x > 0.5) decoded = pow(encoded, vec3(2.2));
    else decoded = mix(encoded / 12.92, pow((encoded + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), encoded));
    vec3 converted = vec3(dot(color_red.xyz, decoded), dot(color_green.xyz, decoded), dot(color_blue.xyz, decoded)) * color_curve.y;
    vec3 output_color = converted;
    if (color_curve.w < 0.5) {
        converted = clamp(converted, 0.0, 1.0);
        output_color = mix(converted * 12.92, 1.055 * pow(converted, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), converted));
    }
    vec4 result = vec4(output_color * opacity, opacity) * alpha;
#if defined(DEBUG_FLAGS)
    if (tint == 1.0) result = vec4(0.0, 0.2, 0.0, 0.2) + result * 0.8;
#endif
    gl_FragColor = result;
}
