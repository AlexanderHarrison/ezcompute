use ezcompute::*;

const W: u32 = 1024;
const H: u32 = 1024;

#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
#[repr(C)]
struct WorldBounds {
    pub pos: [f32; 2],
    pub size: [f32; 2],
}

fn main() {
    let ctx = Ctx::new();

    let texture = ctx.create_storage_texture((W*2, H*2), StorageTextureFormat::Rgba8Unorm);
    let screenshot_texture = ctx.create_texture((W, H), wgpu::TextureFormat::Rgba8UnormSrgb);

    let mut bounds = WorldBounds {
        pos: [0.0; 2],
        size: [4.0; 2],
    };
    let bounds_uniform = ctx.create_uniform(&bounds);

    let render = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[PipelineInput::Uniform(&bounds_uniform)],
        outputs: &[ComputePipelineOutput::StorageTexture(&texture)],
        shader: include_str!("shader.wgsl").into(),
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

    // We use run_ex to separate the update loop from the render loop.
    // This allows us to update at a constant 60Hz while render iterations may take longer.
    const SCROLL_FACTOR: f32 = 1.1;
    ctx.run_ex((W, H), 60, |ev| match ev {
        WindowEvent::Update { input, .. } => {
            let mut bounds_updated = false;
            if input.mouse_scroll != 0.0f32 {
                let factor = SCROLL_FACTOR.powf(-input.mouse_scroll);
                bounds.size[0] *= factor;
                bounds.size[1] *= factor;
                bounds_updated = true;
            }

            if let Some(pos) = input.mouse_position {
                if input.mouse_buttons.left == Some(KeyState::JustPressed) {
                    translation_state = Some(TranslationState {
                        origin_click: [pos.0, pos.1],
                        origin_world: bounds.pos,
                    });
                } else if input.mouse_buttons.left == Some(KeyState::Held) {
                    let state = translation_state.unwrap();
                    let diff_x = pos.0 - state.origin_click[0];
                    let diff_y = pos.1 - state.origin_click[1];
                    let new_pos_x = state.origin_world[0] - diff_x*bounds.size[0]/ W as f32;
                    let new_pos_y = state.origin_world[1] - diff_y*bounds.size[1]/ H as f32;
                    bounds.pos[0] = new_pos_x;
                    bounds.pos[1] = new_pos_y;
                    bounds_updated = true;
                } else {
                    translation_state = None;
                }
            }

            if input.just_pressed(Key::KeyS) {
                let mut encoder = ctx.device.create_command_encoder(&Default::default());
                ctx.copy_texture_to_texture(&mut encoder, &screenshot_copier);
                ctx.queue.submit(std::iter::once(encoder.finish()));
                screenshot_texture.read_to_png(&ctx, std::path::Path::new("mandelbrot.png"));
                println!("Saved screenshot!");
            }

            if input.just_pressed(Key::KeyQ) { 
                Some(WindowTaskEx::Exit) 
            } else if bounds_updated { 
                bounds_uniform.update(&ctx, &bounds);
                Some(WindowTaskEx::Redraw)
            } else {
                None
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
