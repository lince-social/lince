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
uniform vec4 output_red;
uniform vec4 output_green;
uniform vec4 output_blue;
uniform vec4 output_curve;
varying vec2 v_coords;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif
void main() {
    vec4 sample_color = texture2D(tex, v_coords);
#if defined(NO_ALPHA)
    float opacity = 1.0;
#else
    float opacity = clamp(sample_color.a, 0.0, 1.0);
#endif
    vec3 linear = sample_color.rgb / max(opacity, 0.000001);
    vec3 converted = vec3(dot(output_red.xyz, linear), dot(output_green.xyz, linear), dot(output_blue.xyz, linear));
    vec3 encoded;
    if (output_curve.x > 0.5) {
        vec3 normalized = clamp(converted, 0.0, output_curve.y) / 125.0;
        vec3 power = pow(normalized, vec3(2610.0 / 16384.0));
        encoded = pow((vec3(3424.0 / 4096.0) + (2413.0 / 128.0) * power) / (vec3(1.0) + (2392.0 / 128.0) * power), vec3(2523.0 / 32.0));
    } else {
        vec3 normalized = clamp(converted / output_curve.z, 0.0, 1.0);
        encoded = mix(normalized * 12.92, 1.055 * pow(normalized, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), normalized));
    }
    vec4 result = vec4(encoded * opacity, opacity) * alpha;
#if defined(DEBUG_FLAGS)
    if (tint == 1.0) result = vec4(0.0, 0.2, 0.0, 0.2) + result * 0.8;
#endif
    gl_FragColor = result;
}
