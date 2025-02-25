@group(0) @binding(0) var<uniform> camera: mat4x4<f32>;
@group(0) @binding(1) var<storage, read> chunks: array<vec4<i32>>;
@group(0) @binding(2) var<storage, read> surfaces: array<vec2<u32>>;
@group(0) @binding(3) var<storage, read> colours: array<vec4<f32>>;

struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) colour_idx: u32,
    @location(1) depth: f32,
    @location(2) @interpolate(flat) normal: vec3<f32>,
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
    var normal: vec3<f32>;
    switch (face) {
        // left
        case 0u {
            v_offset = vec3(-1, -y, x);
            normal = vec3(-1.0, 0.0, 0.0);
        }
        // right
        case 1u {
            v_offset = vec3(1, y, x);
            normal = vec3(1.0, 0.0, 0.0);
        }
        // down
        case 2u {
            v_offset = vec3(x, -1, -y);
            normal = vec3(0.0, -1.0, 0.0);
        }
        // up
        case 3u {
            v_offset = vec3(x, 1, y);
            normal = vec3(0.0, 1.0, 0.0);
        }
        // front
        case 4u {
            v_offset = vec3(x, y, -1);
            normal = vec3(0.0, 0.0, -1.0);
        }
        // behind
        default {
            v_offset = vec3(x, -y, 1);
            normal = vec3(0.0, 0.0, 1.0);
        }
    }

    let screen_pos = camera * vec4(vec3<f32>(voxel_pos + v_offset), 1.0);

    return VertexOutput(screen_pos, colour_idx, screen_pos.z / 100.0, normal);
}

fn lighting(
    colour: vec4<f32>, 
    normal: vec3<f32>,
    depth: f32,
) -> vec4<f32> {
    let R_SQRT_2: f32 = 1.0 / sqrt(2.0);
    let R_SQRT_3: f32 = 1.0 / sqrt(3.0);
    //let LIGHT_DIR: vec3<f32> = vec3(R_SQRT_3, R_SQRT_3, R_SQRT_3);
    let LIGHT_DIR: vec3<f32> = vec3(R_SQRT_2, R_SQRT_2, 0.0);

    let d = clamp(dot(LIGHT_DIR, normal) + 0.5, 0.0, 2.0) * 0.2 + 0.8;
    return vec4(clamp(colour.rgb * vec3(d), vec3(0.0), vec3(1.0)), colour.a);
}

@fragment fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    return lighting(colours[input.colour_idx], input.normal, input.depth);
}
