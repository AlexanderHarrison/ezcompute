const VIEWPORT: vec2<f32> = vec2(1024.0, 1024.0);
const POINT_BOX_SIZE: f32 = 1.0 / 40.0;

struct ViewState {
    scale: vec2<f32>,
    rotation: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) target_point: vec3<f32>,
    @location(1) target_pos: vec2<f32>,
}

@group(0) @binding(0) var<storage, read> points: array<vec4<f32>>;
@group(0) @binding(1) var<uniform> view_state: ViewState;

@vertex
fn vertex(
    @builtin(vertex_index) vertex_idx: u32,
    @builtin(instance_index) instance_idx: u32
) -> VertexOutput {
    let rot_cos = cos(view_state.rotation);
    let rot_sin = sin(view_state.rotation);

    let rotation_x = mat3x3<f32>(
        vec3(1.0, 0.0, 0.0),
        vec3(0.0, rot_cos.x, -rot_sin.x),
        vec3(0.0, rot_sin.x, rot_cos.x),
    );
    let rotation_y = mat3x3<f32>(
        vec3(rot_cos.y, 0.0, rot_sin.y),
        vec3(0.0, 1.0, 0.0),
        vec3(-rot_sin.y, 0.0, rot_cos.y)
    );

    let rot_mat = rotation_x * rotation_y;
    let target_point = rot_mat * points[instance_idx].xyz * vec3(view_state.scale, 1.0);

    let x = f32(vertex_idx & 1u) * 2.0 - 1.0;
    let y = f32(vertex_idx >> 1u) * 2.0 - 1.0;
    let offset = vec2(x, y) * POINT_BOX_SIZE;
    
    let vpos_2d = vec2(target_point.xy + offset);
    let vpos = vec4<f32>(vpos_2d, target_point.z / 2.0 + 0.5, 1.0);
    return VertexOutput(vpos, target_point, vpos_2d);
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    //let target_pos_screen = input.target_point.xy * vec2(0.5, -0.5) + vec2(0.5, 0.5);

    //let diff = input.pos.xy;
    let diff = (input.target_point.xy - input.target_pos.xy) * VIEWPORT;//; - input.pos.xy;
    let mul = diff*diff;
    let dist_sq = mul.x+mul.y;
    
    //if (dist_sq > 200.0) { discard; }

    let colour_scale = 0.5 / dist_sq;
    let colour = vec4(colour_scale);
    return clamp(colour, vec4(0.0), vec4(1.0));

    //let colour_scale = 10.0 / dist_sq;
    //let colour = vec4(colour_scale, colour_scale, colour_scale, 1.0);
    //return clamp(colour, vec4(0.0), vec4(1.0));
}
