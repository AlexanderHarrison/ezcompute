@group(0) @binding(0) var<uniform> time: f32;
@group(0) @binding(1) var<uniform> rotation: vec2<f32>;

fn rotation_matrix() -> mat3x3<f32> {
    let rot_cos = cos(rotation);
    let rot_sin = sin(rotation);

    let x_rot_mat = mat3x3(
        vec3(1.0, 0.0, 0.0),
        vec3(0.0, rot_cos.x, -rot_sin.x),
        vec3(0.0, rot_sin.x, rot_cos.x),
    );

    let y_rot_mat = mat3x3(
        vec3(rot_cos.y, 0.0, rot_sin.y),
        vec3(0.0, 1.0, 0.0),
        vec3(-rot_sin.y, 0.0, rot_cos.y),
    );

    // rotate along y axis, then along x axis
    return x_rot_mat * y_rot_mat;
}

struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) colour: vec3<f32>,
}

// vertices must be transformed to the xyz box [-1..1, -1..1, 0..1]
@vertex fn vertex(
    @location(0) v: vec3<f32>,
    @location(1) colour: vec3<f32>,
) -> VertexOutput {
    let rotated_pos = rotation_matrix() * v;

    let squashed_depth = (rotated_pos.z + 1.0) / 2.0;
    let squashed_pos = vec3(rotated_pos.x, rotated_pos.y, squashed_depth);
    return VertexOutput(vec4(squashed_pos, 1.0), colour);
}

@fragment fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4(in.colour, 1.0);
}
