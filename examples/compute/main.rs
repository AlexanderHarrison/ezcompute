use ezcompute::*;

// IMPORTANT: Why do we add padding? Why do we use vec4 in the shader?
// Wgsl always aligns vec3s to 16 bytes anyways, 
// so I wrote it this way to be explicit about the memory layout.
//
// https://sotrh.github.io/learn-wgpu/showcase/alignment/#alignment-of-vertex-and-index-buffers
#[derive(Copy, Clone, Debug, bytemuck::Zeroable)]
#[repr(C)]
struct Point {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    _padding: f32,
}

unsafe impl bytemuck::Pod for Point {}

fn random_unit_vector() -> Point {
    let x = fastrand::f32() * 2.0 - 1.0;
    let y = fastrand::f32() * 2.0 - 1.0;
    let z = fastrand::f32() * 2.0 - 1.0;

    let len = (x*x + y*y + z*z).sqrt();
    Point {
        x: x / len,
        y: y / len,
        z: z / len,
        _padding: 0.0
    }
}

fn main() {
    let ctx = Ctx::new();

    const SIZE: usize = 512;
    let mut points = Vec::with_capacity(SIZE);
    for _ in 0..SIZE {
        points.push(random_unit_vector());
    }

    let points_buffer_in = ctx.create_storage_buffer(&points);
    let points_buffer_out = ctx.create_storage_buffer_empty::<Point>(points.len());

    let update_points = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[PipelineInput::StorageBuffer(&points_buffer_in)],
        outputs: &[ComputePipelineOutput::StorageBuffer(&points_buffer_out)],
        shader: include_str!("shader.wgsl").into(),
        shader_entry: "update",
        dispatch_count: points_buffer_in.dispatch_count(16),
    });

    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    for _ in 0..1000 {
        ctx.run_compute_pass(&mut encoder, &[&update_points]);
        ctx.copy_buffer_to_buffer(&mut encoder, &points_buffer_out, &points_buffer_in);
    }
    ctx.queue.submit(std::iter::once(encoder.finish()));

    let points = points_buffer_in.read_to_vec::<Point>(&ctx);

    for p in points {
        println!("{}, {}, {}", p.x, p.y, p.z);
    }
}

