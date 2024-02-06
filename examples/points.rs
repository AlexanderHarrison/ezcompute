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
    let field_texture = ctx.create_storage_texture((W, H), wgpu::TextureFormat::Rg32Float);
    let screen_texture = ctx.create_storage_texture((W, H), wgpu::TextureFormat::Rgba8Unorm);

    let update_points_pipeline = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[
            PipelineInput::StorageBuffer(&points_buffer),
            PipelineInput::StorageTexture(&field_texture),
        ],
        outputs: &[
            ComputePipelineOutput::StorageBuffer(&new_points_buffer),
        ],
        shader_file: std::path::Path::new("examples/points/move.wgsl"),
        shader_entry: "write_points",
        dispatch_count: [(POINTS.len() as u32+63)/64, 1, 1],
    }).unwrap();

    let field_creation_pipeline = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[
            PipelineInput::StorageBuffer(&new_points_buffer),
        ],
        outputs: &[
            ComputePipelineOutput::StorageTexture(&field_texture),
            ComputePipelineOutput::StorageTexture(&screen_texture),
        ],
        shader_file: std::path::Path::new("examples/points/field.wgsl"),
        shader_entry: "calculate_field",
        dispatch_count: [(W+15)/16, (H+15)/16, 1],
    }).unwrap();

    let copy_bind_group = ctx.create_copy_source_bind_group(&screen_texture);

    ctx.record(
        "video.mp4",
        (W, H),
        300,
        60,
        |output_texture| {
            let mut encoder = ctx.device.create_command_encoder(&Default::default());

            ctx.clear_texture(&mut encoder, &field_texture);

            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                ctx.run_compute_pipeline(&mut pass, &update_points_pipeline);
                ctx.run_compute_pipeline(&mut pass, &field_creation_pipeline);
            }

            ctx.copy_texture_to_texture(&mut encoder, &copy_bind_group, &output_texture.view, wgpu::Color::BLACK);

            //encoder.copy_texture_to_texture(
            //    screen_texture.texture.as_image_copy(),
            //    output_texture.texture.as_image_copy(),
            //    screen_texture.texture.size(),
            //);

            ctx.queue.submit(std::iter::once(encoder.finish()));
        }
    );


    //screen_texture.read_to_png(&ctx, std::path::Path::new("output.png"));

    //let copy_bind_group = ctx.create_copy_source_bind_group(&screen_texture);
    //{
    //    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    //    ctx.clear_texture(&mut encoder, &field_texture)
    //}
    //let window = Window::new(&ctx, (W, H));
    //window.run(&ctx, 60.0, |event| {
    //    match event {
    //        WindowEvent::Redraw { window, surface } => {
    //            let surface_texture = surface.get_current_texture().unwrap();
    //            //let view = surface_texture.texture.create_view(&Default::default());

    //            let mut encoder = ctx.device.create_command_encoder(&Default::default());

    //            {
    //                let mut pass = encoder.begin_compute_pass(&Default::default());
    //                ctx.run_compute_pipeline(&mut pass, &update_points_pipeline);
    //                ctx.run_compute_pipeline(&mut pass, &field_creation_pipeline);
    //            }

    //            ctx.copy_buffer_to_buffer(&mut encoder, &new_points_buffer, &points_buffer);
    //            ctx.copy_texture_to_screen(&mut encoder, &copy_bind_group, &surface_texture, wgpu::Color::BLACK);
    //            
    //            ctx.queue.submit(std::iter::once(encoder.finish()));
    //            window.pre_present_notify();
    //            surface_texture.present();
    //            None
    //        },
    //        WindowEvent::Update { delta: _, keys } => {
    //            if keys.iter().any(|k|
    //                *k == ("q", KeyState::JustPressed)
    //                    || *k == (winit::keyboard::NamedKey::Escape, KeyState::JustPressed)
    //            ) { 
    //                return Some(WindowTask::Exit) 
    //            }

    //            Some(WindowTask::Redraw)
    //        },
    //    }
    //})
}
