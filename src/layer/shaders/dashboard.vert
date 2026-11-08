#version 450

layout(push_constant) uniform DashboardPush {
    vec4 rects[6];
    vec4 color;
    vec4 params;
} push_data;

layout(location = 0) out vec2 local_position;
layout(location = 1) flat out vec2 rect_size;

void main() {
    const vec2 positions[6] = vec2[](
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0)
    );
    vec2 uv = positions[gl_VertexIndex];
    vec4 rect;
    // Constant indices avoid missing instances on drivers that mishandle push-constant array indexing.
    switch (gl_InstanceIndex) {
        case 0: rect = push_data.rects[0]; break;
        case 1: rect = push_data.rects[1]; break;
        case 2: rect = push_data.rects[2]; break;
        case 3: rect = push_data.rects[3]; break;
        case 4: rect = push_data.rects[4]; break;
        default: rect = push_data.rects[5]; break;
    }
    rect_size = (rect.zw - rect.xy) * push_data.params.xy * 0.5;
    local_position = uv * rect_size;
    gl_Position = vec4(mix(rect.xy, rect.zw, uv), 0.0, 1.0);
}
