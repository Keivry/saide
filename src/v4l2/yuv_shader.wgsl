// YU12 (Planar YUV 4:2:0) to RGB conversion shader
// ITU-R BT.601 limited range (16-235 for Y, 16-240 for UV)

struct Uniforms {
    // 0=0°, 1=90°, 2=180°, 3=270° (clockwise)
    rotation: u32,
};

@group(0) @binding(2) var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Full-screen quad (2 triangles)
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );

    // Base texture coordinates (0° rotation)
    var tex_coords = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );

    var uv = tex_coords[vertex_index];

    // Apply rotation to texture coordinates (clockwise)
    // Center at (0.5, 0.5), rotate, then shift back
    let centered = uv - vec2<f32>(0.5, 0.5);

    var rotated: vec2<f32>;
    switch uniforms.rotation {
        case 1u: { // 90° clockwise: (x,y) -> (y, -x)
            rotated = vec2<f32>(centered.y, -centered.x);
        }
        case 2u: { // 180°: (x,y) -> (-x, -y)
            rotated = vec2<f32>(-centered.x, -centered.y);
        }
        case 3u: { // 270° clockwise: (x,y) -> (-y, x)
            rotated = vec2<f32>(-centered.y, centered.x);
        }
        default: { // 0°
            rotated = centered;
        }
    }

    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.tex_coord = rotated + vec2<f32>(0.5, 0.5);
    return output;
}

// Single combined YUV texture (Y + V + U packed vertically)
// Texture layout (width × height*2):
//   [0, 0.5)      : Y plane
//   [0.5, 0.75)   : V plane (half width, left-aligned)
//   [0.75, 1.0)   : U plane (half width, left-aligned)
@group(0) @binding(0) var tex_yuv: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample Y from top half [0, 0.5)
    let y_coord = vec2<f32>(in.tex_coord.x, in.tex_coord.y * 0.5);
    let y = textureSample(tex_yuv, tex_sampler, y_coord).r;
    
    // Sample V from second quarter [0.5, 0.75)
    let v_coord = vec2<f32>(in.tex_coord.x * 0.5, 0.5 + in.tex_coord.y * 0.25);
    let v = textureSample(tex_yuv, tex_sampler, v_coord).r;
    
    // Sample U from last quarter [0.75, 1.0)
    let u_coord = vec2<f32>(in.tex_coord.x * 0.5, 0.75 + in.tex_coord.y * 0.25);
    let u = textureSample(tex_yuv, tex_sampler, u_coord).r;

    // BT.601 limited range to full range conversion
    let y_norm = (y - 16.0 / 255.0) * (255.0 / 219.0);
    let u_norm = (u - 128.0 / 255.0) * (255.0 / 224.0);
    let v_norm = (v - 128.0 / 255.0) * (255.0 / 224.0);

    // BT.601 YUV to RGB matrix
    let r = y_norm + 1.402 * v_norm;
    let g = y_norm - 0.344136 * u_norm - 0.714136 * v_norm;
    let b = y_norm + 1.772 * u_norm;

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}
