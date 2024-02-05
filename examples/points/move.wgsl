struct Point {
    pos: vec2<f32>,
    vel: vec2<f32>,
    vals: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> points: array<Point>;
@group(0) @binding(1) var field: texture_storage_2d<rg32float, read>;
@group(0) @binding(2) var<storage, read_write> new_points: array<Point>;

@compute @workgroup_size(64)
fn write_points(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x < arrayLength(&points) { 
        let p = points[id.x];

        let in_bounds = all(p.pos >= vec2(0.0) && p.pos < vec2(600.0));
        
        var force = vec2(0.0);
        if in_bounds {
            let coords = vec2<u32>(p.pos);
            force = textureLoad(field, coords).rg;
        }
        let mass = p.vals.x;

        let new_p = Point(
            p.pos + p.vel,
            p.vel + force / mass,
            p.vals,
        );

        new_points[id.x] = new_p;
    }
}
