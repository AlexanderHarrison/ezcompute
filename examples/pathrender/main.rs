use ezcompute::*;
use glam::{Vec2, Affine2};

const F: f32 = 1.0;
const W: u32 = (256.0 * F) as u32;
const H: u32 = W;

#[derive(Copy, Clone, Debug, PartialEq, bytemuck::NoUninit)]
#[repr(C)]
pub struct Colour {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Colour {
    pub const NONE: Colour = Colour { r: 0, g: 0, b: 0, a: 0 };
    pub const BLACK: Colour = Colour { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Colour = Colour { r: 255, g: 255, b: 255, a: 255 };

    pub const fn new(r: u8, g: u8, b: u8) -> Colour {
        Colour { r, g, b, a: 255 }
    }
    
    pub const fn with_alpha(self, a: u8) -> Colour {
        let f = a as f32 / 255.0;
        Colour {
            r: (self.r as f32 * f) as u8,
            g: (self.g as f32 * f) as u8,
            b: (self.b as f32 * f) as u8,
            a,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, bytemuck::NoUninit)]
#[repr(C)]
pub struct Shape {
    pub bbox_min: Vec2,
    pub bbox_max: Vec2,
    
    pub fill_colour: Colour,
    pub stroke_colour: Colour,
    
    // MUST have at least two points
    pub point_start: u32,
    pub point_end: u32,
    
    pub stroke_radius: f32,
    pub _pad: u32,
}

pub struct Scene {
    pub points: Vec<Vec2>,
    // pub cmds: Vec<Cmd>,
    pub shapes: Vec<Shape>,
}

#[derive(Copy, Clone, Debug, PartialEq, bytemuck::NoUninit)]
#[repr(C)]
struct Desc {
    shape_count: u32,
    background: Colour,
}

fn pmin(p: &[Vec2]) -> Vec2 { p.iter().fold(Vec2::INFINITY, |a, b| a.min(*b)) }
fn pmax(p: &[Vec2]) -> Vec2 { p.iter().fold(Vec2::NEG_INFINITY, |a, b| a.max(*b)) }

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PathEl {
    MoveTo(Vec2),
    LineTo(Vec2),
    QuadTo(Vec2, Vec2),
    CubicTo(Vec2, Vec2, Vec2),
    ClosePath,
}

fn linearize(
    scratch: &mut [Vec2; 64],
    points: &mut Vec<Vec2>
) {
    const TOLERANCE: f32 = 0.9995;
    
    points.push(scratch[0]);
    let mut i = 0;
    while i < 63 {
        let a = scratch[i];
        let b = scratch[i+1];
        let dir = (b - a).normalize();
        
        i += 1;
        while i < 63 {
            let c = scratch[i+1];
            let v = (c - a).normalize();
            if dir.dot(v).abs() < TOLERANCE { break; }
            i += 1;
        }
        points.push(scratch[i]);
    }
    
    let last = scratch[63];
    if *points.last().unwrap() != last { points.push(last); }
}

fn add_quad(
    scratch: &mut [Vec2; 64],
    points: &mut Vec<Vec2>,
    p0: Vec2,
    p1: Vec2,
    p2: Vec2,
) {
    let p01 = p0 - p1;
    let p21 = p2 - p1;
    for i in 0..64 {
        let t = i as f32 / 64.0;
        let tp = 1.0 - t;
        scratch[i] = p1 + tp*tp*p01 + t*t*p21;
    }
    linearize(scratch, points);
}

fn add_cubic(
    scratch: &mut [Vec2; 64],
    points: &mut Vec<Vec2>,
    p0: Vec2,
    p1: Vec2,
    p2: Vec2,
    p3: Vec2,
) {
    for i in 0..64 {
        let t = i as f32 / 64.0;
        let t2 = t*t;
        let tp = 1.0 - t;
        let tp2 = tp*tp;
        scratch[i] = tp2*tp*p0 + (3.0*tp2)*(t*p1) + (3.0*tp)*(t2*p2) + t2*t*p3;
    }
    linearize(scratch, points);
}

impl Scene {
    pub fn new() -> Scene {
        Scene {
            points: Vec::new(),
            shapes: Vec::new(),
        }
    }
    
    pub fn clear(&mut self) {
        self.points.clear();
        self.shapes.clear();
    }
    
    pub fn circle(
        &mut self,
        centre: Vec2,
        radius: f32,
        fill_colour: Colour,
        stroke_colour: Colour,
        stroke_radius: f32,
    ) {
        let point_start = self.points.len();
        self.points.reserve(129);
        for i in 0..=128 {
            let p = (i as f32) / 128.0 * 2.0 * 3.141592654;
            let x = p.cos() * radius;
            let y = p.sin() * radius;
            self.points.push(Vec2::new(x, y) + centre);
        }
        let point_end = self.points.len();
        let bbox_min = pmin(&self.points[point_start..point_end]) - stroke_radius - 1.0;
        let bbox_max = pmax(&self.points[point_start..point_end]) + stroke_radius + 1.0;
        
        self.shapes.push(Shape {
            bbox_min, bbox_max,
            stroke_colour,
            fill_colour,
            point_start: point_start as _,
            point_end: point_end as _,
            stroke_radius,
            _pad: 0,
        });
    }
    
    pub fn path(
        &mut self,
        fill_colour: Colour,
        stroke_colour: Colour,
        stroke_radius: f32,
        cmds: &[PathEl],
        transform: Affine2,
    ) {
        let mut scratch = [Vec2::ZERO; 64];
        let mut cmd_i = 0;
        
        let mut pos = Vec2::ZERO;
        let mut point_start = self.points.len();
        while cmd_i < cmds.len() {
            // start shape
            let next_start_pos = loop {
                if cmd_i >= cmds.len() { break Vec2::ZERO; }
                let cmd = cmds[cmd_i];
                cmd_i += 1;
                match cmd {
                    PathEl::MoveTo(new_pos) => {
                        let new_pos = transform.transform_point2(new_pos);
                        break new_pos;
                    },
                    PathEl::ClosePath => {
                        self.points.push(pos);
                        pos = self.points[point_start];
                        break pos;
                    },
                    PathEl::LineTo(new_pos) => {
                        let new_pos = transform.transform_point2(new_pos);
                        self.points.push(pos);
                        pos = new_pos;
                    },
                    PathEl::QuadTo(p1, p2) => {
                        let p1 = transform.transform_point2(p1);
                        let p2 = transform.transform_point2(p2);
                        add_quad(&mut scratch, &mut self.points, pos, p1, p2);
                        pos = p2;
                    },
                    PathEl::CubicTo(p1, p2, p3) => {
                        let p1 = transform.transform_point2(p1);
                        let p2 = transform.transform_point2(p2);
                        let p3 = transform.transform_point2(p3);
                        add_cubic(&mut scratch, &mut self.points, pos, p1, p2, p3);
                        pos = p3;
                    },
                }
            };
            let end = pos;
            pos = next_start_pos;
            
            // finish shape
            if point_start == self.points.len() { continue; }
            self.points.push(end);
            let point_end = self.points.len();
            let bbox_min = pmin(&self.points[point_start..point_end]) - stroke_radius - 1.0;
            let bbox_max = pmax(&self.points[point_start..point_end]) + stroke_radius + 1.0;
            
            self.shapes.push(Shape {
                bbox_min,
                bbox_max,
                point_start: point_start as _,
                point_end: point_end as _,
                stroke_colour,
                stroke_radius,
                fill_colour,
                _pad: 0,
            });
            
            point_start = point_end;
        }
    }
}

mod gc_icons;

fn main() {
    const F: f32 = 2.0;
    let ctx = Ctx::new();
    
    let tform = Affine2::from_scale_angle_translation(
        Vec2::splat(1.5),
        0.0,
        Vec2::ZERO
    );
    
    let parts = [
        (gc_icons::l, Colour::new(200, 200, 200)),
        (gc_icons::r, Colour::new(200, 200, 200)),
        (gc_icons::z, Colour::new(140, 50, 140)),
        (gc_icons::body, Colour::new(255, 100, 100)),
        (gc_icons::lpanel, Colour::new(255, 100, 100)),
        (gc_icons::cpanel, Colour::new(255, 100, 100)),
        
        (gc_icons::x, Colour::new(200, 200, 200)),
        (gc_icons::y, Colour::new(200, 200, 200)),
        (gc_icons::a, Colour::new(50, 220, 50)),
        (gc_icons::b, Colour::new(220, 50, 50)),
        
        (gc_icons::lgate, Colour::new(200, 200, 200)),
        (gc_icons::lstick, Colour::new(200, 200, 200)),
        (gc_icons::cgate, Colour::new(200, 200, 50)),
        (gc_icons::cstick, Colour::new(200, 200, 50)),
        
        (gc_icons::dpad, Colour::new(200, 200, 200)),
        (gc_icons::du, Colour::new(200, 200, 200)),
        (gc_icons::dd, Colour::new(200, 200, 200)),
        (gc_icons::dl, Colour::new(200, 200, 200)),
        (gc_icons::dr, Colour::new(200, 200, 200)),
        (gc_icons::start, Colour::new(200, 200, 200)),
    ];
    
    let mut scene = Scene::new();
    for (part, fill) in parts {
        scene.path(
            fill,
            Colour::new(10, 10, 10), // stroke
            1.5, // stroke radius
            part.elements,
            tform,
        );
    }
    
    // scene.circle(
    //     Vec2::new(W as f32 / 2.0, H as f32 / 2.0),
    //     35.0 * F,
    //     Colour::new(200, 100, 200).with_alpha(200), // fill
    //     Colour::new(255, 100, 100), // stroke
    //     5.0 * F,
    // );
    // scene.circle(
    //     Vec2::new(W as f32 / 4.0, H as f32 / 4.0),
    //     25.0 * F,
    //     Colour::new(10, 200, 100).with_alpha(200), // fill
    //     Colour::new(100, 100, 255).with_alpha(200), // stroke
    //     5.0 * F,
    // );
    
    let desc = Desc {
        shape_count: scene.shapes.len() as _,
        background: Colour::new(230, 230, 230),
    };
    
    let texture = ctx.create_storage_texture((W, H), StorageTextureFormat::Rgba8Unorm);
    let copier = ctx.create_screen_copier(&texture, ScalingType::Nearest);
    
    let desc_uni = ctx.create_uniform(&desc);
    let points_buf = ctx.create_storage_buffer_ex::<Vec2>(Either::B(4096), wgpu::BufferUsages::empty());
    let shapes_buf = ctx.create_storage_buffer_ex::<Shape>(Either::B(4096), wgpu::BufferUsages::empty());

    let render = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[
            PipelineInput::Uniform(&desc_uni),
            PipelineInput::StorageBuffer(&points_buf),
            PipelineInput::StorageBuffer(&shapes_buf),
        ],
        outputs: &[
            ComputePipelineOutput::StorageTexture(&texture),
        ],
        shader: ShaderSource::File(std::path::Path::new("examples/pathrender/shader.wgsl")),
        shader_entry: "main",
        dispatch_count: texture.dispatch_count((8, 8)),
    });
    
