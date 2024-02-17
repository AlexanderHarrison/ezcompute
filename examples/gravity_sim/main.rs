use ezcompute::*;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
struct Point {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub mass: f32,
    pub padding: [f32; 3],
}

const W: u32 = 600;
const H: u32 = 600;

fn main() {
    assert!(std::mem::size_of::<Point>() == 4*8);
    let ctx = Ctx::new();

    const POINTS: &'static [Point] = &[
        Point { pos: [100.0, 100.0], vel: [0.3, 0.0], mass: 10.0, padding: [0.0; 3]},
        Point { pos: [200.0, 400.0], vel: [0.0, -0.3], mass: 20.0, padding: [0.0; 3]},
        Point { pos: [400.0, 300.0], vel: [0.3, 0.0], mass: 10.0, padding: [0.0; 3]},
        Point { pos: [200.0, 300.0], vel: [0.0, 0.3], mass: 30.0, padding: [0.0; 3]},
        Point { pos: [400.0, 200.0], vel: [-0.3, 0.3], mass: 5.0, padding: [0.0; 3]},
        Point { pos: [300.0, 300.0], vel: [0.0, 0.0], mass: 200.0, padding: [0.0; 3]},
        Point { pos: [200.0, 100.0], vel: [-0.3, 0.0], mass: 10.0, padding: [0.0; 3]},
    ];
    let points_buffer = ctx.create_storage_buffer(&POINTS);
    let new_points_buffer = ctx.create_storage_buffer(&POINTS);
    let field_texture = ctx.create_storage_texture((W, H), StorageTextureFormat::Rg32Float);
    let screen_texture = ctx.create_storage_texture((W, H), StorageTextureFormat::Rgba8Unorm);

    let update_points_pipeline = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[
            PipelineInput::StorageBuffer(&points_buffer),
            PipelineInput::StorageTexture(&field_texture),
        ],
        outputs: &[
            ComputePipelineOutput::StorageBuffer(&new_points_buffer),
        ],
        shader: include_str!("move.wgsl").into(),
        shader_entry: "write_points",
        dispatch_count: points_buffer.dispatch_count(64),
    });

    let field_creation_pipeline = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[
            PipelineInput::StorageBuffer(&new_points_buffer),
        ],
        outputs: &[
            ComputePipelineOutput::StorageTexture(&field_texture),
            ComputePipelineOutput::StorageTexture(&screen_texture),
        ],
        shader: include_str!("field.wgsl").into(),
        shader_entry: "calculate_field",
        dispatch_count: field_texture.dispatch_count((16, 16))
    });

    //ctx.record(
    //    "video.mp4",
    //    (W, H),
    //    300,
    //    60,
    //    |output| {
    //        let mut encoder = ctx.device.create_command_encoder(&Default::default());

    //        ctx.clear_texture(&mut encoder, &field_texture);
    //        ctx.run_compute_pass(&mut encoder, &[&update_points_pipeline, &field_creation_pipeline]);
    //        ctx.copy_texture_to_texture(&mut encoder, &screen_texture, &output);

    //        //encoder.copy_texture_to_texture(
    //        //    screen_texture.texture.as_image_copy(),
    //        //    output_texture.texture.as_image_copy(),
    //        //    screen_texture.texture.size(),
    //        //);

    //        ctx.queue.submit(std::iter::once(encoder.finish()));
    //    }
    //);

    //screen_texture.read_to_png(&ctx, std::path::Path::new("output.png"));

    {
        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        ctx.clear_texture(&mut encoder, &field_texture);
        ctx.queue.submit(std::iter::once(encoder.finish()));
    }

    let screen_copier = ctx.create_screen_copier(&screen_texture, ScalingType::Nearest);

    ctx.run((W, H), 60, |encoder, output, _, _| {
        ctx.run_compute_pass(encoder, &[&update_points_pipeline, &field_creation_pipeline]);
        ctx.copy_buffer_to_buffer(encoder, &new_points_buffer, &points_buffer);
        ctx.copy_texture_to_screen(encoder, &screen_copier, output);

        None
    })
}
