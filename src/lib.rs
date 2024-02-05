use winit::{
    event_loop::{EventLoop, EventLoopBuilder},
    window::WindowBuilder,
};

use wgpu::util::DeviceExt;

struct Update;

#[derive(Debug)]
pub struct Window {
    surface: wgpu::Surface<'static>,
    handle: &'static winit::window::Window,
    surface_config: wgpu::SurfaceConfiguration,
    event_loop: EventLoop<Update>,
}

#[derive(Debug, Copy, Clone)]
pub enum WindowEvent<'a> { 
    Redraw {
        window: &'static winit::window::Window,
        surface: &'a wgpu::Surface<'static>,
    },
    Update {
        delta: f32,
        keys: &'a [KeyEvent],
    }, 
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WindowTask { Redraw, Exit, }

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum KeyState {
    JustPressed,
    Held,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: winit::keyboard::Key,
    pub state: KeyState,
    queued_release: bool,
}

impl<'a> std::cmp::PartialEq<(&'a str, KeyState)> for KeyEvent {
    fn eq<'b>(&'b self, other: &'b (&'a str, KeyState)) -> bool {
        let (key_rhs, state_rhs) = other;

        match self {
            KeyEvent { 
                key: winit::keyboard::Key::Character(key_lhs), 
                state: state_lhs,
                .. 
            } => state_lhs == state_rhs && key_lhs == key_rhs,
            KeyEvent { 
                key: winit::keyboard::Key::Named(key_lhs), 
                state: state_lhs,
                .. 
            } => state_lhs == state_rhs && key_lhs.to_text() == Some(key_rhs),
            _ => false,
        }
    }
}

impl<'a> std::cmp::PartialEq<(winit::keyboard::NamedKey, KeyState)> for KeyEvent {
    fn eq<'b>(&'b self, other: &'b (winit::keyboard::NamedKey, KeyState)) -> bool {
        let (key_rhs, state_rhs) = other;

        match self {
            KeyEvent { 
                key: winit::keyboard::Key::Named(key_lhs), 
                state: state_lhs,
                .. 
            } => state_lhs == state_rhs && key_lhs == key_rhs,
            _ => false,
        }
    }
}

impl Window {
    pub const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;

    pub fn new(ctx: &Ctx, size: (u32, u32)) -> Self {
        let event_loop = EventLoopBuilder::with_user_event().build().unwrap();

        let window = WindowBuilder::new()
            .with_min_inner_size(winit::dpi::PhysicalSize::new(size.0, size.1))
            .with_max_inner_size(winit::dpi::PhysicalSize::new(size.0, size.1))
            .build(&event_loop).unwrap();

        let window: &'static _ = ctx.alloc.alloc(window);
        let surface = ctx.instance.create_surface(window).unwrap();
        let size = window.inner_size();

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: Self::TEXTURE_FORMAT,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            desired_maximum_frame_latency: 2,
            view_formats: vec![],
        };
        surface.configure(&ctx.device, &surface_config);

