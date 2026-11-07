#version 450

layout(push_constant) uniform OverlayPush {
    vec4 rect;
    float opacity;
} push_data;

layout(location = 0) out vec4 out_color;

void main() {
    out_color = vec4(0.02, 0.02, 0.02, push_data.opacity);
}
