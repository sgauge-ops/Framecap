#version 450

layout(push_constant) uniform DashboardPush {
    vec4 rects[6];
    vec4 color;
    vec4 params;
} push_data;

layout(location = 0) in vec2 local_position;
layout(location = 1) flat in vec2 rect_size;
layout(location = 0) out vec4 out_color;

void main() {
    float radius = min(push_data.params.z, min(rect_size.x, rect_size.y) * 0.5);
    float coverage = 1.0;
    if (radius > 0.0) {
        vec2 q = abs(local_position - rect_size * 0.5) - (rect_size * 0.5 - radius);
        float distance = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
        coverage = 1.0 - smoothstep(-0.5, 0.5, distance);
        if (push_data.params.w > 0.0) {
            coverage *= smoothstep(-push_data.params.w - 0.5, -push_data.params.w + 0.5, distance);
        }
    }
    float shading = push_data.params.w < 0.0
        ? mix(1.15, 0.85, clamp(local_position.y / rect_size.y, 0.0, 1.0))
        : 1.0;
    out_color = vec4(push_data.color.rgb * shading, push_data.color.a * coverage);
}
