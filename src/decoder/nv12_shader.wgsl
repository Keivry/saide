// NV12 to RGB shader - Standard implementation
// Based on mpv/ffmpeg/chromium implementations
//
// NV12 format:
//   Y plane:  width × height (luminance)
//   UV plane: width × height/2 (chrominance, interleaved UVUVUV...)

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );
    
    var tex_coords = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );
    
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.tex_coord = tex_coords[vertex_index];
    
    return out;
}

// Two separate textures (standard approach)
@group(0) @binding(0) var y_texture: texture_2d<f32>;   // R8Unorm, full resolution
@group(0) @binding(1) var uv_texture: texture_2d<f32>;  // Rg8Unorm, half resolution
@group(0) @binding(2) var tex_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample Y (full resolution)
    let y = textureSample(y_texture, tex_sampler, in.tex_coord).r;
    
    // Sample UV (half resolution, hardware does bilinear interpolation)
    // UV texture is already Rg8Unorm where R=U, G=V
    let uv = textureSample(uv_texture, tex_sampler, in.tex_coord);
    let u = uv.r;
    let v = uv.g;
    
    // ITU-R BT.601 conversion (limited range [16-235] for Y, [16-240] for UV)
    // This is the standard used by most video content
    
    // Convert from limited range to full range
    let y_full = (y - 0.0625) * 1.164;           // (y - 16/255) * 255/219
    let u_full = u - 0.5;                        // (u - 128/255)
    let v_full = v - 0.5;                        // (v - 128/255)
    
    // BT.601 YUV to RGB conversion matrix
    var r = y_full + 1.596 * v_full;
    var g = y_full - 0.391 * u_full - 0.813 * v_full;
    var b = y_full + 2.018 * u_full;
    
    // Clamp to [0, 1]
    r = clamp(r, 0.0, 1.0);
    g = clamp(g, 0.0, 1.0);
    b = clamp(b, 0.0, 1.0);
    
    return vec4<f32>(r, g, b, 1.0);
}
