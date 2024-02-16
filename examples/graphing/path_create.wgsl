const LINE_SIZE: f32 = 0.01;

struct Bounds {
    pos: vec2<f32>,
    size: vec2<f32>,
}

// world space
@group(0) @binding(0) var<storage, read> points: array<vec2<f32>>;
@group(0) @binding(1) var<uniform> world_bounds: Bounds;

// device coordinates (NDC)
@group(0) @binding(2) var<storage, read_write> vertices: array<vec2<f32>>;

@compute @workgroup_size(32)
fn path_create(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = arrayLength(&points);
    if id.x+1 >= size { return; }

    let point = points[id.x];
    let point_after = points[id.x+1];
    
    let diff = point_after - point;
    let normal = normalize(vec2(-diff.y, diff.x)) * LINE_SIZE;

    let point_ndc = (point - world_bounds.pos) * 2.0 / world_bounds.size;
    vertices[id.x*2] = point_ndc + normal;
    vertices[id.x*2+1] = point_ndc - normal;
}
