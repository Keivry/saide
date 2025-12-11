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
    
    // Full-screen quad (2 triangles, 6 vertices)
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),  // Bottom-left
        vec2<f32>( 1.0, -1.0),  // Bottom-right
        vec2<f32>(-1.0,  1.0),  // Top-left
        vec2<f32>(-1.0,  1.0),  // Top-left
        vec2<f32>( 1.0, -1.0),  // Bottom-right
        vec2<f32>( 1.0,  1.0),  // Top-right
    );
    
    var tex_coords = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),  // Bottom-left
        vec2<f32>(1.0, 1.0),  // Bottom-right
        vec2<f32>(0.0, 0.0),  // Top-left
        vec2<f32>(0.0, 0.0),  // Top-left
        vec2<f32>(1.0, 1.0),  // Bottom-right
        vec2<f32>(1.0, 0.0),  // Top-right
    );
    
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.tex_coord = tex_coords[vertex_index];
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(rgba_texture, rgba_sampler, in.tex_coord);
}
