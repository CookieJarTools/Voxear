#version 450

// Bootstrap pass: proves the device, swapchain and pipeline path end to end.
// Shaders move to Slang once the material system lands.

layout(push_constant) uniform Push {
    float time;
    float aspect;
} push;

layout(location = 0) out vec3 out_color;

const vec2 POSITIONS[3] = vec2[](vec2(0.0, -0.62), vec2(0.56, 0.42), vec2(-0.56, 0.42));
const vec3 COLORS[3] = vec3[](vec3(0.55, 0.36, 0.96), vec3(0.24, 0.62, 0.98), vec3(0.96, 0.42, 0.66));

void main() {
    float angle = push.time * 0.6;
    mat2 spin = mat2(cos(angle), sin(angle), -sin(angle), cos(angle));
    vec2 position = spin * POSITIONS[gl_VertexIndex];
    position.x /= push.aspect;
    gl_Position = vec4(position, 0.0, 1.0);
    // The swapchain is sRGB, so colours picked in sRGB are linearised first.
    out_color = pow(COLORS[gl_VertexIndex], vec3(2.2));
}
