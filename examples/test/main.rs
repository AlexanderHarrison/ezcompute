use ezcompute::*;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
struct Vertex {
    pub pos: [f32; 3],
    pub colour: [f32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
struct Input {
    pub time: f32,   
}

fn main() {
    let ctx = Ctx::new();
    let window = Window::new(&ctx, (600, 600));

    let vertex_buffer = ctx.create_vertex_buffer(
        VERTICES,
        &wgpu::vertex_attr_array!(0 => Float32x3, 1 => Float32x3)
    );

    let mut input = Input { time: 0.0 };
    let input_uniform = ctx.create_uniform(&input);

    let pipeline = ctx.create_render_pipeline(RenderPipelineDescriptor {
        inputs: &[PipelineInput::Uniform(&input_uniform)],
        vertex_buffer,
        shader_file: std::path::Path::new("examples/test/test.wgsl"),
        shader_vertex_entry: "vertex",
        shader_fragment_entry: "frag",
        output_format: Window::TEXTURE_FORMAT,
    }).unwrap();

    window.run(&ctx, 60.0, |event| {
        match event {
            WindowEvent::Update { delta, keys } => {
                input.time += delta;
                
                if keys.iter().any(|k|
                    *k == ("q", KeyState::JustPressed)
                        || *k == (winit::keyboard::NamedKey::Escape, KeyState::JustPressed)
                ) { 
                    return Some(WindowTask::Exit) 
                }

                Some(WindowTask::Redraw)
            },
            WindowEvent::Redraw { window, surface } => {
                input_uniform.update(&ctx, &input);

                let output = surface.get_current_texture().unwrap(); 
                let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

                let mut encoder = ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("main pass encoder"),
                });

                let desc = wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                            ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                };

                {
                    let mut pass = encoder.begin_render_pass(&desc);
                    ctx.run_render_pipeline(&mut pass, &pipeline);
                }

                ctx.queue.submit(std::iter::once(encoder.finish()));
                window.pre_present_notify();
                output.present();
                None
            }
        }
    })
}

const VERTICES: &'static [Vertex] = &[
    Vertex {
        pos: [-0.5, -0.5, 0.0],
        colour: [1.0, 1.0, 1.0],
    },
    Vertex {
        pos: [0.5, -0.5, 0.0],
        colour: [1.0, 0.0, 1.0],
    },
    Vertex {
        pos: [0.0, 0.5, 0.0],
        colour: [0.0, 1.0, 1.0],
    },
];
