struct Desc {
    shape_count: u32,
    background_colour: u32,
}

struct Shape {
    bbox_min: vec2f,
    bbox_max: vec2f,
    fill_colour: u32,
    stroke_colour: u32,
    point_start: u32,
    point_end: u32,
    radius: f32,
    _pad: u32,
}

@group(0) @binding(0) var<uniform> desc: Desc;
@group(0) @binding(1) var<storage, read> points: array<vec2<f32>>;
@group(0) @binding(2) var<storage, read> shapes: array<Shape>;
@group(0) @binding(3) var screen: texture_storage_2d<rgba8unorm, write>;

// screen blending assuming premultiplied alpha
fn blend(a: vec3f, b: vec4f) -> vec3f {
    let p = a * (1.0 - b.a);
    return p+b.rgb - p*b.rgb;
}

fn line(p: vec2f, a: vec2f, b: vec2f) -> f32 {
    let pa = p-a;
    let ba = b-a;
    let h = clamp(dot(pa,ba)/dot(ba,ba), 0.0, 1.0);
    return length(pa - ba*h);
}

fn side(p: vec2f, a: vec2f, b: vec2f) -> f32 {
    return (b.y - a.y)*(p.x - a.x) + (a.x - b.x)*(p.y - a.y);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(screen);
    let size = vec2<i32>(dims);
    let pos = vec2<f32>(id.xy);
    
    var colour = unpack4x8unorm(desc.background_colour).rgb;
    
    for (var s: u32 = 0; s < desc.shape_count; s++) {
        var shape_colour = colour;
    
        let shape = shapes[s];
        if (any(pos < shape.bbox_min) || any(shape.bbox_max < pos)) { continue; }
        
        if shape.fill_colour != 0 {
            let fill_colour = unpack4x8unorm(shape.fill_colour);
        
            var side1 = false;
            var side2 = false;
            var side3 = false;
            var side4 = false;
            
            for (var i: u32 = shape.point_start; i < shape.point_end-1; i++) {
                let a = points[i];
                let b = points[i+1];
                
                var pd: vec2f;
                var pu: vec2f;
                if a.y > b.y {
                    pu = a;
                    pd = b;
                } else {
                    pu = b;
                    pd = a;
                }
                
                var pl: vec2f;
                var pr: vec2f;
                if a.x > b.x {
                    pr = a;
                    pl = b;
                } else {
                    pr = b;
                    pl = a;
                }
                
                if pd.y <= pos.y && pos.y <= pu.y {
                    let pside = (pu.y - pd.y)*(pos.x - pd.x) + (pd.x - pu.x)*(pos.y - pd.y);
                    side1 |= pside >= 0.0;
                    side2 |= pside <= 0.0;
                }
                
                if pl.x <= pos.x && pos.x <= pr.x {
                    let pside = (pl.y - pr.y)*(pos.x - pr.x) + (pr.x - pl.x)*(pos.y - pr.y);
                    side3 |= pside >= 0.0;
                    side4 |= pside <= 0.0;
                }
            }
            
            if side1 && side2 && side3 && side4 {
                shape_colour = blend(colour, fill_colour);
                if (shape.stroke_colour == shape.fill_colour) {
                    colour = shape_colour;
                    continue;
                }
            }
        }
        
        let stroke_colour = unpack4x8unorm(shape.stroke_colour);
        var factor = 0.0;
        for (var i: u32 = shape.point_start; i < shape.point_end-1; i++) {
            let a = points[i];
            let b = points[i+1];
            
            let pmin = min(a, b);
            let pmax = max(a, b);
            let bbox_min = pmin - shape.radius - 0.5;
            let bbox_max = pmax + shape.radius + 0.5;
            if (any(pos < bbox_min) || any(bbox_max < pos)) { continue; }
            
            let dist = line(pos, a, b);
            let aa = clamp(shape.radius - dist, 0.0, 0.5) * 2.0;
            factor = max(factor, aa);
        }
        
        if (factor == 1.0) {
            // overwrite any fill
            shape_colour = colour;
        } 
        colour = blend(shape_colour, stroke_colour * factor);
    }
    
    colour = clamp(colour, vec3f(0.0), vec3f(1.0));
    textureStore(screen, id.xy, vec4f(colour, 1.0));
}
