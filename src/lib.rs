use winit::{
    event_loop::EventLoopBuilder,
    window::WindowBuilder,
};

use wgpu::util::DeviceExt;

pub const SURFACE_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;

#[derive(Debug)]
pub struct Ctx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub instance: wgpu::Instance,

    pub copy_pipeline_layout: wgpu::PipelineLayout,
    pub copy_bind_group_layout: wgpu::BindGroupLayout,
    pub copy_sampler: wgpu::Sampler,
    pub copy_shader: wgpu::ShaderModule,

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
        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
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

        let copy_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&copy_bind_group_layout],
            push_constant_ranges: &[],
        });

        let copy_sampler = device.create_sampler(&Default::default());

        Self {
            device, queue, instance,
            copy_pipeline_layout, copy_bind_group_layout, copy_sampler, copy_shader,
            alloc: Box::leak(Box::new(bumpalo::Bump::new())),
        }
    }

    pub fn run<F>(&self, size: (u32, u32), updates_per_second: u32, mut f: F) where
        F: FnMut(WindowEvent) -> Option<WindowTask>
    {
        let event_loop = EventLoopBuilder::with_user_event().build().unwrap();

        let window = WindowBuilder::new()
            .with_min_inner_size(winit::dpi::PhysicalSize::new(size.0, size.1))
            .with_max_inner_size(winit::dpi::PhysicalSize::new(size.0, size.1))
            .build(&event_loop).unwrap();

        let window: &'static _ = self.alloc.alloc(window);
        let surface = self.instance.create_surface(window).unwrap();
        let size = window.inner_size();

        let mut surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: SURFACE_TEXTURE_FORMAT,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            desired_maximum_frame_latency: 2,
            view_formats: vec![],
        };
        surface.configure(&self.device, &surface_config);

        let update_period = std::time::Duration::from_secs_f64((updates_per_second as f64 * 1.0).recip());
        let event_sender = event_loop.create_proxy();
        std::thread::spawn(move || {
            loop {
                let _ = event_sender.send_event(Update);
                std::thread::sleep(update_period);
            }
        });

        let mut keys: Vec<KeyEvent> = Vec::with_capacity(16);

        let mut prev_update_instant = None;

        let depth_texture_desc = wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: size.width, height: size.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };

        let depth_texture = self.device.create_texture(&depth_texture_desc);
        let depth_view = depth_texture.create_view(&Default::default());

        let null_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            ..depth_texture_desc
        });
        let null_view = null_texture.create_view(&Default::default());

        // messy, but it works
        let mut output = RenderTexture {
            texture: null_texture,
            view: null_view,
            depth_texture,
            depth_view,
        };

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

                let events = KeyEvents { events: keys.as_slice() };
                match (f)(WindowEvent::Update { delta, keys: events }) {
                    Some(WindowTask::Redraw) => window.request_redraw(),
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
            } if window_id == window.id() => match event {
                winit::event::WindowEvent::RedrawRequested => {
                    let mut surface_texture = match surface.get_current_texture() {
                        Ok(s) => s,
                        Err(wgpu::SurfaceError::Timeout) => return,
                        Err(e) => panic!("{}", e),
                    };

                    let surface_size = surface_texture.texture.size();
                    if surface_size != output.depth_texture.size() {
                        output.depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                            size: surface_size,
                            ..depth_texture_desc
                        });
                        output.depth_view = output.depth_texture.create_view(&Default::default())
                    }

                    take_mut::take(&mut surface_texture.texture, |texture| {
                        let view = texture.create_view(&Default::default());
                        let null_texture = std::mem::replace(&mut output.texture, texture);
                        let null_view = std::mem::replace(&mut output.view, view);
                        match (f)(WindowEvent::Redraw { output: &output }) {
                            Some(WindowTask::Redraw) => window.request_redraw(),
                            Some(WindowTask::Exit) => window_target.exit(),
                            None => (),
                        };

                        let texture = std::mem::replace(&mut output.texture, null_texture);
                        output.view = null_view;
                        texture
                    });
                    window.pre_present_notify();
                    surface_texture.present();
                },
                winit::event::WindowEvent::Resized(new_size) => {
                    surface_config.width = new_size.width;
                    surface_config.height = new_size.height;
                    surface.configure(&self.device, &surface_config);
                },
                winit::event::WindowEvent::KeyboardInput {
                    event: winit::event::KeyEvent { 
                        state: winit::event::ElementState::Pressed, 
                        physical_key: winit::keyboard::PhysicalKey::Code(physical_key), 
                        .. 
                    },
                    ..
                } => {
                    // occurs during repeat presses
                    if keys.iter().any(|k| k.key == *physical_key) { return };

                    keys.push(KeyEvent { key: *physical_key, state: KeyState::JustPressed, queued_release: false })
                },
                winit::event::WindowEvent::KeyboardInput {
                    event: winit::event::KeyEvent { 
                        state: winit::event::ElementState::Released, 
                        physical_key: winit::keyboard::PhysicalKey::Code(physical_key), 
                        .. 
                    },
                    ..
                } => {
                    keys.retain_mut(|k| {
                        if k.key != *physical_key {
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

    //pub fn record<F>(
    //    &self, 
    //    output: &'static str, 
    //    size: (u32, u32), 
    //    frame_count: u32, 
    //    frame_rate: u32,
    //    mut f: F
    //) where
    //    F: FnMut(&Texture)
    //{
    //    let texture = self.create_texture(size, wgpu::TextureFormat::Bgra8UnormSrgb);
    //    let size = (size.0 as usize, size.1 as usize);
    //    let size_i = (size.0 as i32, size.1 as i32);

    //    let (sender, receiver) = std::sync::mpsc::channel::<Vec<u8>>();

    //    let write_thread = std::thread::spawn(move || {
    //        let mut file = std::io::BufWriter::new(std::fs::OpenOptions::new()
    //            .create(true)
    //            .write(true)
    //            .open(std::path::Path::new(output))
    //            .expect("could not open or create file"));

    //        let mut encoder = x264::Setup::high()
    //            .timebase(1, 90_000)
    //            .fps(frame_rate, 1)
    //            .annexb(false)
    //            .build(x264::Colorspace::BGRA, size_i.0, size_i.1)
    //            .unwrap();

    //        let mut mp4_writer = mp4::Mp4Writer::write_start(file, &Mp4Config {
    //            major_brand: str::parse("mp42").unwrap(),
    //            minor_version: 0,
    //            compatible_brands: vec![],
    //            timescale: 90_000,
    //        });
    //        
    //        mp4_writer.add_track(&mp4::TrackConfig {
    //            track_type: mpv::TrackType::Video,
    //            timescale: 90_000,
    //            language: String::new(),
    //            media_conf: mp4::MediaConfig::AvcConfig(mp4::AvcConfig {
    //                width: size.0 as _,
    //                height: size.1 as _,

    //            })
    //        });

    //        use std::io::Write;
    //        file.write_all(encoder.headers().unwrap().entirety()).unwrap();

    //        let mut frame_num: i64 = 0;
    //        let frame_rate = frame_rate as i64;
    //        loop {
    //            let rgba_buf = match receiver.recv() {
    //                Ok(buf) => buf,
    //                Err(_) => break
    //            };

    //            // 90kHz resolution
    //            let timestamp = frame_num * 90_000 / frame_rate;
    //            //let timestamp = frame_num * 60;

    //            let image = x264::Image::bgra(size_i.0, size_i.1, &rgba_buf);
    //            let (data, _) = encoder.encode(timestamp, image).unwrap();
    //            file.write_all(data.entirety()).unwrap();
    //            frame_num += 1;
    //        }

    //        let mut flush = encoder.flush();
    //        while let Some(result) = flush.next() {
    //            let (data, _) = result.unwrap();
    //            file.write_all(data.entirety()).unwrap();
    //        }

    //        file.flush().unwrap();
    //    });

    //    for _ in 0..frame_count {
    //        (f)(&texture);
    //        texture.read(self, sender.clone());
    //    }
    //    std::mem::drop(sender);

    //    write_thread.join().unwrap();
    //}

    pub fn create_vertex_buffer<T: bytemuck::NoUninit>(
        &self, 
        desc: VertexBufferDescriptor<'_, T>,
    ) -> VertexBuffer {
        let attributes: &'static [wgpu::VertexAttribute] = self.alloc.alloc_slice_copy(desc.attributes);
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<T>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes,
        };

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(desc.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = match desc.index_buffer {
            Some(IndexBufferData::Uint16(data)) => {
                let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(data),
                    usage: wgpu::BufferUsages::INDEX,
                });
                Some(IndexBuffer {
                    format: wgpu::IndexFormat::Uint16,
                    buffer,
                    index_count: data.len() as u32,
                })
            },
            Some(IndexBufferData::Uint32(data)) => {
                let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(data),
                    usage: wgpu::BufferUsages::INDEX,
                });
                Some(IndexBuffer {
                    format: wgpu::IndexFormat::Uint32,
                    buffer,
                    index_count: data.len() as u32,
                })
            },
            None => None,
        };

        let vertex_count = desc.vertices.len() as u32;

        VertexBuffer { 
            vertex_buffer, 
            vertex_layout, 
            vertex_count,
            index_buffer,
            primitives: desc.primitives,
        }
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
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        Texture { texture, view }
    }

    /// texture format for storage texture must be rgba8unorm
    pub fn create_storage_texture_with_data<T: bytemuck::NoUninit>(
        &self,
        size: (u32, u32), 
        format: wgpu::TextureFormat,
        data: &[T]
    ) -> Texture {
        let texture = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::STORAGE_BINDING 
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(data),
        );
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

    pub fn create_render_texture(&self, size: (u32, u32), format: wgpu::TextureFormat) -> RenderTexture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&Default::default());

        RenderTexture { texture, view, depth_texture, depth_view }
    }

    pub fn create_texture_copier<'a>(&self, src: &'a Texture, dst: &'a Texture) -> TextureCopier<'a> {
        let source_format = src.texture.format();
        let target_format = dst.texture.format();

        let formats_match = source_format.remove_srgb_suffix() == target_format.remove_srgb_suffix();
        let dims_match = src.texture.dimension() == dst.texture.dimension();
        let size_match = src.texture.size() == dst.texture.size();
        
        if formats_match && dims_match && size_match {
            TextureCopier::Fast { src, dst }
        } else {
            let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&self.copy_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.copy_shader,
                    entry_point: "vs_main",
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.copy_shader,
                    entry_point: "fs_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        //blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(), // tri list
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
            });

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.copy_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.copy_sampler),
                    },
                ],
            });

            TextureCopier::Slow { pipeline, bind_group, dst }
        }
    }

    pub fn create_texture_copier_transparent<'a>(&self, src: &'a Texture, dst: &'a Texture) -> TextureCopier<'a> {
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&self.copy_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.copy_shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.copy_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: dst.texture.format(),
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(), // tri list
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.copy_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.copy_sampler),
                },
            ],
        });

        TextureCopier::Transparent { pipeline, bind_group, dst }
    }

    pub fn create_screen_copier<'a>(&self, src: &'a Texture) -> ScreenCopier {
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&self.copy_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.copy_shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.copy_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: SURFACE_TEXTURE_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(), // tri list
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.copy_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.copy_sampler),
                },
            ],
        });

        ScreenCopier { pipeline, bind_group }
    }

    pub fn create_render_pipeline<'a, 'b>(
        &self, 
        desc: RenderPipelineDescriptor<'a, 'b>
    ) -> Result<RenderPipeline<'a>, PipelineCreationError> {
        let draw_range = if let Some(ref ib) = desc.vertex_buffer.index_buffer {
            0..ib.index_count
        } else {
            0..desc.vertex_buffer.vertex_count
        };

        self.create_render_pipeline_ex(RenderPipelineDescriptorEx {
            inputs: desc.inputs,
            vertex_buffer: Either::A(desc.vertex_buffer),
            shader_file: desc.shader_file,
            shader_vertex_entry: desc.shader_vertex_entry,
            shader_fragment_entry: desc.shader_fragment_entry,
            output_format: desc.output_format,
            primitives: desc.vertex_buffer.primitives,
            draw_range,
            blend_state: None,
            instance_range: 0..1,
        })
    }

    pub fn create_render_pipeline_ex<'a, 'b>(
        &self, 
        desc: RenderPipelineDescriptorEx<'a, 'b>
    ) -> Result<RenderPipeline<'a>, PipelineCreationError> {
        let bind_group_layout_entries: &'static [wgpu::BindGroupLayoutEntry] = self.alloc.alloc_slice_fill_iter({
            desc.inputs.iter()
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
            desc.inputs.iter()
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

        let shader_source = std::fs::read_to_string(desc.shader_file)
            .map_err(|_| PipelineCreationError::ShaderFileDoesNotExist)?;
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Owned(shader_source)),
        });

        let (vertex_buffer, primitives, vb_layout) = match desc.vertex_buffer {
            Either::A(vbo) => (Some(vbo), vbo.primitives, Some(vbo.vertex_layout.clone())),
            Either::B(primitives) => (None, primitives, None),
        };

        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            })),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: desc.shader_vertex_entry,
                buffers: vb_layout.as_slice(),
            },
            primitive: wgpu::PrimitiveState {
                topology: primitives,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                //count: desc.multisample_count,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: desc.shader_fragment_entry,
                targets: &[Some(wgpu::ColorTargetState {
                    format: desc.output_format,
                    blend: desc.blend_state,
                    write_mask: wgpu::ColorWrites::ALL,
                })]
            }),
            multiview: None,
        });

        Ok(RenderPipeline {
            wgpu_pipeline: pipeline,
            shader,
            bind_group: bind_group,
            bind_group_layout: bind_group_layout,
            vertex_buffer,
            instance_range: desc.instance_range,
            draw_range: desc.draw_range,
        })
    }

    pub fn create_compute_pipeline(
        &self, 
        desc: ComputePipelineDescriptor<'_>
    ) -> Result<ComputePipeline, PipelineCreationError> {
        let input_count = desc.inputs.len();
        let bind_group_layout_entries: &'static [wgpu::BindGroupLayoutEntry] = self.alloc.alloc_slice_fill_iter(
            CustomChain::new(
                desc.inputs.iter()
                    .enumerate()
                    .map(|(i, input): (usize, &PipelineInput)|
                        wgpu::BindGroupLayoutEntry {
                            binding: i as u32,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            count: None,
                            ty: input.binding_type(),
                        }
                    ),
                desc.outputs.iter()
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
                desc.inputs.iter()
                    .enumerate()
                    .map(|(i, input): (usize, &PipelineInput)| 
                        wgpu::BindGroupEntry {
                            binding: i as u32,
                            resource: input.binding_resource(),
                        }
                    ),
                desc.outputs.iter()
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

        let shader_source = std::fs::read_to_string(desc.shader_file)
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
            entry_point: desc.shader_entry,
        });

        Ok(ComputePipeline {
            wgpu_pipeline: pipeline,
            shader,
            bind_group,
            bind_group_layout,
            dispatch_count: desc.dispatch_count,
        })
    }

    pub fn run_render_pipeline<'a>(&self, pass: &mut wgpu::RenderPass<'a>, pipeline: &'a RenderPipeline) {
        pass.set_pipeline(&pipeline.wgpu_pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        let draw_range = pipeline.draw_range.clone();
        let instance_range = pipeline.instance_range.clone();
        if let Some(v) = pipeline.vertex_buffer {
            pass.set_vertex_buffer(0, v.vertex_buffer.slice(..));

            if let Some(ref ib) = v.index_buffer {
                pass.set_index_buffer(ib.buffer.slice(..), ib.format);
                pass.draw_indexed(draw_range, 0, instance_range);
             } else {
                pass.draw(draw_range, instance_range);
             }
        } else {
            pass.draw(draw_range, instance_range);
        }
    }

    pub fn run_compute_pipeline<'a>(&self, pass: &mut wgpu::ComputePass<'a>, pipeline: &'a ComputePipeline) {
        pass.set_pipeline(&pipeline.wgpu_pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        let [x, y, z] = pipeline.dispatch_count;
        pass.dispatch_workgroups(x, y, z);
    }

    pub fn run_render_pass(
        &self, 
        encoder: &mut wgpu::CommandEncoder,
        output: &RenderTexture,
        clear_colour: wgpu::Color, 
        passes: &[&RenderPipeline]
    ) {
        let desc = wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output.view,
                resolve_target: None,
                    ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_colour),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &output.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        };
        let mut pass = encoder.begin_render_pass(&desc);
        for pipeline in passes {
            self.run_render_pipeline(&mut pass, pipeline);
        }
    }

    pub fn run_compute_pass(
        &self, 
        encoder: &mut wgpu::CommandEncoder,
        passes: &[&ComputePipeline]
    ) {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        for pipeline in passes {
            self.run_compute_pipeline(&mut pass, pipeline);
        }
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
        src: &StorageBuffer,
        dst: &StorageBuffer
    ) {
        assert!(src.layout == dst.layout);
        encoder.copy_buffer_to_buffer(&src.buffer, 0, &dst.buffer, 0, src.buffer.size());
    }

    pub fn copy_texture_to_screen(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        copier: &ScreenCopier,
        output: &Texture,
    ) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        rpass.set_pipeline(&copier.pipeline);
        rpass.set_bind_group(0, &copier.bind_group, &[]);
        rpass.draw(0..3, 0..2);
    }

    pub fn copy_texture_to_texture(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        copier: &TextureCopier,
    ) {
        match copier {
            TextureCopier::Fast { src, dst } => {
                encoder.copy_texture_to_texture(
                    src.texture.as_image_copy(),
                    dst.texture.as_image_copy(),
                    src.texture.size(),
                )
            },
            TextureCopier::Slow { ref pipeline, ref bind_group, dst } => {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &dst.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                rpass.set_pipeline(pipeline);
                rpass.set_bind_group(0, bind_group, &[]);
                rpass.draw(0..3, 0..2);
            },
            TextureCopier::Transparent { ref pipeline, ref bind_group, dst } => {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &dst.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                rpass.set_pipeline(pipeline);
                rpass.set_bind_group(0, bind_group, &[]);
                rpass.draw(0..3, 0..2);
            },
        }
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
pub struct RenderPipelineDescriptor<'a, 'b> {
    pub inputs: &'b [PipelineInput<'b>],
    pub vertex_buffer: &'a VertexBuffer,
    pub shader_file: &'b std::path::Path,
    pub shader_vertex_entry: &'static str,
    pub shader_fragment_entry: &'static str,
    pub output_format: wgpu::TextureFormat,
}