        Window { handle: window, surface, event_loop, surface_config }
    }

    pub fn run<F: FnMut(WindowEvent) -> Option<WindowTask>>(self, ctx: &Ctx, updates_per_second: f64, mut f: F) {
        let Self { handle, mut surface_config, surface, event_loop } = self;
        
        let update_period = std::time::Duration::from_secs_f64(updates_per_second.recip());
        let event_sender = event_loop.create_proxy();
        std::thread::spawn(move || {
            loop {
                let _ = event_sender.send_event(Update);
                std::thread::sleep(update_period);
            }
        });

        let mut keys: Vec<KeyEvent> = Vec::with_capacity(16);

        let mut prev_update_instant = None;
        event_loop.run(move |event, window_target| match event {
            winit::event::Event::UserEvent(Update) => {
                let now = std::time::Instant::now();
                let delta = match prev_update_instant {
                    Some(i) => {
                        let diff: std::time::Duration = now - i;
                        diff.as_secs_f32()
                    }
                    None => update_period.as_secs_f32(),
                };
                prev_update_instant = Some(now);

                match (f)(WindowEvent::Update { delta, keys: keys.as_slice() }) {
                    Some(WindowTask::Redraw) => handle.request_redraw(),
                    Some(WindowTask::Exit) => window_target.exit(),
                    None => (),
                }

                keys.retain_mut(|k| {
                    k.state = KeyState::Held;
                    !k.queued_release
                });
            },
            winit::event::Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == handle.id() => match event {
                winit::event::WindowEvent::RedrawRequested => {
                    match (f)(WindowEvent::Redraw { window: handle, surface: &surface }) {
                        Some(WindowTask::Redraw) => handle.request_redraw(),
                        Some(WindowTask::Exit) => window_target.exit(),
                        None => (),
                    }
                },
                winit::event::WindowEvent::Resized(new_size) => {
                    surface_config.width = new_size.width;
                    surface_config.height = new_size.height;
                    surface.configure(&ctx.device, &surface_config);
                },
                winit::event::WindowEvent::KeyboardInput {
                    event: winit::event::KeyEvent { state: winit::event::ElementState::Pressed, logical_key, .. },
                    ..
                } => {
                    // occurs during repeat presses
                    if keys.iter().any(|k| k.key == *logical_key) { return };

                    keys.push(KeyEvent { key: logical_key.clone(), state: KeyState::JustPressed, queued_release: false })
                },
                winit::event::WindowEvent::KeyboardInput {
                    event: winit::event::KeyEvent { state: winit::event::ElementState::Released, logical_key, .. },
                    ..
                } => {
                    keys.retain_mut(|k| {
                        if k.key != *logical_key {
                            true
                        } else {
                            match k.state {
                                KeyState::Held => false,
                                KeyState::JustPressed => {
                                    k.queued_release = true;
                                    true
                                }
                            }
                        }
                    });
                }
                winit::event::WindowEvent::CloseRequested => window_target.exit(),
                _ => (),
            },
            _ => (),
        }).expect("error running event loop")
    }
}

#[derive(Debug)]
pub struct Ctx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub instance: wgpu::Instance,

    pub copy_pipeline: wgpu::RenderPipeline,
    pub copy_bind_group_layout: wgpu::BindGroupLayout,
    pub copy_sampler: wgpu::Sampler,

    pub alloc: &'static bumpalo::Bump,
}

impl Ctx {
    pub fn new() -> Self {
        pollster::block_on(Self::new_async())
    }

