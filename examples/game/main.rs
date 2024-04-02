use ezcompute::*;

const W: u32 = 1920 / 4;
const H: u32 = 1080 / 4;
const SCALE: u32 = 3;

#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
#[repr(C)]
struct Buddy {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
}
const ZERO_BUDDY: Buddy = Buddy { pos: [0.0; 2], vel: [0.0; 2] };

const BUDDY_COUNT: usize = 1080 * 2;

#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
#[repr(C)]
struct Info {
    pub mouse_pos: [f32; 2],
    pub mouse_flags: u32,
    pub _pad: u32,
}

const MOUSE_LEFT_DOWN: u32 = 1 << 0;
const MOUSE_RIGHT_DOWN: u32 = 1 << 1;

fn f32_rng_buf(rng: &mut fastrand::Rng, buf: &mut [f32]) {
    rng.fill(bytemuck::cast_slice_mut(buf));

    for i in 0..buf.len() {
        let n: u32 = bytemuck::cast(buf[i]);
        let b = 32;
        let f = core::f32::MANTISSA_DIGITS - 1;
        buf[i] = f32::from_bits((1 << (b - 2)) - (1 << f) + (n >> (b - f))) - 1.0;
    }
}

fn main() {
    let ctx = Ctx::new();

    let mut rng = fastrand::Rng::new();

    let mut prng = [0u32; BUDDY_COUNT];
    rng.fill(bytemuck::cast_slice_mut(&mut prng));

    let mut buddies = [ZERO_BUDDY; BUDDY_COUNT];
    for i in 0..BUDDY_COUNT { 
        buddies[i].pos = [fastrand::f32()*(W as f32), fastrand::f32()*(H as f32)]; 
    }

    let prng_buf = ctx.create_storage_buffer(&prng);
    let info_uniform = ctx.create_uniform(&Info {
        mouse_pos: [-1.0, -1.0],
        mouse_flags: 0,
        _pad: 0,
    });
    let buddies_buf = ctx.create_storage_buffer(&buddies);
    let texture = ctx.create_storage_texture((W, H), StorageTextureFormat::Rgba8Unorm);

    let mut prng_tex_data = [0f32; (2*W*H) as usize];
    let prng_texture = ctx.create_storage_texture((W, H), StorageTextureFormat::Rg32Float);

    let field = ctx.create_storage_texture((W, H), StorageTextureFormat::Rgba32Float);
    let field_out = ctx.create_storage_texture((W, H), StorageTextureFormat::Rgba32Float);

    let update_field = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[
            PipelineInput::Uniform(&info_uniform),
            PipelineInput::StorageTexture(&prng_texture),
            PipelineInput::StorageTexture(&field),
        ],
        outputs: &[
            ComputePipelineOutput::StorageTexture(&field_out),
            ComputePipelineOutput::StorageTexture(&texture),
        ],
        shader: ShaderSource::File(std::path::Path::new("examples/game/per_unit.wgsl")),
        shader_entry: "update",
        dispatch_count: field.dispatch_count((16, 16)),
    });

    let update_buddies = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[
            PipelineInput::StorageBuffer(&prng_buf),
            PipelineInput::Uniform(&info_uniform),
            PipelineInput::StorageTexture(&field_out),
        ],
        outputs: &[
            ComputePipelineOutput::StorageBuffer(&buddies_buf),
            ComputePipelineOutput::StorageTexture(&texture),
        ],
        shader: ShaderSource::File(std::path::Path::new("examples/game/per_buddy.wgsl")),
        shader_entry: "update",
        dispatch_count: buddies_buf.dispatch_count(16),
    });

    let screen_copier = ctx.create_screen_copier(&texture, ScalingType::Nearest);
    let field_copier = ctx.create_texture_copier_fast(&field_out, &field);

    ctx.run_ex((W*SCALE, H*SCALE), 60, |ev| match ev {
        WindowEvent::Update { input, .. } => {
            info_uniform.update(&ctx, &Info {
                mouse_pos: match input.mouse_position {
                    None => [-1.0, -1.0],
                    Some((x, y)) => [
                        (x / SCALE as f32), 
                        (y / SCALE as f32),
                    ],
                },
                mouse_flags: {
                    (if input.mouse_buttons.left.is_some() { MOUSE_LEFT_DOWN } else { 0 })
                    ^ (if input.mouse_buttons.right.is_some() { MOUSE_RIGHT_DOWN } else { 0 })
                },
                _pad: 0,
            });

            if input.just_pressed(Key::KeyQ) { 
                Some(WindowTaskEx::Exit) 
            } else {
                Some(WindowTaskEx::Redraw)
            }
        },
        WindowEvent::Redraw { output } => {
            let mut encoder = ctx.device.create_command_encoder(&Default::default());
            ctx.clear_texture(&mut encoder, &texture);

            //rng.fill(bytemuck::cast_slice_mut(&mut prng));
            //prng_buf.update(&ctx, &prng);
            //ctx.run_compute_pass(&mut encoder, &[&update_field, &update_buddies]);
            
            f32_rng_buf(&mut rng, &mut prng_tex_data);
            prng_texture.update(&ctx, &prng_tex_data);
            ctx.run_compute_pass(&mut encoder, &[&update_field]);
            ctx.copy_texture_to_screen(&mut encoder, &screen_copier, output);
            ctx.copy_texture_to_texture(&mut encoder, &field_copier);
            ctx.queue.submit(std::iter::once(encoder.finish()));

            None
        }
    });
}
