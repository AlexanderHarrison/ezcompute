use ezcompute::*;

#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
#[repr(C)]
struct Vertex {
    pub pos: [f32; 3],
    pub colour: [f32; 3],
}

fn main() {
    let ctx = Ctx::new();

    let vertices: &'static [Vertex] = &[
        Vertex { pos: [-0.5,  0.5, -0.5], colour: [0.6, 0.0, 0.0] },
        Vertex { pos: [ 0.5,  0.5, -0.5], colour: [0.0, 0.8, 0.0] },
        Vertex { pos: [-0.5, -0.5, -0.5], colour: [0.0, 0.0, 0.8] },
        Vertex { pos: [ 0.5, -0.5, -0.5], colour: [0.0, 0.8, 0.8] },
        Vertex { pos: [-0.5,  0.5,  0.5], colour: [0.8, 0.8, 0.0] },
        Vertex { pos: [ 0.5,  0.5,  0.5], colour: [0.8, 0.0, 0.8] },
        Vertex { pos: [-0.5, -0.5,  0.5], colour: [0.3, 0.3, 0.6] },
        Vertex { pos: [ 0.5, -0.5,  0.5], colour: [0.3, 0.6, 0.3] },
    ];

    let indices: &'static [[u32; 3]] = &[
        [0, 1, 2],
        [2, 1, 3],
        [4, 0, 6],
        [6, 0, 2],
        [7, 5, 6],
        [6, 5, 4],
        [3, 1, 7],
        [7, 1, 5],
        [4, 5, 0],
        [0, 5, 1],
        [3, 7, 2],
        [2, 7, 6],
    ];

    let cube = ctx.create_vertex_buffer(VertexBufferDescriptor {
        vertices,
        attributes: &wgpu::vertex_attr_array!(0 => Float32x3, 1 => Float32x3),
        index_buffer: Some(IndexBufferData::from_array_u32(indices)),
        primitives: wgpu::PrimitiveTopology::TriangleList,
    });

    let mut time: f32 = 0.0;
    let time_uniform = ctx.create_uniform(&time);
    let mut rotation: [f32; 2] = [0.0; 2];
    let rotation_uniform = ctx.create_uniform(&rotation);

    let render_cube = ctx.create_render_pipeline(RenderPipelineDescriptor {
        inputs: &[PipelineInput::Uniform(&time_uniform), PipelineInput::Uniform(&rotation_uniform)],
        vertex_buffer: &cube,
        shader_file: std::path::Path::new("examples/cube/shader.wgsl"),
        shader_vertex_entry: "vertex",
        shader_fragment_entry: "fragment",
        output_format: OUTPUT_TEXTURE_FORMAT,
    }).unwrap();

    ctx.run(
        (512, 512), 
        60, 
        |encoder, output, delta, input| {
            time += delta;
            time_uniform.update(&ctx, &time);
            if let Some(pos) = input.mouse_position {
                rotation[0] = pos.1 / 50.0;
                rotation[1] = pos.0 / 50.0;
                rotation_uniform.update(&ctx, &rotation);
            }

            ctx.run_render_pass(encoder, output, wgpu::Color::BLACK, &[&render_cube]);

            if input.just_pressed(Key::KeyQ) || input.just_pressed(Key::Escape) {
                Some(WindowTask::Exit)
            } else {
                None
            }
        },
    )
}
