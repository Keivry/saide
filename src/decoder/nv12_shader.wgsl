// NV12 to RGB shader - Standard implementation with rotation support
// Based on mpv/ffmpeg/chromium implementations
//
// NV12 format:
//   Y plane:  width × height (luminance)
//   UV plane: width × height/2 (chrominance, interleaved UVUVUV...)

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
}

// Rotation uniform (0-3, clockwise 90° increments)
struct Uniforms {
    rotation: u32,
}
@group(0) @binding(3) var<uniform> uniforms: Uniforms;

fn rotate_tex_coord(uv: vec2<f32>, rotation: u32) -> vec2<f32> {
    // Center around 0.5
    let centered = uv - vec2<f32>(0.5, 0.5);
    var rotated: vec2<f32>;
    
    switch rotation {
        case 0u: {
            // No rotation
            rotated = centered;
        }
        case 1u: {
            // 90° clockwise: (x, y) -> (y, -x)
            rotated = vec2<f32>(centered.y, -centered.x);
        }
        case 2u: {
            // 180°: (x, y) -> (-x, -y)
            rotated = vec2<f32>(-centered.x, -centered.y);
        }
        default: {
            // 270° clockwise: (x, y) -> (-y, x)
            rotated = vec2<f32>(-centered.y, centered.x);
        }
    }
    
    // Return to [0, 1] range
    return rotated + vec2<f32>(0.5, 0.5);
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
    
    // Apply rotation to texture coordinates
    out.tex_coord = rotate_tex_coord(tex_coords[vertex_index], uniforms.rotation);
    
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
    
    // Sample UV (half resolution texture, but full resolution coordinates)
    // The UV texture is already half-sized, so we sample at the same tex_coord
    let uv = textureSample(uv_texture, tex_sampler, in.tex_coord);
    let u = uv.r - 0.5;
    let v = uv.g - 0.5;
    
    // ITU-R BT.601 conversion (limited range [16-235] for Y, [16-240] for UV)
    // This is the standard used by most video content
    
    // Convert from limited range to full range
    let y_full = (y - 0.0625) * 1.164;           // (y - 16/255) * 255/219
    
    // BT.601 YUV to RGB conversion matrix
    var r = y_full + 1.596 * v;
    var g = y_full - 0.391 * u - 0.813 * v;
    var b = y_full + 2.018 * u;
    
    // Clamp to [0, 1]
    r = clamp(r, 0.0, 1.0);
    g = clamp(g, 0.0, 1.0);
    b = clamp(b, 0.0, 1.0);
    
    return vec4<f32>(r, g, b, 1.0);
}
