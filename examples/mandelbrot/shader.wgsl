struct Bounds {
    pos: vec2<f32>,
    size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> bounds: Bounds;
@group(0) @binding(1) var output: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(16, 16)
fn render(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(output);
    let p = id.xy;
    if any(p >= dims) { return; }

    let c = vec2<f32>(p) * bounds.size / vec2<f32>(dims) + bounds.pos - bounds.size / 2.0;
    var z = c;
    
    var i: u32;
    for (i = 0u; i < 256; i++) {
        let sq = z*z;
        
        if sq.x + sq.y >= 4.0 { break; }

        let new_z = vec2(sq.x - sq.y, 2.0*z.x*z.y) + c;

        z = new_z;
    }

    let sq = z*z;
    if sq.x+sq.y <= 4.0 {
        textureStore(output, p, vec4(0.0, 0.0, 0.0, 1.0));
    } else {
        let t = f32(i) / 256.0;

        let theta = 3.0*3.141592654*t;
        let scale = sqrt(1-t*t);
        let r = abs(scale*cos(theta));
        let g = abs(scale*sin(theta));
        let colour = vec4(r, g, t, 1.0);

        textureStore(output, p, colour);
    }
}