    pub async fn new_async() -> Self {
        env_logger::init();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            dx12_shader_compiler: Default::default(),
            gles_minor_version: Default::default(),
            flags: Default::default(),
        });
        let adapter = instance.request_adapter(&Default::default()).await.unwrap();
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::CLEAR_TEXTURE
                    | wgpu::Features:: TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
                required_limits: Default::default(),
            },
            None
        ).await.unwrap();

        let copy_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(include_str!("copy.wgsl").into()),
        });

        let copy_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&copy_bind_group_layout],
            push_constant_ranges: &[],
        });

        let copy_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &copy_shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &copy_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: Window::TEXTURE_FORMAT,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let copy_sampler = device.create_sampler(&Default::default());

        Self {
            device, queue, instance,
            copy_pipeline, copy_bind_group_layout, copy_sampler,
            alloc: Box::leak(Box::new(bumpalo::Bump::new())),
        }
    }

    pub fn record<F>(
        &self, 
        output: &'static str, 
        size: (u32, u32), 
        frame_count: u32, 
        frame_rate: usize,
        mut f: F
    ) where
        F: FnMut(&Texture)
    {
        let texture = self.create_texture(size, wgpu::TextureFormat::Rgba8Unorm);
        let size = (size.0 as usize, size.1 as usize);

        let (sender, receiver) = std::sync::mpsc::channel::<Vec<u8>>();

        let write_thread = std::thread::spawn(move || {
            let mut y_buf = vec![0u8; size.0 * size.1];
            let mut u_buf = vec![0u8; size.0 * size.1];
            let mut v_buf = vec![0u8; size.0 * size.1];

            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(std::path::Path::new(output)).expect("could not open or create file");

            let mut encoder = y4m::encode(size.0, size.1, y4m::Ratio::new(frame_rate, 1))
                .with_colorspace(y4m::Colorspace::C444)
                .write_header(file)
                .unwrap();

            let mut frame_num = 0;

            let mut frame_num = 0;
            loop {
                let rgba_buf = match receiver.recv() {
                    Ok(buf) => buf,
                    Err(_) => break
                };

                convert_rgba_to_yuv444p(&rgba_buf, size.0, size.1, &mut y_buf, &mut u_buf, &mut v_buf);

                let frame = y4m::Frame::new([&y_buf, &u_buf, &v_buf], None);
                if frame_num == frame_count { break }
                frame_num += 1;
                print!("encoding frame {}/{}\r", frame_num, frame_count);
                encoder.write_frame(&frame).unwrap();
            }
        });

        for _ in 0..frame_count {
            (f)(&texture);
            texture.read(self, sender.clone());
        }
        std::mem::drop(sender);

        write_thread.join().unwrap();
    }

    pub fn create_vertex_buffer<T: bytemuck::NoUninit>(
        &self, 
        vertices: &[T],
        attributes: &[wgpu::VertexAttribute],
    ) -> VertexBuffer {
        let attributes: &'static [wgpu::VertexAttribute] = self.alloc.alloc_slice_copy(attributes);

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<T>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes,
        };

        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let vertex_count = vertices.len() as u32;

        VertexBuffer { buffer, vertex_layout, vertex_count }
    }

    pub fn create_storage_buffer<T: bytemuck::NoUninit>(&self, data: &[T]) -> StorageBuffer {
        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        });

        let layout = std::alloc::Layout::new::<T>();

        StorageBuffer { buffer, layout }
    }

    pub fn create_uniform<T: bytemuck::NoUninit>(&self, data: &T) -> Uniform {
        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(std::slice::from_ref(data)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = std::alloc::Layout::new::<T>();
        Uniform { buffer, layout, }
    }

    /// texture format for storage texture must be rgba8unorm
    pub fn create_storage_texture(&self, size: (u32, u32), format: wgpu::TextureFormat) -> Texture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::STORAGE_BINDING 
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        Texture { texture, view }
    }

    pub fn create_texture(&self, size: (u32, u32), format: wgpu::TextureFormat) -> Texture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        Texture { texture, view }
    }

    pub fn create_render_pipeline(
        &self, 
        desc: RenderPipelineDescriptor<'_>
    ) -> Result<RenderPipeline, PipelineCreationError> {
        let vertex_count = desc.vertex_buffer.vertex_count;
        self.create_render_pipeline_ex(RenderPipelineDescriptorEx {
            inputs: desc.inputs,
            vertex_buffer: Some(desc.vertex_buffer),
            shader_file: desc.shader_file,
            shader_vertex_entry: desc.shader_vertex_entry,
            shader_fragment_entry: desc.shader_fragment_entry,
            output_format: desc.output_format,
            primitive: wgpu::PrimitiveTopology::TriangleList,
            vertex_count,
            blend_state: None,
            instance_count: 1,
        })
    }

    pub fn create_render_pipeline_ex(
        &self, 
        render_pipeline_desc: RenderPipelineDescriptorEx<'_>
    ) -> Result<RenderPipeline, PipelineCreationError> {
        let bind_group_layout_entries: &'static [wgpu::BindGroupLayoutEntry] = self.alloc.alloc_slice_fill_iter({
            render_pipeline_desc.inputs.iter()
                .enumerate()
                .map(|(i, input): (usize, &PipelineInput)| {
                    wgpu::BindGroupLayoutEntry {
                        binding: i as u32,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        count: None,
                        ty: input.binding_type(),
                    }
                })
        });

        let bind_group_layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: bind_group_layout_entries,
        });

        let bind_group_entries: &[wgpu::BindGroupEntry] = self.alloc.alloc_slice_fill_iter({
            render_pipeline_desc.inputs.iter()
                .enumerate()
                .map(|(i, input): (usize, &PipelineInput)| {
                    wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: input.binding_resource(),
                    }
                })
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: bind_group_entries,
        });

        let shader_source = std::fs::read_to_string(render_pipeline_desc.shader_file)
            .map_err(|_| PipelineCreationError::ShaderFileDoesNotExist)?;
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Owned(shader_source)),
        });

        let vb_layout = render_pipeline_desc.vertex_buffer.as_ref()
            .map(|vbo| vbo.vertex_layout.clone());

        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            })),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: render_pipeline_desc.shader_vertex_entry,
                buffers: vb_layout.as_slice(),
            },
            primitive: wgpu::PrimitiveState {
                topology: render_pipeline_desc.primitive,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: render_pipeline_desc.shader_fragment_entry,
                targets: &[Some(wgpu::ColorTargetState {
                    format: render_pipeline_desc.output_format,
                    blend: render_pipeline_desc.blend_state,
                    write_mask: wgpu::ColorWrites::ALL,
                })]
            }),
            multiview: None,
        });

        let vertex_count = render_pipeline_desc.vertex_count;
        let instance_count = render_pipeline_desc.instance_count;

        Ok(RenderPipeline {
            wgpu_pipeline: pipeline,
            shader,
            bind_group: bind_group,
            bind_group_layout: bind_group_layout,
            vertex_buffer: render_pipeline_desc.vertex_buffer,
            instance_count,
            vertex_count,
        })
    }

    pub fn create_compute_pipeline(
        &self, 
        compute_pipeline_desc: ComputePipelineDescriptor<'_>
    ) -> Result<ComputePipeline, PipelineCreationError> {
        let input_count = compute_pipeline_desc.inputs.len();
        let bind_group_layout_entries: &'static [wgpu::BindGroupLayoutEntry] = self.alloc.alloc_slice_fill_iter(
            CustomChain::new(
                compute_pipeline_desc.inputs.iter()
                    .enumerate()
                    .map(|(i, input): (usize, &PipelineInput)|
                        wgpu::BindGroupLayoutEntry {
                            binding: i as u32,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            count: None,
                            ty: input.binding_type(),
                        }
                    ),
                compute_pipeline_desc.outputs.iter()
                    .enumerate()
                    .map(|(i, output): (usize, &ComputePipelineOutput)|
                        wgpu::BindGroupLayoutEntry {
                            binding: (i + input_count) as u32,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            count: None,
                            ty: output.binding_type(),
                        }
                    )
            )
        );

        let bind_group_layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: bind_group_layout_entries,
        });

        let bind_group_entries: &[wgpu::BindGroupEntry] = self.alloc.alloc_slice_fill_iter(
            CustomChain::new(
                compute_pipeline_desc.inputs.iter()
                    .enumerate()
                    .map(|(i, input): (usize, &PipelineInput)| 
                        wgpu::BindGroupEntry {
                            binding: i as u32,
                            resource: input.binding_resource(),
                        }
                    ),
                compute_pipeline_desc.outputs.iter()
                    .enumerate()
                    .map(|(i, output): (usize, &ComputePipelineOutput)|
                        wgpu::BindGroupEntry {
                            binding: (i + input_count) as u32,
                            resource: output.binding_resource(), 
                        }
                    )
            )
        );

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: bind_group_entries,
        });

        let shader_source = std::fs::read_to_string(compute_pipeline_desc.shader_file)
            .map_err(|_| PipelineCreationError::ShaderFileDoesNotExist)?;

        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Owned(shader_source)),
        });

        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(&self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            })),
            module: &shader,
            entry_point: compute_pipeline_desc.shader_entry,
        });

        Ok(ComputePipeline {
            wgpu_pipeline: pipeline,
            shader,
            bind_group,
            bind_group_layout,
            dispatch_count: compute_pipeline_desc.dispatch_count,
        })
    }

    pub fn start_render_pass<'a>(
        &self, 
        encoder: &'a mut wgpu::CommandEncoder,
        clear_colour: Option<wgpu::Color>,
        output_view: &'a wgpu::TextureView,
    ) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: match clear_colour {
                        Some(c) => wgpu::LoadOp::Clear(c),
                        None => wgpu::LoadOp::Load,
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        })
    }

    pub fn run_render_pipeline<'a>(&self, pass: &mut wgpu::RenderPass<'a>, pipeline: &'a RenderPipeline) {
        pass.set_pipeline(&pipeline.wgpu_pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        if let Some(ref vbo) = pipeline.vertex_buffer {
            pass.set_vertex_buffer(0, vbo.buffer.slice(..));
        }
        pass.draw(0..pipeline.vertex_count, 0..pipeline.instance_count);
    }

    pub fn run_compute_pipeline<'a>(&self, pass: &mut wgpu::ComputePass<'a>, pipeline: &'a ComputePipeline) {
        pass.set_pipeline(&pipeline.wgpu_pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        let [x, y, z] = pipeline.dispatch_count;
        pass.dispatch_workgroups(x, y, z);
    }

    pub fn create_copy_source_bind_group(&self, source: &Texture) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.copy_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.copy_sampler),
                },
            ],
        })
    }

    pub fn clear_texture(
        &self, 
        encoder: &mut wgpu::CommandEncoder,
        texture: &Texture,
    ) {
        encoder.clear_texture(&texture.texture, &wgpu::ImageSubresourceRange::default());
    }

    pub fn copy_buffer_to_buffer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        source: &StorageBuffer,
        dest: &StorageBuffer
    ) {
        assert!(source.layout == dest.layout);
        encoder.copy_buffer_to_buffer(&source.buffer, 0, &dest.buffer, 0, source.buffer.size());
    }

    pub fn copy_texture_to_screen(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        copy_source_bind_group: &wgpu::BindGroup,
        surface: &wgpu::SurfaceTexture,
        background_colour: wgpu::Color,
    ) {
        let surface_view = surface.texture.create_view(&Default::default());
        self.copy_texture_to_texture(encoder, copy_source_bind_group, &surface_view, background_colour);
    }

    pub fn copy_texture_to_texture(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        copy_source_bind_group: &wgpu::BindGroup,
        target_texture_view: &wgpu::TextureView,
        background_colour: wgpu::Color,
    ) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(background_colour),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        rpass.set_pipeline(&self.copy_pipeline);
        rpass.set_bind_group(0, &copy_source_bind_group, &[]);
        rpass.draw(0..3, 0..2);
    }
}

