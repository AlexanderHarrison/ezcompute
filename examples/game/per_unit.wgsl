struct Info {
    mouse_pos: vec2<f32>,
    mouse_flags: u32,
}

const WIDTH: i32 = 1920 / 4;
const HEIGHT: i32 = 1080 / 4;

const WIDTH_F: f32 = 1920.0 / 4.0;
const HEIGHT_F: f32 = 1080.0 / 4.0;

const MOUSE_LEFT_DOWN: u32 = 1u << 0u;
const MOUSE_RIGHT_DOWN: u32 = 1u << 1u;

@group(0) @binding(0) var<uniform> info: Info;
@group(0) @binding(1) var prng: texture_storage_2d<rg32float, read>;
@group(0) @binding(2) var field: texture_storage_2d<rgba32float, read>;

@group(0) @binding(3) var field_out: texture_storage_2d<rgba32float, write>;
@group(0) @binding(4) var output: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(16, 16)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(field);
    let p = id.xy;
    if any(p >= dims) { return; }

    let rng = textureLoad(prng, id.xy);
    var field_val = textureLoad(field, id.xy);

    var r = field_val.r;
    r += (rng.r - 0.55) * 0.2;
    r = clamp(r, 0.0, 1.0);
    field_val.r = r;

    let out_val = select(0.0, 1.0, r > 0.95);

    textureStore(field_out, id.xy, field_val);
    textureStore(output, id.xy, vec4(vec3(out_val), 1.0));

    //if rng.r * rng.g > 0.95 {
    //    textureStore(output, id.xy, vec4(1.0));
    //} else {
    //    textureStore(output, id.xy, vec4(0.0, 0.0, 0.0, 1.0));
    //}
    //textureStore(output, id.xy, vec4(rng.r, rng.g, 0.0, 1.0));
}
