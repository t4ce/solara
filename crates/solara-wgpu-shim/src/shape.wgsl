struct ScreenUniform {
    size: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> screen: ScreenUniform;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) shape_type: u32,
}

struct Instance {
    @location(1) pos_size: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) shape_type: u32,
}

fn pixel_to_ndc(pixel: vec2<f32>) -> vec2<f32> {
    let ndc = (pixel / screen.size) * 2.0 - vec2(1.0, 1.0);
    return vec2(ndc.x, -ndc.y);
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, instance: Instance) -> VertexOutput {
    var unit = array<vec2<f32>, 6>(
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(1.0, 0.0),
        vec2(1.0, 1.0),
        vec2(0.0, 1.0),
    );

    let uv = unit[vertex_index];
    let pixel_pos = instance.pos_size.xy + uv * instance.pos_size.zw;
    let ndc = pixel_to_ndc(pixel_pos);

    var out: VertexOutput;
    out.clip_position = vec4(ndc, 0.0, 1.0);
    out.local_uv = uv * 2.0 - vec2(1.0, 1.0);
    out.color = instance.color;
    out.shape_type = instance.shape_type;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.shape_type == 1u {
        let dist = length(in.local_uv);
        if dist > 1.0 {
            discard;
        }
        let edge = 1.0 - smoothstep(0.95, 1.0, dist);
        return vec4(in.color.rgb, in.color.a * edge);
    }

    return in.color;
}
