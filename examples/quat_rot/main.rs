use ezcompute::*;
use glam::f32::Mat4;

fn main() {
    let ctx = Ctx::new();

    let vertex_buffer = ctx.create_vertex_buffer(VertexBufferDescriptor::CUBE);
    let rot = ctx.create_uniform(&Mat4::IDENTITY);
    let render_tri = ctx.create_render_pipeline_ex(RenderPipelineDescriptorEx {
        instance_range: 0..64,
        ..RenderPipelineDescriptor {
            inputs: &[PipelineInput::Uniform(&rot)],
            vertex_buffer: &vertex_buffer,
            shader: ShaderSource::Str(include_str!("shader.wgsl")),
            //shader: ShaderSource::File(std::path::Path::new("examples/quat_rot/shader.wgsl")),
            shader_vertex_entry: "vertex",
            shader_fragment_entry: "fragment",
            output_format: OUTPUT_TEXTURE_FORMAT,
        }.into()
    });

    let mut quat: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

    fn norm(v: &mut [f32; 4]) {
        let len = (v[0].powi(2) + v[1].powi(2) + v[2].powi(2) + v[3].powi(2)).sqrt().recip();
        v[0] *= len; v[1] *= len; v[2] *= len; v[3] *= len;
    }

    ctx.run((512, 512), 60, |encoder, output, _time_delta, input| {
        if input.held(Key::KeyQ) { quat[0] += 0.01; }
        if input.held(Key::KeyW) { quat[1] += 0.01; }
        if input.held(Key::KeyE) { quat[2] += 0.01; }
        if input.held(Key::KeyR) { quat[3] += 0.01; }
        if input.held(Key::KeyA) { quat[0] -= 0.01; }
        if input.held(Key::KeyS) { quat[1] -= 0.01; }
        if input.held(Key::KeyD) { quat[2] -= 0.01; }
        if input.held(Key::KeyF) { quat[3] -= 0.01; }
        norm(&mut quat);

        let perspective_mat = Mat4::perspective_lh(
            1.0, 
            1.0,
            0.1,
            32.0*10.0
        );

        let mat = perspective_mat * Mat4::from_quat(glam::f32::Quat::from_array(quat));
        rot.update(&ctx, &mat);

        ctx.run_render_pass(encoder, output, wgpu::Color::BLACK, &[&render_tri]);

        if input.just_pressed(Key::Escape) { Some(WindowTask::Exit) } else { None }
    });
}