#[derive(Debug)]
pub enum Either<A, B> {
    A(A),
    B(B),
}

#[derive(Debug)]
pub struct RenderPipelineDescriptorEx<'a, 'b> {
    pub inputs: &'b [PipelineInput<'b>],
    pub vertex_buffer: Either<&'a VertexBuffer, wgpu::PrimitiveTopology>,
    pub shader_file: &'b std::path::Path,
    pub shader_vertex_entry: &'static str,
    pub shader_fragment_entry: &'static str,
    pub output_format: wgpu::TextureFormat,
    pub primitives: wgpu::PrimitiveTopology,
    pub blend_state: Option<wgpu::BlendState>,

    pub draw_range: std::ops::Range<u32>,
    pub instance_range: std::ops::Range<u32>,
}

#[derive(Debug)]
pub enum DepthBuffer {
    NotCreated,
    Created(Texture),
}

#[derive(Debug)]
pub struct RenderPipeline<'a> {
    pub wgpu_pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub vertex_buffer: Option<&'a VertexBuffer>,
    pub shader: wgpu::ShaderModule,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub draw_range: std::ops::Range<u32>,
    pub instance_range: std::ops::Range<u32>,
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
pub enum TextureCopier<'a> {
    Fast {
        src: &'a Texture,
        dst: &'a Texture,
    },
    Slow {
        pipeline: wgpu::RenderPipeline,
        bind_group: wgpu::BindGroup,
        dst: &'a Texture
    },
    Transparent {
        pipeline: wgpu::RenderPipeline,
        bind_group: wgpu::BindGroup,
        dst: &'a Texture,
    }
}

