struct ScreenUniform {
    size: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> screen: ScreenUniform;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

fn pixel_to_ndc(pixel: vec2<f32>) -> vec2<f32> {
    let ndc = (pixel / screen.size) * 2.0 - vec2(1.0, 1.0);
    return vec2(ndc.x, -ndc.y);
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var pixels = array<vec2<f32>, 3>(
        vec2(24.0, 24.0),
        vec2(124.0, 24.0),
        vec2(74.0, 110.0),
    );
    var colors = array<vec4<f32>, 3>(
        vec4(1.0, 0.2, 0.3, 1.0),
        vec4(0.2, 1.0, 0.4, 1.0),
        vec4(0.3, 0.5, 1.0, 1.0),
    );

    var out: VertexOutput;
    let ndc = pixel_to_ndc(pixels[vertex_index]);
    out.clip_position = vec4(ndc, 0.0, 1.0);
    out.color = colors[vertex_index];
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
