use ezcompute::*;

fn main() {
    let ctx = Ctx::new();

    let vertices: &[[f32; 2]] = &[[-0.5, -0.5], [0.5, -0.5], [0.0, 0.5]];
    let vertex_buffer = ctx.create_vertex_buffer(VertexBufferDescriptor {
        vertices,
        attributes: &wgpu::vertex_attr_array!(0 => Float32x2),
        index_buffer: None,
        primitives: wgpu::PrimitiveTopology::TriangleList,
    });

    let render_tri = ctx.create_render_pipeline(RenderPipelineDescriptor {
        inputs: &[],
        vertex_buffer: &vertex_buffer,
        shader: ShaderSource::Str(include_str!("shader.wgsl")),
        shader_vertex_entry: "vertex",
        shader_fragment_entry: "fragment",
        output_format: OUTPUT_TEXTURE_FORMAT,
    });

    ctx.run((512, 512), 30, |encoder, output, _time_delta, _input| {
        ctx.run_render_pass(encoder, output, wgpu::Color::BLACK, &[&render_tri]);
        None
    });
}
