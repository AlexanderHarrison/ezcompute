use ezcompute::*;

const W: u32 = 1024;
const H: u32 = 1024;

// IMPORTANT: Why do we add padding? Why do we use vec4 in the shader?
// Wgsl always aligns vec3s to 16 bytes anyways, 
// so I wrote it this way to be explicit about the memory layout.
//
// https://sotrh.github.io/learn-wgpu/showcase/alignment/#alignment-of-vertex-and-index-buffers
#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
#[repr(C)]
struct Point {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    _padding: f32,
}


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

#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
#[repr(C)]
struct ViewState {
    pub scale: [f32; 2],
    pub rotation: [f32; 2]
}

fn main() {
    let ctx = Ctx::new();

    const SIZE: usize = 512;
    let mut points = Vec::with_capacity(SIZE);
    for _ in 0..SIZE {
        points.push(random_unit_vector());
    }

    let points_buffer = ctx.create_storage_buffer(&points);
    let mut view_state = ViewState { scale: [0.8; 2], rotation: [0.0; 2] };
    let view_state_uniform = ctx.create_uniform(&view_state);

    // We disable depth test to allow points at the back of the sphere to draw correctly.
    // Enable it and see what happens!
    let render_points = ctx.create_render_pipeline_ex(RenderPipelineDescriptorEx {
        inputs: &[PipelineInput::StorageBuffer(&points_buffer), PipelineInput::Uniform(&view_state_uniform)],
        vertex_buffer: Either::B(wgpu::PrimitiveTopology::TriangleStrip),
        shader: ShaderSource::Str(include_str!("shader.wgsl")),
        shader_vertex_entry: "vertex",
        shader_fragment_entry: "fragment",
        output_format: OUTPUT_TEXTURE_FORMAT,
        blend_state: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        draw_range: 0..4,
        instance_range: 0..(SIZE as _),
        disable_depth_test: true,
    });

    ctx.run((W, H), 60, |encoder, output, _delta, keys| {
        let mut uniform_written = false;

        const SCALE_CHANGE_FACTOR: f32 = 1.01;
        if keys.held(Key::ArrowUp) { 
            view_state.scale[0] *= SCALE_CHANGE_FACTOR;
            view_state.scale[1] *= SCALE_CHANGE_FACTOR;
            uniform_written = true;
        }
        if keys.held(Key::ArrowDown) { 
            view_state.scale[0] *= SCALE_CHANGE_FACTOR.recip();
            view_state.scale[1] *= SCALE_CHANGE_FACTOR.recip();
            uniform_written = true;
        }

        const ROTATION_CHANGE_DELTA: f32 = 0.01;
        if keys.held(Key::ArrowLeft) { 
            view_state.rotation[0] += ROTATION_CHANGE_DELTA;
            view_state.rotation[1] += ROTATION_CHANGE_DELTA;
            uniform_written = true;
        }
        if keys.held(Key::ArrowRight) { 
            view_state.rotation[0] -= ROTATION_CHANGE_DELTA;
            view_state.rotation[1] -= ROTATION_CHANGE_DELTA;
            uniform_written = true;
        }

        if uniform_written {
            view_state_uniform.update(&ctx, &view_state)
        }

        ctx.run_render_pass(encoder, output, wgpu::Color::BLACK, &[&render_points]);

        if keys.just_pressed(Key::KeyQ) { 
            Some(WindowTask::Exit) 
        } else {
            None
        }
    })
}
