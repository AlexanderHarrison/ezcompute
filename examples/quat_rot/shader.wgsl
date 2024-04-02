@group(0) @binding(0) var<uniform> rot: mat4x4<f32>;

struct FragInput {
    @builtin(position) pos: vec4<f32>,
    @location(0) pos2: vec4<f32>,
    @location(1) colour: vec3<f32>,
}

@vertex fn vertex(
    @builtin(instance_index) instance_idx: u32,
    @location(0) pos: vec3<f32>
) -> FragInput {
    let x = f32(instance_idx & 3) - 1.5;
    let y = f32((instance_idx >> 2) & 3) - 1.5;
    let z = f32((instance_idx >> 4) & 3) - 1.5;

    let offset = 7.0 * vec3(x, y, z);

    let out = rot * vec4(pos + offset, 1.0);
    let colour = normalize(pos + offset) * 0.5 + 0.5;
    return FragInput(out, out, colour);
}

@fragment fn fragment(input: FragInput) -> @location(0) vec4<f32> {
    let dx = dpdxFine(input.pos2.xyz);
    let dy = dpdyFine(input.pos2.xyz);
    let normal = normalize(cross(dx, dy));
    let d = dot(normalize(vec3(0.0, 0.8, -1.0)), normal);

    return vec4(input.colour * clamp(d, 0.1, 1.0), 1.0);
}