#[derive(Debug)]
pub struct ComputePipelineDescriptor<'a> {
    pub inputs: &'a [PipelineInput<'a>],
    pub outputs: &'a [ComputePipelineOutput<'a>],
    pub shader_file: &'a std::path::Path,
    pub shader_entry: &'static str,
    pub dispatch_count: [u32; 3],
}


#[derive(Debug)]
pub struct ComputePipeline {
    pub wgpu_pipeline: wgpu::ComputePipeline,
    pub bind_group: wgpu::BindGroup,
    pub shader: wgpu::ShaderModule,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub dispatch_count: [u32; 3],
}

#[derive(Debug)]
pub struct RenderPipelineDescriptor<'a> {
    pub inputs: &'a [PipelineInput<'a>],
    pub vertex_buffer: VertexBuffer,
    pub shader_file: &'a std::path::Path,
    pub shader_vertex_entry: &'static str,
    pub shader_fragment_entry: &'static str,
    pub output_format: wgpu::TextureFormat,
}

#[derive(Debug)]
pub struct RenderPipelineDescriptorEx<'a> {
    pub inputs: &'a [PipelineInput<'a>],
    pub vertex_buffer: Option<VertexBuffer>,
    pub shader_file: &'a std::path::Path,
    pub shader_vertex_entry: &'static str,
    pub shader_fragment_entry: &'static str,
    pub output_format: wgpu::TextureFormat,
    pub primitive: wgpu::PrimitiveTopology,
    pub blend_state: Option<wgpu::BlendState>,
    pub instance_count: u32,
    pub vertex_count: u32,
}