#[derive(Debug)]
pub struct ScreenCopier {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
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

#[derive(Debug)]
pub struct RenderTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
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

    /// Returns the number of elements in the buffer, not the size in bytes.
    pub fn len(&self) -> u32 {
        (self.buffer.size() / self.layout.pad_to_align().size() as u64) as u32
    }

    pub fn dispatch_count(&self, workgroup_size: u32) -> [u32; 3] {
        assert!(workgroup_size != 0);
        [(self.len() + workgroup_size-1)/workgroup_size, 1, 1]
    }
}

#[derive(Copy, Clone, Debug)]
pub enum IndexBufferData<'a> {
    Uint16(&'a [u16]),
    Uint32(&'a [u32]),
}

impl<'a> IndexBufferData<'a> {
    pub fn from_array_u16<const N: usize>(array: &'a [[u16; N]]) -> Self {
        IndexBufferData::Uint16(bytemuck::cast_slice(array))
    }

    pub fn from_array_u32<const N: usize>(array: &'a [[u32; N]]) -> Self {
        IndexBufferData::Uint32(bytemuck::cast_slice(array))
    }
}

#[derive(Copy, Clone, Debug)]
pub struct VertexBufferDescriptor<'a, T: bytemuck::NoUninit> {
    pub vertices: &'a [T],
    pub attributes: &'a [wgpu::VertexAttribute],
    pub index_buffer: Option<IndexBufferData<'a>>,
    pub primitives: wgpu::PrimitiveTopology,
}

