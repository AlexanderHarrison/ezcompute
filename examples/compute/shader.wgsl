@group(0) @binding(0) var<storage, read> points_in: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> points_out: array<vec4<f32>>;

@compute @workgroup_size(16)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
    let size: u32 = arrayLength(&points_in);
    if id.x >= size { return; }

    let e = points_in[id.x].xyz;

    var f = vec3(0.0, 0.0, 0.0);
    for (var k = 0u; k < size; k = k + 1) {
        let e2 = points_in[k].xyz;
        let diff = e - e2;

        if any(diff != vec3(0.0)) {
            let sq = diff * diff;
            let len_sq = sq.x + sq.y + sq.z;
            let force = 1.0 / len_sq;
            f = f + diff / sqrt(len_sq) * force * 0.015;
        }
    }

    let new_e = normalize(f + e);
    points_out[id.x] = vec4(new_e, 0.0);
}
