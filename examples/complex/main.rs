use ezcompute::*;

const W: u32 = 1024;
const H: u32 = 1024;

#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
#[repr(C)]
struct Data {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub time: f32,
    pub _padding: f32,
}

fn main() {
    let ctx = Ctx::new();

    let texture = ctx.create_storage_texture((W*2, H*2), StorageTextureFormat::Rgba8Unorm);
    let screenshot_texture = ctx.create_texture((W, H), wgpu::TextureFormat::Rgba8UnormSrgb);

    let mut data = Data {
        pos: [0.0; 2],
        size: [4.0; 2],
        time: 0.0,
        _padding: 0.0,
    };
    let data_uniform = ctx.create_uniform(&data);

    let render = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[PipelineInput::Uniform(&data_uniform)],
        outputs: &[ComputePipelineOutput::StorageTexture(&texture)],
        shader: ShaderSource::File(std::path::Path::new("examples/complex/shader.wgsl")),
        shader_entry: "render",
        dispatch_count: texture.dispatch_count((16, 16)),
    });

    let screen_copier = ctx.create_screen_copier(&texture, ScalingType::Linear);
    let screenshot_copier = ctx.create_texture_copier(&texture, &screenshot_texture, ScalingType::Linear);
    let mut timer = ctx.create_timer();

    #[derive(Copy, Clone)]
    struct TranslationState {
        pub origin_click: [f32; 2],
        pub origin_world: [f32; 2],
    }
    let mut translation_state: Option<TranslationState> = None;
    let start_time = std::time::Instant::now();

    // We use run_ex to separate the update loop from the render loop.
    // This allows us to update at a constant 60Hz while render iterations may take longer.
    const SCROLL_FACTOR: f32 = 1.1;
    ctx.run_ex((W, H), 60, |ev| match ev {
        WindowEvent::Update { input, .. } => {
            if input.mouse_scroll != 0.0f32 {
                let factor = SCROLL_FACTOR.powf(-input.mouse_scroll);
                data.size[0] *= factor;
                data.size[1] *= factor;
            }

            if let Some(pos) = input.mouse_position {
                if input.mouse_buttons.left == Some(KeyState::JustPressed) {
                    translation_state = Some(TranslationState {
                        origin_click: [pos.0, pos.1],
                        origin_world: data.pos,
                    });
                } else if input.mouse_buttons.left == Some(KeyState::Held) {
                    let state = translation_state.unwrap();
                    let diff_x = pos.0 - state.origin_click[0];
                    let diff_y = pos.1 - state.origin_click[1];
                    let new_pos_x = state.origin_world[0] - diff_x*data.size[0]/ W as f32;
                    let new_pos_y = state.origin_world[1] - diff_y*data.size[1]/ H as f32;
                    data.pos[0] = new_pos_x;
                    data.pos[1] = new_pos_y;
                } else {
                    translation_state = None;
                }
            }

            data.time = start_time.elapsed().as_secs_f32();
            data_uniform.update(&ctx, &data);

            if input.just_pressed(Key::KeyS) {
                let mut encoder = ctx.device.create_command_encoder(&Default::default());
                ctx.copy_texture_to_texture(&mut encoder, &screenshot_copier);
                ctx.queue.submit(std::iter::once(encoder.finish()));
                screenshot_texture.read_to_png(&ctx, std::path::Path::new("complex.png"));
                println!("Saved screenshot!");
            }

            if input.just_pressed(Key::KeyQ) { 
                Some(WindowTaskEx::Exit) 
            } else {
                Some(WindowTaskEx::Redraw)
            }
        },
        WindowEvent::Redraw { output } => {
            let mut encoder = ctx.device.create_command_encoder(&Default::default());
            timer.start(&mut encoder);
            ctx.run_compute_pass(&mut encoder, &[&render]);
            timer.split(&mut encoder, "render");
            ctx.copy_texture_to_screen(&mut encoder, &screen_copier, output);
            timer.print(&mut encoder);
            ctx.queue.submit(std::iter::once(encoder.finish()));

            None
        }
    });
}