#[derive(Debug)]
pub struct IndexBuffer {
    pub format: wgpu::IndexFormat,
    pub buffer: wgpu::Buffer,
    pub index_count: u32,
}

#[derive(Debug)]
pub struct VertexBuffer {
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_layout: wgpu::VertexBufferLayout<'static>,
    pub vertex_count: u32,
    pub index_buffer: Option<IndexBuffer>,
    pub primitives: wgpu::PrimitiveTopology,
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

    /// texture format must be Rgba8Unorm or Bgra8Unorm, or their srgb variants.
    pub fn read(&self, ctx: &Ctx, sender: std::sync::mpsc::Sender<Vec<u8>>) {
        let width = self.texture.width();
        let height = self.texture.height();

        let format = self.texture.format();
        assert!(format == wgpu::TextureFormat::Bgra8Unorm 
                || format == wgpu::TextureFormat::Bgra8UnormSrgb 
                || format == wgpu::TextureFormat::Rgba8Unorm);

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

    pub fn dispatch_count(&self, workgroup_size: [u32; 2]) -> [u32; 3] {
        let size = self.texture.size();
        assert!(workgroup_size[0] != 0);
        assert!(workgroup_size[1] != 0);
        let w_width = workgroup_size[0];
        let w_height = workgroup_size[1];
        [(size.width + w_width-1)/w_width, (size.height + w_height-1)/w_height, 1]
    }
}

struct Update;

#[derive(Debug, Copy, Clone)]
pub enum WindowEvent<'a> { 
    Redraw {
        output: &'a RenderTexture,
    },
    Update {
        delta: f32,
        keys: KeyEvents<'a>,
    }, 
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WindowTask { Redraw, Exit, }

pub use winit::keyboard::KeyCode as Key;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum KeyState {
    JustPressed,
    Held,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub state: KeyState,
    pub(crate) queued_release: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct KeyEvents<'a> {
    pub events: &'a [KeyEvent],
}

impl<'a> KeyEvents<'a> {
    pub fn just_pressed(&self, key: Key) -> bool {
        self.events.iter().any(|event| event.key == key && event.state == KeyState::JustPressed)
    }

    pub fn down(&self, key: Key) -> bool {
        self.events.iter().any(|event| event.key == key)
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
