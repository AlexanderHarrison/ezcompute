struct Data {
    pos: vec2<f32>,
    size: vec2<f32>,
    time: f32,
}

@group(0) @binding(0) var<uniform> data: Data;
@group(0) @binding(1) var output: texture_storage_2d<rgba8unorm, write>;

const PI: f32 = 3.141592653589793;
const LINE_WIDTH: f32 = 1.0 / 1024;

// creates a sin curve. c(...-2pi/3) = 0, c(0) = 1, c(2pi/3...) = 0
fn c(x: f32) -> f32 {
    return cos(clamp(0.75*x, -PI/2.0, PI/2.0));
}

fn complex_log(z: vec2<f32>) -> vec2<f32> {
    let angle = atan2(z.y, z.x);
    let mag = length(z);
    return vec2(log(mag), angle);
}

fn complex_exp(z: vec2<f32>) -> vec2<f32> {
    // exp(z) = exp(Re(z)) * exp(Im(z));
    let a = exp(z.x);
    let b = vec2(cos(z.y), sin(z.y));
    return a * b;
}

fn complex_add(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return vec2(a.x+b.x, a.y+b.y);
}

fn complex_mul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return vec2(
        a.x*b.x - a.y*b.y,
        a.x*b.y + a.y*b.x,
    );
}

fn complex_pow(z: vec2<f32>, n: vec2<f32>) -> vec2<f32> {
    return complex_exp(complex_mul(n, complex_log(z)));
}

@compute @workgroup_size(16, 16)
fn render(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(output);
    let p = id.xy;
    if any(p >= dims) { return; }

    var c = vec2<f32>(p) * data.size / vec2<f32>(dims) + data.pos - data.size / 2.0;

    // run complex function -------------------

    //let circle = vec2(2.0*cos(data.time), sin(data.time));
    //let z = complex_mul(complex_log(c), circle);

    //let circle = vec2(cos(data.time), sin(data.time));
    //var z = vec2(0.0);
    //let n = 2.0;
    //for (var i = -n; i <= n+0.1; i += 1.0) {
    //    let a = complex_mul(circle, complex_pow(complex_mul(circle, c), vec2(i, 0.0)));
    //    z = complex_add(z, a);
    //}

    let z = complex_pow(c, vec2(data.time, 0.0));

    // draw colours ----------------------------

    let angle = atan2(z.y, z.x);
    let magnitude_sqrt = log2(length(z));
    let magnitude_mod = fract(magnitude_sqrt) * 0.8 + 0.2;

    let r = c(angle);
    let g = c(angle + 2.0*PI/3.0) + c(angle - 4.0*PI/3.0);
    let b = c(angle - 2.0*PI/3.0) + c(angle + 4.0*PI/3.0);

    var colour = vec3(r, g, b) * magnitude_mod;

    // draw lines ------------------------------

    let line_width = vec2(LINE_WIDTH) * data.size;
    if any(fract(c+line_width/2.0) < line_width) {
        colour += vec3(0.5, 0.5, 0.5);
    }

    colour = clamp(colour, vec3(0.0), vec3(1.0));
    textureStore(output, p, vec4(colour, 1.0));
}
