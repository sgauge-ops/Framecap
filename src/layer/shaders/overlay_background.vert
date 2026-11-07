#version 450

layout(push_constant) uniform OverlayPush {
    vec4 rect;
    float opacity;
} push_data;

void main() {
    const vec2 positions[6] = vec2[](
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, 1.0),
        vec2(1.0, 0.0),
        vec2(1.0, 1.0)
    );
    vec2 position = mix(push_data.rect.xy, push_data.rect.zw, positions[gl_VertexIndex]);
    gl_Position = vec4(position, 0.0, 1.0);
}