#[derive(Debug)]
pub struct RenderPipeline {
    pub wgpu_pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub vertex_buffer: Option<VertexBuffer>,
    pub shader: wgpu::ShaderModule,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub instance_count: u32,
    pub vertex_count: u32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PipelineCreationError {
    ShaderFileDoesNotExist,
}

#[derive(Copy, Clone, Debug)]
pub enum PipelineInput<'a> {
    Uniform(&'a Uniform),
    StorageBuffer(&'a StorageBuffer),
    Texture(&'a Texture),
    StorageTexture(&'a Texture),
}

impl<'a> PipelineInput<'a> {
    pub fn binding_resource(self) -> wgpu::BindingResource<'a> {
        match self {
            PipelineInput::Uniform(uniform) => wgpu::BindingResource::Buffer(
                wgpu::BufferBinding { buffer: &uniform.buffer, offset: 0, size: None }
            ),
            PipelineInput::StorageBuffer(ssbo) => wgpu::BindingResource::Buffer(
                wgpu::BufferBinding { buffer: &ssbo.buffer, offset: 0, size: None }
            ),
            PipelineInput::Texture(texture) => wgpu::BindingResource::TextureView(
                &texture.view
            ),
            PipelineInput::StorageTexture(texture) => wgpu::BindingResource::TextureView(
                &texture.view
            ),
        }
    }

    pub fn binding_type(self) -> wgpu::BindingType {
        match self {
            PipelineInput::Uniform(_) => wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            PipelineInput::StorageBuffer(_) => wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            PipelineInput::Texture(texture) => wgpu::BindingType::Texture {
                sample_type: texture.texture.format().sample_type(None, None).expect("incompatible texture sample type"),
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            PipelineInput::StorageTexture(texture) => wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::ReadOnly,
                format: texture.texture.format(),
                view_dimension: wgpu::TextureViewDimension::D2,
            },
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ComputePipelineOutput<'a> {
    StorageBuffer(&'a StorageBuffer),
    StorageTexture(&'a Texture),
}

impl<'a> ComputePipelineOutput<'a> {
    pub fn binding_resource(self) -> wgpu::BindingResource<'a> {
        match self {
            ComputePipelineOutput::StorageBuffer(ssbo) => wgpu::BindingResource::Buffer(
                wgpu::BufferBinding { buffer: &ssbo.buffer, offset: 0, size: None }
            ),
            ComputePipelineOutput::StorageTexture(texture) => wgpu::BindingResource::TextureView(
                &texture.view
            ),
        }
    }

    pub fn binding_type(self) -> wgpu::BindingType {
        match self {
            ComputePipelineOutput::StorageBuffer(_) => wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            ComputePipelineOutput::StorageTexture(texture) => wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: texture.texture.format(),
                view_dimension: wgpu::TextureViewDimension::D2,
            },
        }
    }
}

#[derive(Debug)]
pub struct Uniform {
    pub buffer: wgpu::Buffer,
    pub layout: std::alloc::Layout,
}

#[derive(Debug)]
pub struct StorageBuffer {
    pub buffer: wgpu::Buffer,
    pub layout: std::alloc::Layout,
}

#[derive(Debug)]
pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl Uniform {
    pub fn update<T: bytemuck::NoUninit>(&self, ctx: &Ctx, data: &T) {
        assert!(std::alloc::Layout::new::<T>() == self.layout);
        ctx.queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(std::slice::from_ref(data)));
    }
}

impl StorageBuffer {
    pub fn update<T: bytemuck::NoUninit>(&self, ctx: &Ctx, data: &[T]) {
        assert!(std::alloc::Layout::new::<T>() == self.layout);
        ctx.queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(data));
    }
}

#[derive(Debug)]
pub struct VertexBuffer {
    pub buffer: wgpu::Buffer,
    pub vertex_layout: wgpu::VertexBufferLayout<'static>,
    pub vertex_count: u32,
}

impl Texture {
    pub fn read_to_png(&self, ctx: &Ctx, file: &std::path::Path) {
        let buf = self.read_to_vec(ctx);
        let width = self.texture.width();
        let height = self.texture.height();

        lodepng::encode_file(
            file,
            &buf,
            width as usize,
            height as usize,
            lodepng::ColorType::RGBA,
            8
        ).unwrap();
    }

    pub fn read_to_vec(&self, ctx: &Ctx) -> Vec<u8> {
        let (sender, receiver) = std::sync::mpsc::channel::<Vec<u8>>();
        self.read(ctx, sender);
        match receiver.recv() {
            Ok(buf) => buf,
            Err(e) => panic!("reading data buffer failed: {}", e),
        }
    }

    /// texture format must be Rgba8Unorm or Bgra8Unorm.
    pub fn read(&self, ctx: &Ctx, sender: std::sync::mpsc::Sender<Vec<u8>>) {
        let width = self.texture.width();
        let height = self.texture.height();

        let format = self.texture.format();
        assert!(format == wgpu::TextureFormat::Bgra8Unorm || format == wgpu::TextureFormat::Rgba8Unorm);

        let bytes_per_row_packed = width * 4;
        let bytes_per_row_texture = if bytes_per_row_packed & 255 != 0 {
            (bytes_per_row_packed & (!255)) + 256
        } else {
            bytes_per_row_packed
        };

        let buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes_per_row_texture as u64 * height as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::ImageCopyBuffer {
                buffer: &buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row_texture),
                    rows_per_image: None,
                }
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1
            }
        );
        
        ctx.queue.submit(std::iter::once(encoder.finish()));

        let bytes_per_row_texture = bytes_per_row_texture as usize;
        let bytes_per_row_packed = bytes_per_row_packed as usize;
        let height = height as usize;

        let arc_buffer = std::sync::Arc::new(buffer);
        let callback_buffer = arc_buffer.clone();

        arc_buffer.slice(..)
            .map_async(
                wgpu::MapMode::Read,
                move |res| {
                    match res {
                        Ok(_) => (),
                        Err(_) => {
                            eprintln!("texture read failed");
                            return;
                        }
                    };

                    let texture = callback_buffer.slice(..).get_mapped_range();

                    let mut buffer = vec![0; bytes_per_row_packed * height];

                    for y in 0..height {
                        let dst = &mut buffer[y*bytes_per_row_packed..][..bytes_per_row_packed];
                        let src = &texture[y*bytes_per_row_texture..][..bytes_per_row_packed];
                        dst.copy_from_slice(src);
                    }

                    match sender.send(buffer) {
                        Ok(_) => (),
                        Err(e) => eprintln!("texture data send failed: {}", e),
                    }
                }
            );

        ctx.device.poll(wgpu::Maintain::Wait);
    }

}

