struct Bounds {
    pos: vec2<f32>,
    size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> world_bounds: Bounds;
@group(0) @binding(1) var<storage, read_write> points: array<vec2<f32>>;

fn plotted_function(x: f32) -> f32 {
    return x*sin(x/20.0);
}

@compute @workgroup_size(32)
fn points_create(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = arrayLength(&points);
    if id.x >= size { return; }
    
    let percent = f32(id.x) / f32(size-1);
    let x = world_bounds.pos.x + world_bounds.size.x * (percent - 0.5);

    points[id.x] = vec2(x, plotted_function(x));
}
