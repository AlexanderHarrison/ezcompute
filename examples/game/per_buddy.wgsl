struct Buddy {
    pos: vec2<f32>,
    vel: vec2<f32>,
}

struct Info {
    mouse_pos: vec2<f32>,
    mouse_flags: u32,
}

const WIDTH: i32 = 1920 / 4;
const HEIGHT: i32 = 1080 / 4;

const WIDTH_F: f32 = 1920.0 / 4.0;
const HEIGHT_F: f32 = 1080.0 / 4.0;

const FRICTION: f32 = 0.6;
const MASS: f32 = 1.0;

const MOUSE_LEFT_DOWN: u32 = 1u << 0u;
const MOUSE_RIGHT_DOWN: u32 = 1u << 1u;

// read
@group(0) @binding(0) var<storage, read> prng: array<u32>;
@group(0) @binding(1) var<uniform> info: Info;
@group(0) @binding(2) var field: texture_storage_2d<r32float, read>;

// read-write
@group(0) @binding(3) var<storage, read_write> buddies: array<Buddy>;

// write
@group(0) @binding(4) var output: texture_storage_2d<rgba8unorm, write>;

fn get_rng(rng: ptr<function, u32>, bits: u32) -> u32 {
    let ret = *rng & ((1u << bits)-1u);
    *rng >>= bits;
    return ret;
}

@compute @workgroup_size(16)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&buddies) { return; }

    var buddy = buddies[id.x];
    var rng = prng[id.x];

    var force = vec2<f32>(0.0, 0.0);

    // random walk --------------------------------

    switch (get_rng(&rng, 2u)) {
        case 0u: { buddy.vel += vec2(1.0, 0.0); }
        case 1u: { buddy.vel += vec2(-1.0, 0.0); }
        default: { }
    }

    switch (get_rng(&rng, 2u)) {
        case 0u: { buddy.vel += vec2(0.0, 1.0); }
        case 1u: { buddy.vel += vec2(0.0, -1.0); }
        default: { }
    }

    // mouse force ------------------------------

    if all(info.mouse_pos != vec2(-1.0)) {
        var mult = 0.0;
        if (info.mouse_flags & MOUSE_LEFT_DOWN) != 0u {
            mult += 20.0;
        }
        if (info.mouse_flags & MOUSE_RIGHT_DOWN) != 0u {
            mult -= 20.0;
        }

        let diff_from_mouse = buddy.pos - info.mouse_pos;
        let dist_from_mouse = length(diff_from_mouse) + 0.0001;
        force += normalize(diff_from_mouse) * (mult / dist_from_mouse);
    }

    // apply physics ----------------------------

    buddy.vel += force * MASS;
    buddy.pos += buddy.vel;
    buddy.vel *= FRICTION;

    // wrapping ----------------------------------

    if buddy.pos.x < 0 {
        buddy.pos.x += WIDTH_F;
    } else if buddy.pos.x >= WIDTH_F {
        buddy.pos.x -= WIDTH_F;
    }

    if buddy.pos.y < 0 {
        buddy.pos.y += HEIGHT_F;
    } else if buddy.pos.y >= HEIGHT_F {
        buddy.pos.y -= HEIGHT_F;
    }
    
    // saturating --------------------------------

    //if buddy.pos.x < 0 {
    //    buddy.pos.x = 0.0;
    //} else if buddy.pos.x >= WIDTH_F {
    //    buddy.pos.x = WIDTH_F - 1.0;
    //}

    //if buddy.pos.y < 0 {
    //    buddy.pos.y = 0.0;
    //} else if buddy.pos.y >= HEIGHT_F {
    //    buddy.pos.y = HEIGHT_F - 1.0;
    //}

    // write to texture --------------------------

    let rounded = vec2<u32>(round(buddy.pos));
    textureStore(output, rounded, vec4(1.0, 1.0, 1.0, 1.0));
    buddies[id.x] = buddy;
}
