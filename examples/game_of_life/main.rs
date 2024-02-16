use ezcompute::*;

const W: u32 = 256;
const H: u32 = 256;
const CELL_SIZE: u32 = 4;
const SCREEN_SIZE: (u32, u32) = (W*CELL_SIZE, H*CELL_SIZE);

fn main() {
    let ctx = Ctx::new();

    let mut rng = fastrand::Rng::new();
    let mut initial_cells = [0u32; (W*H) as usize];
    for cell in initial_cells.iter_mut() {
        *cell = rng.bool() as u32;
    }

    let cells_texture = ctx.create_storage_texture_with_data((W, H), wgpu::TextureFormat::R32Uint, &initial_cells);
    let cells_texture_out = ctx.create_storage_texture((W, H), wgpu::TextureFormat::R32Uint);
    let screen_texture = ctx.create_storage_texture((W, H), wgpu::TextureFormat::Rgba8Unorm);

    let update_pipeline = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[
            PipelineInput::StorageTexture(&cells_texture),
        ],
        outputs: &[
            ComputePipelineOutput::StorageTexture(&cells_texture_out),
            ComputePipelineOutput::StorageTexture(&screen_texture),
        ],
        shader_file: std::path::Path::new("examples/game_of_life/update.wgsl"),
        shader_entry: "update",
        dispatch_count: cells_texture.dispatch_count((16, 16)),
    }).unwrap();

    let back_copy = ctx.create_texture_copier(&cells_texture_out, &cells_texture);
    let screen_copy = ctx.create_screen_copier(&screen_texture);

    let mut running = true;
    let mut step = false;

    ctx.run(SCREEN_SIZE, 20, |encoder, output, _delta, keys| {
        if keys.just_pressed(Key::Space) {
            running = !running;
        }

        if keys.just_pressed(Key::Period) {
            step = true;
        }

        if running || step {
            ctx.run_compute_pass(encoder, &[&update_pipeline]);
            ctx.copy_texture_to_texture(encoder, &back_copy);
            if step { running = false; step = false }
        }
        ctx.copy_texture_to_screen(encoder, &screen_copy, output);

        if keys.just_pressed(Key::KeyQ) || keys.just_pressed(Key::Escape) {
            Some(WindowTask::Exit)
        } else {
            None
        }
    })
}
