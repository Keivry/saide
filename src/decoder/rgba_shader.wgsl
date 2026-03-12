// SPDX-License-Identifier: MIT OR Apache-2.0

// RGBA texture rendering shader with rotation support

@group(0) @binding(0)
var rgba_texture: texture_2d<f32>;

@group(0) @binding(1)
var rgba_sampler: sampler;

// Rotation uniform (0-3, clockwise 90° increments)
struct Uniforms {
    rotation: u32,
}
@group(0) @binding(2) var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
}

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
    
    // Apply rotation to texture coordinates
    out.tex_coord = rotate_tex_coord(tex_coords[vertex_index], uniforms.rotation);
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(rgba_texture, rgba_sampler, in.tex_coord);
}
