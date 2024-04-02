@group(0) @binding(0) var<uniform> camera: mat4x4<f32>;
@group(0) @binding(1) var<storage, read> chunks: array<vec4<i32>>;
@group(0) @binding(2) var<storage, read> surfaces: array<vec2<u32>>;
@group(0) @binding(3) var<storage, read> colours: array<vec4<f32>>;

struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) colour_idx: u32,
    @location(1) depth: f32,
    //@location(1) pos2: vec4<f32>,
}

@vertex fn vertex(
    @builtin(vertex_index) vertex_idx: u32,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    let surface = surfaces[instance_idx];
    let voxel_pos_idx = surface.x & 0xFFFF;
    let face = (surface.x >> 16) & 0xFF;
    //let extent_y = (surface.x >> 24) & 0x0F;
    //let extent_x = (surface.x >> 28) & 0x0F;
    //let extent_y = 0u;
    //let extent_x = 0u;
    let extent_x = surface.x >> 24;
    let extent_y = 0u;

    let chunk_idx = surface.y & 0xFFFF;
    let colour_idx = surface.y >> 16;

    let chunk_pos = chunks[chunk_idx].xyz;

    let voxel_y = i32(voxel_pos_idx >> 10);
    let voxel_z = i32((voxel_pos_idx >> 5) & 0x1F);
    let voxel_x = i32(voxel_pos_idx & 0x1F);

    let voxel_pos = chunk_pos * 64 + vec3(voxel_x, voxel_y, voxel_z) * 2;

    let x = i32((vertex_idx & 1u) * 2 * (extent_x+1)) - 1;
    let y = i32((vertex_idx >> 1u) * 2 * (extent_y+1)) - 1;

    var v_offset: vec3<i32>;
    switch (face) {
        // left
        case 0u {
            v_offset = vec3(-1, -y, x);
        }
        // right
        case 1u {
            v_offset = vec3(1, y, x);
        }
        // down
        case 2u {
            v_offset = vec3(x, -1, -y);
        }
        // up
        case 3u {
            v_offset = vec3(x, 1, y);
        }
        // front
        case 4u {
            v_offset = vec3(x, y, -1);
        }
        // behind
        default {
            v_offset = vec3(-x, y, 1);
        }
    }

    let screen_pos = camera * vec4(vec3<f32>(voxel_pos + v_offset), 1.0);

    return VertexOutput(screen_pos, colour_idx, screen_pos.z / 100.0);
}

const SQRT_2_R: f32 = 0.7071067811865475;
const LIGHT_DIR: vec3<f32> = vec3(SQRT_2_R, 0.0, -SQRT_2_R);
@fragment fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    return colours[input.colour_idx] * input.depth;
    //let dx = dpdxFine(input.pos2.xyz);
    //let dy = dpdyFine(input.pos2.xyz);
    //let normal = normalize(cross(dx, dy));
    //let d = dot(LIGHT_DIR, normal);
    //return input.colour * clamp(d, 0.1, 1.0);
}