/// std Chain doesn't impl ExactSizeIterator >:(
struct CustomChain<A, B> {
    a: Option<A>,
    b: Option<B>,
}

impl<A, B> CustomChain<A, B> {
    pub fn new(a: A, b: B) -> CustomChain<A, B> {
        CustomChain { a: Some(a), b: Some(b) }
    }
}

impl<A, B> Iterator for CustomChain<A, B>
where
    A: Iterator + ExactSizeIterator,
    B: Iterator<Item = A::Item> + ExactSizeIterator,
{
    type Item = A::Item;

    fn next(&mut self) -> Option<A::Item> {
        let ret_a = match self.a {
            Some(ref mut a) => {
                let ret = a.next();
                if matches!(ret, None) {
                    self.a = None;
                }
                ret
            },
            None => None,
        };

        match ret_a {
            Some(r) => Some(r),
            None => {
                self.b.as_mut()?.next()
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len_a = self.a.as_ref().map(|a| a.len()).unwrap_or(0);
        let len_b = self.b.as_ref().map(|b| b.len()).unwrap_or(0);
        let len = len_a + len_b;
        (len, Some(len))
    }
}

impl<A, B> ExactSizeIterator for CustomChain<A, B>
where
    A: Iterator + ExactSizeIterator,
    B: Iterator<Item = A::Item> + ExactSizeIterator,
{
    fn len(&self) -> usize {
        self.size_hint().0
    }
}

/// adopted from https://github.com/marcellBan/rgb2yuv420-rs
fn convert_rgba_to_yuv444p(
    img: &[u8],
    width: usize,
    height: usize,
    y_buffer: &mut [u8],
    u_buffer: &mut [u8],
    v_buffer: &mut [u8],
) {
    let frame_size = width * height;

    assert!(y_buffer.len() >= frame_size);
    assert!(u_buffer.len() >= frame_size);
    assert!(v_buffer.len() >= frame_size);

    let chroma_size = frame_size / 4;
    let mut yuv_index = 0;
    let mut index = 0;
    for j in 0..height {
        for _ in 0..width {
            use std::ops::Mul;
            let r = f32::from(img[index + 0]).mul(1.5).min(255.0);
            let g = f32::from(img[index + 1]).mul(1.5).min(255.0);
            let b = f32::from(img[index + 2]).mul(1.5).min(255.0);

            let y = ( 0.257 * r + 0.504 * g + 0.098 * b +  16.0) as u8;
            let u = (-0.148 * r - 0.291 * g + 0.439 * b + 128.0) as u8;
            let v = ( 0.439 * r - 0.368 * g - 0.071 * b + 128.0) as u8;

            index += 4;

            y_buffer[yuv_index] = y;
            u_buffer[yuv_index] = u;
            v_buffer[yuv_index] = v;
            yuv_index += 1;
        }
    }
}

///// adopted from https://github.com/marcellBan/rgb2yuv420-rs
//fn convert_rgb_to_yuv420p(
//    img: &[u8],
//    width: u32,
//    height: u32,
//    bytes_per_pixel: usize,
//    yuv_buffer: &mut [u8]
//) {
//    assert!(yuv_buffer.len() >= (width * height * 3 / 2) as usize);
//
//    let frame_size = (width * height) as usize;
//    let chroma_size = frame_size / 4;
//    let mut y_index = 0;
//    let mut uv_index = frame_size;
//    let mut index = 0;
//    for j in 0..height {
//        for _ in 0..width {
//            let r = i32::from(img[index + 0]);
//            let g = i32::from(img[index + 1]);
//            let b = i32::from(img[index + 2]);
//            index += bytes_per_pixel;
//            yuv_buffer[y_index] = clamp((77 * r + 150 * g + 29 * b + 128) >> 8);
//            y_index += 1;
//            if j % 2 == 0 && index % 2 == 0 {
//                let u = clamp(((-43 * r - 84 * g + 127 * b + 128) >> 8) + 128);
//                let v = clamp(((127 * r - 106 * g - 21 * b + 128) >> 8) + 128);
//                yuv_buffer[uv_index] = u;
//                yuv_buffer[uv_index + chroma_size] = v;
//                uv_index += 1;
//            }
//        }
//    }
//}
//
//fn clamp(val: i32) -> u8 {
//    if val < 0 {
//        0
//    } else if val > 255 {
//        255
//    } else {
//        val as u8
//    }
//}
//
