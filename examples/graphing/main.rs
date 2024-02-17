use ezcompute::*;

const W: u32 = 512;
const H: u32 = 512;

#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
#[repr(C)]
struct WorldBounds {
    pub pos: [f32; 2],
    pub size: [f32; 2],
}

fn main() {
    let ctx = Ctx::new();

    let points = ctx.create_storage_buffer_empty::<[f32; 2]>(W as _);
    let vertex_count = points.len() * 2 - 2;
    let vertex_buffer = ctx.create_storage_buffer_ex::<[f32; 2]>(
        Either::B(vertex_count as _),
        wgpu::BufferUsages::VERTEX,
    );

    let mut bounds = WorldBounds { pos: [0.0, 0.0], size: [500.0, 500.0] };
    let bounds_uniform = ctx.create_uniform(&bounds);

    let path_strip_create = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[PipelineInput::StorageBuffer(&points), PipelineInput::Uniform(&bounds_uniform)],
        outputs: &[ComputePipelineOutput::StorageBuffer(&vertex_buffer)],
        shader: include_str!("path_create.wgsl").into(),
        shader_entry: "path_create",
        dispatch_count: points.dispatch_count(32),
    });

    let points_create = ctx.create_compute_pipeline(ComputePipelineDescriptor {
        inputs: &[PipelineInput::Uniform(&bounds_uniform)],
        outputs: &[ComputePipelineOutput::StorageBuffer(&points)],
        shader: include_str!("function.wgsl").into(),
        shader_entry: "points_create",
        dispatch_count: points.dispatch_count(32),
    });

    let vbuffer = VertexBuffer {
        vertex_buffer: vertex_buffer.buffer,
        vertex_layout: wgpu::VertexBufferLayout {
            array_stride: vertex_buffer.layout.size() as _,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array!(0 => Float32x2),
        },
        vertex_count,
        index_buffer: None,
        primitives: wgpu::PrimitiveTopology::TriangleStrip,
    };

    let path_strip_render = ctx.create_render_pipeline(RenderPipelineDescriptor {
        inputs: &[],
        vertex_buffer: &vbuffer,
        shader: include_str!("path_render.wgsl").into(),
        shader_vertex_entry: "vertex",
        shader_fragment_entry: "fragment",
        output_format: OUTPUT_TEXTURE_FORMAT,
    });

    #[derive(Copy, Clone)]
    struct TranslationState {
        pub origin_click: [f32; 2],
        pub origin_world: [f32; 2],
    }
    let mut translation_state: Option<TranslationState> = None;

    let mut timer = ctx.create_timer();
    
    ctx.run((W, H), 60, |encoder, output, _delta, input| {
        const SCROLL_FACTOR: f32 = 1.1;

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
                let new_pos_y = state.origin_world[1] + diff_y*bounds.size[1]/ H as f32;
                bounds.pos[0] = new_pos_x;
                bounds.pos[1] = new_pos_y;
                bounds_updated = true;
            } else {
                translation_state = None;
            }
        }

        if bounds_updated {
            bounds_uniform.update(&ctx, &bounds);
        }

        timer.start(encoder);
        ctx.run_compute_pass(encoder, &[&points_create, &path_strip_create]);
        timer.split(encoder, "compute pass");
        ctx.run_render_pass(encoder, output, wgpu::Color::BLACK, &[&path_strip_render]);
        timer.split(encoder, " render pass");
        timer.print(encoder);

        if input.just_pressed(Key::KeyQ) { Some(WindowTask::Exit) } else { None }
    });
}
