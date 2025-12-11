// Simple RGBA texture rendering shader

@group(0) @binding(0)
var rgba_texture: texture_2d<f32>;

@group(0) @binding(1)
var rgba_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    
    // Full-screen quad (two triangles)
    let x = f32((vertex_index & 1u) << 1u) - 1.0;
    let y = 1.0 - f32((vertex_index & 2u));
    
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.tex_coord = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(rgba_texture, rgba_sampler, in.tex_coord);
}