    let mut timer = ctx.create_timer();
    let mut n = 0.0;
    ctx.run((W, H), 60, |encoder, output, delta, input| {
        /*scene.clear();
        n += delta;
        
        let b = Vec2::new(n.cos(), n.sin()) * 50.0;
        
        scene.path(
            Colour::new(200, 100, 200).with_alpha(200), // fill
            Colour::new(255, 100, 100), // stroke
            5.0, // stroke radius
            &[
                PathEl::MoveTo(Vec2::new(100.0, 100.0)),
                PathEl::QuadTo(&(
                    Vec2::new(150.0, 50.0) + b,
                    Vec2::new(200.0, 100.0)
                )),
                PathEl::QuadTo(&(
                    Vec2::new(250.0, 150.0) + b,
                    Vec2::new(200.0, 200.0)
                )),
                PathEl::QuadTo(&(
                    Vec2::new(150.0, 250.0) + b,
                    Vec2::new(100.0, 200.0)
                )),
                PathEl::CubicTo(&(
                    Vec2::new(150.0, 150.0) + b,
                    Vec2::new(50.0,  150.0) + b,
                    Vec2::new(100.0, 100.0)
                )),
            ]
        );*/
    
        points_buf.update(&ctx, &scene.points.as_slice());
        shapes_buf.update(&ctx, &scene.shapes.as_slice());
    
        timer.start(encoder);
        ctx.run_compute_pass(encoder, &[&render]);
        timer.split(encoder, "compute pass");
        timer.print(encoder);
        
        ctx.copy_texture_to_screen(encoder, &copier, output);
        
        if input.just_pressed(Key::Escape) { Some(WindowTask::Exit) } else { None }
    });
}
