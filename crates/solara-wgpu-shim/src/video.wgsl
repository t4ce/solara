struct ScreenUniform {
    size: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> screen: ScreenUniform;

@group(1) @binding(0)
var video_frame: texture_2d<f32>;

@group(1) @binding(1)
var video_sampler: sampler;

struct VertexInput {
    @location(0) pixel_position: vec2<f32>,
    @location(1) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let ndc = (input.pixel_position / screen.size) * 2.0 - vec2(1.0, 1.0);
    var out: VertexOutput;
    out.clip_position = vec4(ndc.x, -ndc.y, 0.0, 1.0);
    out.uv = input.uv;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(video_frame, video_sampler, input.uv);
}
