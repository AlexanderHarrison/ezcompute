struct Point {
    pos: vec2<f32>,
    vel: vec2<f32>,
    vals: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> points: array<Point>;
@group(0) @binding(1) var field: texture_storage_2d<rg32float, write>;
@group(0) @binding(2) var screen: texture_storage_2d<rgba8unorm, write>;

@workgroup_size(16, 16)
@compute 
fn calculate_field(@builtin(global_invocation_id) id: vec3<u32>) {
    let field_size = textureDimensions(field).xy;
    if any(id.xy >= field_size) { return; }

    let pos = vec2<f32>(id.xy);
    var strength = vec2(0.0);

    let size = arrayLength(&points);
    for (var i: u32 = 0; i < size; i++) {
        let point = points[i];

        let diff = point.pos - pos;
        let dist = sqrt(diff.x*diff.x + diff.y*diff.y + 0.01);

        let mass = point.vals.x;
        
        if dist > 5.0 {
            strength += mass * diff / (dist*dist*dist);
        }
    }

    textureStore(field, id.xy, vec4(strength, 0.0, 1.0));

    let colour = sqrt(min(abs(strength), vec2(1.0)));
    textureStore(screen, id.xy, vec4(colour, 0.0, 1.0));
}
