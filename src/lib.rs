#[cfg(test)]
mod tests;

use wgpu::util::DeviceExt;

const DEBUG_LINES: bool = false;

#[cfg(feature = "vello")]
pub use vello;

pub use wgpu;
pub use bytemuck;

pub struct Ctx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub instance: wgpu::Instance,

    pub output_texture_format: wgpu::TextureFormat,
    pub copy_pipeline_layout: wgpu::PipelineLayout,
    pub copy_bind_group_layout: wgpu::BindGroupLayout,
    pub copy_sampler_nearest: wgpu::Sampler,
    pub copy_sampler_linear: wgpu::Sampler,
    pub copy_shader: wgpu::ShaderModule,

    pub alloc: &'static bumpalo::Bump,

    #[cfg(feature = "vello")]
    pub vello_renderer: std::cell::RefCell<vello::Renderer>,
}

pub struct CtxDescriptor {
    pub srgb_output_format: bool,
}

impl Default for CtxDescriptor {
    fn default() -> CtxDescriptor {
        CtxDescriptor {
            srgb_output_format: true,
        }
    }
}

impl Ctx {
    pub fn new() -> Self {
        Self::new_ex(CtxDescriptor::default())
    }

    pub fn new_ex(desc: CtxDescriptor) -> Self {
        pollster::block_on(Self::new_ex_async(desc))
    }

    pub async fn new_ex_async(desc: CtxDescriptor) -> Self {
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
                    | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
                    | wgpu::Features::TEXTURE_BINDING_ARRAY
                    | wgpu::Features::TIMESTAMP_QUERY
                    | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                    | if DEBUG_LINES {
                        wgpu::Features::POLYGON_MODE_LINE
                    } else {
                        wgpu::Features::empty()
                    },
                required_limits: Default::default(),
            },
            None
        ).await.unwrap();

        let output_texture_format = if desc.srgb_output_format {
            wgpu::TextureFormat::Bgra8UnormSrgb
        } else {
            wgpu::TextureFormat::Bgra8Unorm
        };

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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let copy_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&copy_bind_group_layout],
            push_constant_ranges: &[],
        });

        let copy_sampler_linear = device.create_sampler(&wgpu::SamplerDescriptor {
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let copy_sampler_nearest = device.create_sampler(&Default::default());
        
        #[cfg(feature = "vello")]
        let vello_renderer = std::cell::RefCell::new(vello::Renderer::new(&device, vello::RendererOptions {
            use_cpu: false,
            surface_format: Some(output_texture_format),
            antialiasing_support: vello::AaSupport::area_only(),
            num_init_threads: None,
        }).unwrap());

        Self {
            device, queue, instance,
            output_texture_format,
            copy_pipeline_layout, copy_bind_group_layout, copy_sampler_linear, copy_sampler_nearest, copy_shader,
            alloc: Box::leak(Box::new(bumpalo::Bump::new())),

            #[cfg(feature = "vello")]
            vello_renderer,
        }
    }

    #[cfg(feature = "winit")]
    pub fn run<F>(
        &self,
        size: (u32, u32),
        frames_per_second: u32,
        mut f: F,
    ) where
        F: FnMut(&mut wgpu::CommandEncoder, &RenderTexture, f32, Input) -> Option<WindowTask>,
    {
        let mut delta = 0.0;
        let mut keys = Vec::with_capacity(16);
        let mut mouse_position = None;
        let mut mouse_scroll = 0.0;
        let mut mouse_buttons = MouseButtons { left: None, middle: None, right: None };

        self.run_ex(
            size, 
            frames_per_second,
            |ev| match ev {
                WindowEvent::Update { delta: new_delta, input: new_input } => {
                    mouse_position = new_input.mouse_position;
                    mouse_scroll = new_input.mouse_scroll;
                    keys.clear();
                    keys.extend_from_slice(new_input.key_events);
                    delta = new_delta;
                    mouse_buttons = new_input.mouse_buttons;
                    Some(WindowTaskEx::Redraw)
                }
                WindowEvent::Redraw { output } => {
                    let mut encoder = self.device.create_command_encoder(&Default::default());
                    let input = Input { key_events: &keys, mouse_position, mouse_scroll, mouse_buttons };
                    let event = (f)(&mut encoder, output, delta, input);
                    self.queue.submit(std::iter::once(encoder.finish()));
                    event.map(WindowTaskEx::from)
                }
            }
        )
    }

    #[cfg(feature = "winit")]
    pub fn run_ex<F>(
        &self,
        size: (u32, u32),
        updates_per_second: u32,
        f: F,
    ) where
        F: FnMut(WindowEvent) -> Option<WindowTaskEx>,
    {
        use winit::{
            application::ApplicationHandler,
            event_loop::{ActiveEventLoop, EventLoop},
            window::{WindowId, Window},
        };

        struct Update;

        struct PreInitState<'a, F> {
            pub ctx: &'a Ctx,
            pub size: (u32, u32),
            pub update_period: std::time::Duration,
            pub f: F,
        }

        struct State<'a, F> {
            pub ctx: &'a Ctx,
            pub window: &'static Window,
            pub surface: wgpu::Surface<'static>,
            pub surface_config: wgpu::SurfaceConfiguration,

            pub prev_update_instant: Option<std::time::Instant>,
            pub update_period: std::time::Duration,
            pub output: RenderTexture,
            pub f: F,

            pub keys: Vec<KeyEvent>,
            pub mouse_position: Option<(f32, f32)>,
            pub mouse_scroll: f32,
            pub mouse_buttons: MouseButtons,
        }

        enum StateMaybe<'a, F> {
            Uninit(PreInitState<'a, F>),
            Init(State<'a, F>),
        }

        impl<'a, F> State<'a, F> {
            fn init(init: PreInitState<'a, F>, event_loop: &ActiveEventLoop) -> Self {
                let ctx = init.ctx;
                let window = event_loop.create_window(
                    Window::default_attributes()
                        .with_min_inner_size(winit::dpi::PhysicalSize::new(init.size.0, init.size.1))
                        .with_max_inner_size(winit::dpi::PhysicalSize::new(init.size.0, init.size.1))
                ).unwrap();
                let window: &'static Window = ctx.alloc.alloc(window);

                let size = window.inner_size();
                let surface_config = wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: ctx.output_texture_format,
                    width: size.width,
                    height: size.height,
                    present_mode: wgpu::PresentMode::AutoVsync,
                    alpha_mode: wgpu::CompositeAlphaMode::Auto,
                    desired_maximum_frame_latency: 2,
                    view_formats: vec![],
                };
                let surface = ctx.instance.create_surface(window).unwrap();
                surface.configure(&ctx.device, &surface_config);

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

                let depth_texture = ctx.device.create_texture(&depth_texture_desc);
                let depth_view = depth_texture.create_view(&Default::default());

                let null_texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
                    size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                    ..depth_texture_desc
                });
                let null_view = null_texture.create_view(&Default::default());

                State {
                    ctx,
                    window,
                    surface,
                    surface_config,

                    prev_update_instant: None,
                    update_period: init.update_period,

                    output: RenderTexture {
                        texture: null_texture,
                        view: null_view,
                        depth_texture,
                        depth_view,
                    },

                    f: init.f,

                    keys: Vec::with_capacity(16),
                    mouse_position: None,
                    mouse_scroll: 0.0,
                    mouse_buttons: MouseButtons { left: None, middle: None, right: None },
                }
            }
        }

        impl<'a, F> ApplicationHandler<Update> for StateMaybe<'a, F> 
            where F: FnMut(WindowEvent) -> Option<WindowTaskEx>
        {
            fn resumed(&mut self, event_loop: &ActiveEventLoop) {
                take_mut::take(self, |s| {
                    match s {
                        StateMaybe::Uninit(init) => {
                            StateMaybe::Init(State::init(init, event_loop))
                        }
                        _ => s,
                    }
                })
            }

            fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: Update) {
                let st = match self {
                    StateMaybe::Uninit(..) => return,
                    StateMaybe::Init(ref mut st) => st,
                };

                let now = std::time::Instant::now();
                let delta = match st.prev_update_instant {
                    Some(i) => {
                        let diff: std::time::Duration = now - i;
                        diff.as_secs_f32()
                    }
                    None => st.update_period.as_secs_f32(),
                };
                st.prev_update_instant = Some(now);

                let input = Input { 
                    key_events: &st.keys, 
                    mouse_position: st.mouse_position, 
                    mouse_scroll: st.mouse_scroll, 
                    mouse_buttons: st.mouse_buttons, 
                };
                match (st.f)(WindowEvent::Update { delta, input }) {
                    Some(WindowTaskEx::Redraw) => st.window.request_redraw(),
                    Some(WindowTaskEx::Exit) => event_loop.exit(),
                    None => (),
                }

                for k in st.keys.iter_mut() {
                    k.state = KeyState::Held;
                }
                st.mouse_scroll = 0.0;
                if st.mouse_buttons.left.is_some() { st.mouse_buttons.left = Some(KeyState::Held); }
                if st.mouse_buttons.middle.is_some() { st.mouse_buttons.middle = Some(KeyState::Held); }
                if st.mouse_buttons.right.is_some() { st.mouse_buttons.right = Some(KeyState::Held); }
            }

            fn window_event(
                &mut self, 
                event_loop: &ActiveEventLoop, 
                window_id: WindowId, 
                event: winit::event::WindowEvent
            ) {
                let st = match self {
                    StateMaybe::Uninit(..) => return,
                    StateMaybe::Init(ref mut st) => st,
                };

                if window_id != st.window.id() { return; }

                let ctx = st.ctx;
                
                match event {
                    winit::event::WindowEvent::RedrawRequested => {
                        let mut surface_texture = match st.surface.get_current_texture() {
                            Ok(s) => s,
                            Err(wgpu::SurfaceError::Timeout) => return,
                            Err(e) => panic!("{}", e),
                        };

                        let surface_size = surface_texture.texture.size();
                        if surface_size != st.output.depth_texture.size() {
                            st.output.depth_texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
                                label: None,
                                size: surface_size,
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D2,
                                format: wgpu::TextureFormat::Depth32Float,
                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                                view_formats: &[],
                            });
                            st.output.depth_view = st.output.depth_texture.create_view(&Default::default())
                        }

                        take_mut::take(&mut surface_texture.texture, |texture| {
                            let view = texture.create_view(&Default::default());
                            let null_texture = std::mem::replace(&mut st.output.texture, texture);
                            let null_view = std::mem::replace(&mut st.output.view, view);

                            let task = (st.f)(WindowEvent::Redraw { output: &st.output });
                            
                            match task {
                                Some(WindowTaskEx::Redraw) => st.window.request_redraw(),
                                Some(WindowTaskEx::Exit) => event_loop.exit(),
                                None => (),
                            }

                            let texture = std::mem::replace(&mut st.output.texture, null_texture);
                            st.output.view = null_view;
                            texture
                        });

                        st.window.pre_present_notify();
                        surface_texture.present();
                    },
                    winit::event::WindowEvent::CursorMoved { position, .. } => {
                        st.mouse_position = Some((position.x as f32, position.y as f32));
                    },
                    winit::event::WindowEvent::CursorLeft { .. } => {
                        st.mouse_position = None;
                    },
                    winit::event::WindowEvent::MouseWheel { delta, .. } => {
                        match delta {
                            winit::event::MouseScrollDelta::LineDelta(_, y) => {
                                st.mouse_scroll += y;
                            }
                            _ => (),
                        }
                    },
                    winit::event::WindowEvent::MouseInput { 
                        state: winit::event::ElementState::Pressed, button, .. 
                    } => {
                        match button {
                            winit::event::MouseButton::Left => st.mouse_buttons.left = Some(KeyState::JustPressed),
                            winit::event::MouseButton::Middle => st.mouse_buttons.middle = Some(KeyState::JustPressed),
                            winit::event::MouseButton::Right => st.mouse_buttons.right = Some(KeyState::JustPressed),
                            _ => (),
                        }
                    },
                    winit::event::WindowEvent::MouseInput { 
                        state: winit::event::ElementState::Released, button, .. 
                    } => {
                        match button {
                            winit::event::MouseButton::Left => st.mouse_buttons.left = None,
                            winit::event::MouseButton::Middle => st.mouse_buttons.middle = None,
                            winit::event::MouseButton::Right => st.mouse_buttons.right = None,
                            _ => (),
                        }
                    },
                    winit::event::WindowEvent::Resized(new_size) => {
                        st.surface_config.width = new_size.width;
                        st.surface_config.height = new_size.height;
                        st.surface.configure(&ctx.device, &st.surface_config);
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
                        if st.keys.iter().any(|k| k.key == physical_key) { return };
                        st.keys.push(KeyEvent { key: physical_key, state: KeyState::JustPressed })
                    },
                    winit::event::WindowEvent::KeyboardInput {
                        event: winit::event::KeyEvent { 
                            state: winit::event::ElementState::Released, 
                            physical_key: winit::keyboard::PhysicalKey::Code(physical_key), 
                            .. 
                        },
                        ..
                    } => {
                        st.keys.retain_mut(|k| k.key != physical_key);
                    }
                    winit::event::WindowEvent::CloseRequested => event_loop.exit(),
                    _ => (),
                }
            }
        }

        let update_period = std::time::Duration::from_secs_f64((updates_per_second as f64).recip());

        let mut st = StateMaybe::Uninit(PreInitState {
            ctx: self,
            size,
            update_period,
            f,
        });

        let event_loop = EventLoop::with_user_event().build().unwrap();
        let event_sender = event_loop.create_proxy();
        std::thread::spawn(move || {
            loop {
                let _ = event_sender.send_event(Update);
                std::thread::sleep(update_period);
            }
        });

        event_loop.run_app(&mut st).unwrap();
    }

    #[cfg(feature = "vello")]
    pub fn draw_scene(
        &self,
        scene: &vello::Scene,
        output: &Texture,
        base_colour: vello::peniko::Color,
    ) {
        assert_eq!(
            output.texture.format(),
            wgpu::TextureFormat::Rgba8Unorm,
            "Vello requires that textures have format Rgba8Unorm"
        );

        let size = output.texture.size();
        self.vello_renderer.borrow_mut().render_to_texture(
            &self.device,
            &self.queue,
            scene, 
            &output.view,
            &vello::RenderParams {
                base_color: base_colour,
                width: size.width,
                height: size.height,
                antialiasing_method: vello::AaConfig::Area,
            }
        ).unwrap();
    }

    // #[cfg(feature = "video")]
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

    pub fn create_sampler(&self, oob: wgpu::AddressMode, filter: wgpu::FilterMode) -> wgpu::Sampler {
        self.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: oob,
            address_mode_v: oob,
            address_mode_w: oob,
            mag_filter: filter,
            min_filter: filter,
            ..Default::default()
        })
    }

    pub fn create_instance_buffer<T: bytemuck::NoUninit>(
        &self, 
        desc: InstanceBufferDescriptor<'_, T>,
    ) -> InstanceBuffer {
        let attributes: &'static [wgpu::VertexAttribute] = self.alloc.alloc_slice_copy(desc.attributes);
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<T>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes,
        };

        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(desc.instances),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let count = desc.instances.len() as u32;

        InstanceBuffer {
            buffer,
            vertex_layout,
            count
        }
    }

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
        self.create_storage_buffer_ex::<T>(Either::A(data), wgpu::BufferUsages::empty())
    }

    pub fn create_storage_buffer_empty<T: bytemuck::NoUninit>(&self, len: usize) -> StorageBuffer {
        self.create_storage_buffer_ex::<T>(Either::B(len), wgpu::BufferUsages::empty())
    }

    pub fn create_storage_buffer_ex<T: bytemuck::NoUninit>(
        &self, 
        data_or_size: Either<&[T], usize>,
        extra_usages: wgpu::BufferUsages,
    ) -> StorageBuffer {
        if std::alloc::Layout::new::<T>().size() == 0 { panic!("buffer size cannot be zero") }


        let layout = std::alloc::Layout::new::<T>();
        let usage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC 
            | wgpu::BufferUsages::COPY_DST | extra_usages;

        let buffer = match data_or_size {
            Either::A(data) => {
                if data.len() == 0 { panic!("buffer size cannot be zero") }
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(data),
                    usage,
                })
            },
            Either::B(len) => {
                if len == 0 { panic!("buffer size cannot be zero") }
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: len as u64 * layout.size() as u64,
                    usage,
                    mapped_at_creation: false,
                })
            }
        };

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

    pub fn create_storage_texture(&self, size: (u32, u32), format: StorageTextureFormat) -> Texture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: format.into(),
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

    pub fn create_storage_texture_with_data<T: bytemuck::NoUninit>(
        &self,
        size: (u32, u32), 
        format: StorageTextureFormat,
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
                format: format.into(),
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
                | wgpu::TextureUsages::RENDER_ATTACHMENT
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

    /// Requires texture formats to only differ in Srgb-ness and have the same size and dimensions.
    pub fn create_texture_copier_fast<'a>(&self, src: &'a Texture, dst: &'a Texture) -> TextureCopier<'a> {
        let src_format = src.texture.format();
        let dst_format = dst.texture.format();
        assert_eq!(
            src_format.remove_srgb_suffix(), dst_format.remove_srgb_suffix(),
            "Ctx::create_texture_copier_fast: Texture formats do not match: src = {:?}, dst = {:?}\nFormats may only differ in Srgb-ness", 
            src_format,
            dst_format
        );

        let src_dim = src.texture.dimension();
        let dst_dim = dst.texture.dimension();
        assert_eq!(
            src_dim, dst_dim,
            "Ctx::create_texture_copier_fast: Texture dimensions do not match: src = {:?}, dst = {:?}", 
            src_dim,
            dst_dim
        );

        let src_size = src.texture.size();
        let dst_size = dst.texture.size();
        assert_eq!(
            src_size, dst_size,
            "Ctx::create_texture_copier_fast: Texture sizes do not match: src = {:?}, dst = {:?}", 
            src_size,
            dst_size
        );
        
        TextureCopier::Fast { src, dst }
    }

    /// Does not support integer texture formats (Uint, Sint).
    pub fn create_texture_copier<'a>(
        &self, 
        src: &'a Texture, 
        dst: &'a Texture,
        scaling_type: ScalingType,
    ) -> TextureCopier<'a> {
        self.create_texture_copier_ex(TextureCopierDescriptorEx {
            src, 
            dst, 
            scaling_type,
            clear_colour: Some(wgpu::Color::BLACK),
        })
    }

    pub fn create_texture_copier_ex<'a>(&self, desc: TextureCopierDescriptorEx<'a>) -> TextureCopier<'a> {
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&self.copy_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.copy_shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.copy_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: desc.dst.texture.format(),
                    blend: if desc.clear_colour.is_none() {
                        Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING)
                    } else {
                        None
                    },
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
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
                    resource: wgpu::BindingResource::TextureView(&desc.src.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(match desc.scaling_type {
                        ScalingType::Linear => &self.copy_sampler_linear,
                        ScalingType::Nearest => &self.copy_sampler_nearest,
                    }),
                },
            ],
        });

        match desc.clear_colour {
            Some(c) => TextureCopier::Slow { pipeline, bind_group, dst: desc.dst, clear_colour: c },
            None => TextureCopier::Transparent { pipeline, bind_group, dst: desc.dst },
        }
    }

    pub fn create_screen_copier<'a>(&self, src: &'a Texture, scaling_type: ScalingType) -> ScreenCopier {
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&self.copy_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.copy_shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.copy_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.output_texture_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
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
                    resource: wgpu::BindingResource::Sampler(match scaling_type {
                        ScalingType::Linear => &self.copy_sampler_linear,
                        ScalingType::Nearest => &self.copy_sampler_nearest,
                    }),
                },
            ],
        });

        ScreenCopier { pipeline, bind_group }
    }

    pub fn create_render_pipeline<'a, 'b>(
        &self, 
        desc: RenderPipelineDescriptor<'a, 'b>
    ) -> RenderPipeline<'a> {
        self.create_render_pipeline_ex(desc.into())
    }

    pub fn create_render_pipeline_ex<'a, 'b>(
        &self, 
        desc: RenderPipelineDescriptorEx<'a, 'b>
    ) -> RenderPipeline<'a> {
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

        let wgpu_shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(desc.shader.to_cow()),
        });

        let (vertex_buffer, primitives, vbuffers): (_, _, &'static _) = match (desc.vertex_buffer, desc.instance_buffer) {
            (Either::A(vbo), None) => (
                Some(vbo), 
                vbo.primitives, 
                self.alloc.alloc_slice_clone(&[vbo.vertex_layout.clone()])
            ),
            (Either::A(vbo), Some(ibo)) => (
                Some(vbo), 
                vbo.primitives, 
                self.alloc.alloc_slice_clone(&[vbo.vertex_layout.clone(), ibo.vertex_layout.clone()])
            ),
            (Either::B(primitives), None) => (None, primitives, &[]),
            (Either::B(primitives), Some(ibo))  => (
                None, 
                primitives, 
                self.alloc.alloc_slice_clone(&[ibo.vertex_layout.clone()])
            ),
        };

        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            })),
            vertex: wgpu::VertexState {
                module: &wgpu_shader,
                entry_point: desc.shader_vertex_entry,
                buffers: vbuffers,
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: primitives,
                cull_mode: desc.cull_mode,
                polygon_mode: if DEBUG_LINES {
                    wgpu::PolygonMode::Line
                } else {
                    wgpu::PolygonMode::Fill
                },
                ..Default::default()
            },
            depth_stencil: if !desc.disable_depth_test {
                Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: Default::default(),
                    bias: Default::default(),
                })
            } else { 
                None 
            },
            multisample: wgpu::MultisampleState {
                //count: desc.multisample_count,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &wgpu_shader,
                entry_point: desc.shader_fragment_entry,
                targets: &[Some(wgpu::ColorTargetState {
                    format: desc.output_format,
                    blend: desc.blend_state,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
        });

        RenderPipeline {
            wgpu_pipeline: pipeline,
            shader: desc.shader,
            bind_group: bind_group,
            vertex_buffer,
            instance_buffer: desc.instance_buffer,
            instance_range: desc.instance_range,
            draw_range: desc.draw_range,
            disable_depth_test: desc.disable_depth_test,
            output_format: desc.output_format,
        }
    }

    pub fn create_compute_pipeline(
        &self, 
        desc: ComputePipelineDescriptor<'_>
    ) -> ComputePipeline {
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

        let wgpu_shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(desc.shader.to_cow()),
        });

        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(&layout),
            module: &wgpu_shader,
            entry_point: desc.shader_entry,
            compilation_options: Default::default(),
        });

        ComputePipeline {
            wgpu_pipeline: pipeline,
            shader: desc.shader,
            layout,
            shader_entry: desc.shader_entry,
            bind_group,
            bind_group_layout,
            dispatch_count: desc.dispatch_count,
        }
    }

    pub fn run_render_pipeline<'a>(&self, pass: &mut wgpu::RenderPass<'a>, pipeline: &'a RenderPipeline) {
        pass.set_pipeline(&pipeline.wgpu_pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        let draw_range = pipeline.draw_range.clone();
        let instance_range = pipeline.instance_range.clone();

        match (pipeline.vertex_buffer, pipeline.instance_buffer) {
            (None, None) => pass.draw(draw_range, instance_range),
            (Some(v), i) => {
                pass.set_vertex_buffer(0, v.vertex_buffer.slice(..));


                if let Some(i) = i {
                    pass.set_vertex_buffer(1, i.buffer.slice(..));
                }

                if let Some(ref ib) = v.index_buffer {
                    pass.set_index_buffer(ib.buffer.slice(..), ib.format);
                    pass.draw_indexed(draw_range, 0, instance_range);
                } else {
                    pass.draw(draw_range, instance_range);
                }
            },
            (None, Some(i)) => {
                pass.set_vertex_buffer(0, i.buffer.slice(..));
                pass.draw(draw_range, instance_range);
            }
        }
    }

    pub fn run_compute_pipeline<'a>(&self, pass: &mut wgpu::ComputePass<'a>, pipeline: &'a ComputePipeline) {
        //let t = std::time::Instant::now();
        //if let ShaderSource::File(path) = pipeline.shader {
        //    if let Ok(metadata) = std::fs::metadata(path) {
        //        if let Ok(modified) = metadata.modified() {
        //            println!("modified: {:?}", modified);
        //        }
        //    }
        //}
        //println!("{}", t.elapsed().as_nanos());
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
        for pass in passes {
            assert_eq!(
                pass.output_format, output.texture.format(),
                "Ctx::run_render_pass: Output texture format )ust match the output formats of the RenderPipelines"
            )
        }

        if passes.len() > 0 {
            let disable_depth_test = passes[0].disable_depth_test;
            if passes[1..].iter().any(|pass| pass.disable_depth_test != disable_depth_test) {
                panic!("run_render_pass: RenderPipeline disable_depth_test must not be varied across pipelines in a single pass");
            }

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
                depth_stencil_attachment: if !disable_depth_test {
                    Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &output.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    })
                } else {
                    None
                },
                occlusion_query_set: None,
                timestamp_writes: None,
            };
            let mut pass = encoder.begin_render_pass(&desc);
            for pipeline in passes {
                self.run_render_pipeline(&mut pass, pipeline);
            }
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
        assert_eq!(
            src.buffer.size(), dst.buffer.size(), 
           "Ctx::copy_buffer_to_buffer: buffers must be the same size"
        );

        assert_eq!(
            src.layout, dst.layout, 
           "Ctx::copy_buffer_to_buffer: buffers must contain the same type"
        );
        encoder.copy_buffer_to_buffer(&src.buffer, 0, &dst.buffer, 0, src.buffer.size());
    }

    pub fn copy_texture_to_screen(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        copier: &ScreenCopier,
        output: &RenderTexture,
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
            TextureCopier::Slow { ref pipeline, ref bind_group, dst, clear_colour } => {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &dst.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(*clear_colour),
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

    pub fn create_timer(&self) -> GPUTimer {
        let query_set = self.device.create_query_set(&wgpu::QuerySetDescriptor {
            label: None,
            ty: wgpu::QueryType::Timestamp,
            count: GPUTimer::MAX_TIMESTAMP_COUNT,
        });

        let timestamp_period = self.queue.get_timestamp_period();
        if timestamp_period == 0.0 { panic!("timestamps are unsupported on this machine") }

        let query_resolution_buffer = self.create_storage_buffer_ex::<u64>(
            Either::B(GPUTimer::MAX_TIMESTAMP_COUNT as _),
            wgpu::BufferUsages::QUERY_RESOLVE
        );

        GPUTimer {
            query_set,
            timestamp_period,
            query_resolution_buffer,

            timestamp_idx: 0,
            timestamp_labels: Vec::new(),
            ctx: self
        }
    }
}

pub struct GPUTimer<'a> {
    pub query_set: wgpu::QuerySet,
    pub query_resolution_buffer: StorageBuffer,
    pub timestamp_period: f32,

    pub timestamp_idx: u32,
    pub timestamp_labels: Vec<&'static str>,
    pub ctx: &'a Ctx,
}

impl<'a> GPUTimer<'a> {
    pub const MAX_TIMESTAMP_COUNT: u32 = 255u32;

    pub fn start(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.timestamp_idx = 0;
        self.write_timestamp(encoder);
        self.timestamp_labels.clear();
    }

    pub fn split(&mut self, encoder: &mut wgpu::CommandEncoder, label: &'static str) {
        if self.timestamp_idx == 255 { 
            eprintln!("GPUTimer misuse - too many splits: call start, then splits, then print");
            return 
        }
        self.timestamp_labels.push(label);
        self.write_timestamp(encoder);
    }

    pub fn print(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if self.timestamp_idx as usize != self.timestamp_labels.len()+1 { 
            eprintln!("GPUTimer misuse: call start, then splits, then print");
            return;
        }

        encoder.resolve_query_set(
            &self.query_set, 
            0..self.timestamp_idx, 
            &self.query_resolution_buffer.buffer,
            0
        );

        let times = self.query_resolution_buffer.read_to_vec::<u64>(self.ctx);

        let mut start_time = times[0];
        let period = self.timestamp_period as f64;
        for i in 1..(self.timestamp_idx as usize) {
            let timestamp = times[i];
            let label = self.timestamp_labels[i-1];
            let t = (timestamp - start_time) as f64 * period;
            if t > 1_000_000_000.0 {
                println!("{}: {:3}s", label, t / 1_000_000_000.0);
            } else if t > 1_000_000.0 {
                println!("{}: {:3}ms", label, t / 1_000_000.0);
            } else if t > 1_000.0 {
                println!("{}: {:3}us", label, t / 1_000.0);
            } else {
                println!("{}: {:3}ns", label, t);
            }
            start_time = timestamp;
        }
    }

    fn write_timestamp(&mut self, encoder: &mut wgpu::CommandEncoder) {
        encoder.write_timestamp(&self.query_set, self.timestamp_idx);
        self.timestamp_idx += 1;
    }
}

#[derive(Debug)]
pub struct ComputePipelineDescriptor<'a> {
    pub inputs: &'a [PipelineInput<'a>],
    pub outputs: &'a [ComputePipelineOutput<'a>],
    pub shader: ShaderSource,
    pub shader_entry: &'static str,
    pub dispatch_count: [u32; 3],
}

#[derive(Copy, Clone, Debug)]
pub enum ShaderSource {
    Str(&'static str),
    File(&'static std::path::Path),
}

impl ShaderSource {
    pub fn to_cow(self) -> std::borrow::Cow<'static, str> {
        match self {
            ShaderSource::Str(s) => std::borrow::Cow::Borrowed(s),
            ShaderSource::File(p) => {
                match std::fs::read_to_string(p) {
                    Ok(s) => std::borrow::Cow::Owned(s),
                    Err(e) => panic!("Could not open shader {}: {}", p.display(), e),
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct ComputePipeline {
    pub wgpu_pipeline: wgpu::ComputePipeline,
    pub bind_group: wgpu::BindGroup,
    pub layout: wgpu::PipelineLayout,
    pub shader: ShaderSource,
    pub shader_entry: &'static str,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub dispatch_count: [u32; 3],
}

#[derive(Debug)]
pub struct RenderPipelineDescriptor<'a, 'b> {
    pub inputs: &'b [PipelineInput<'b>],
    pub vertex_buffer: &'a VertexBuffer,
    pub shader: ShaderSource,
    pub shader_vertex_entry: &'static str,
    pub shader_fragment_entry: &'static str,
    pub output_format: wgpu::TextureFormat,
}

impl<'a, 'b> From<RenderPipelineDescriptor<'a, 'b>> for RenderPipelineDescriptorEx<'a, 'b> {
    fn from(desc: RenderPipelineDescriptor<'a, 'b>) -> Self {
        let draw_range = if let Some(ref ib) = desc.vertex_buffer.index_buffer {
            0..ib.index_count
        } else {
            0..desc.vertex_buffer.vertex_count
        };

        RenderPipelineDescriptorEx {
            inputs: desc.inputs,
            vertex_buffer: Either::A(desc.vertex_buffer),
            instance_buffer: None,
            shader: desc.shader,
            shader_vertex_entry: desc.shader_vertex_entry,
            shader_fragment_entry: desc.shader_fragment_entry,
            output_format: desc.output_format,
            draw_range,
            blend_state: None,
            cull_mode: None,
            instance_range: 0..1,
            disable_depth_test: false,
        }
    }
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
    pub instance_buffer: Option<&'a InstanceBuffer>,
    pub shader: ShaderSource,
    pub shader_vertex_entry: &'static str,
    pub shader_fragment_entry: &'static str,
    pub output_format: wgpu::TextureFormat,
    pub blend_state: Option<wgpu::BlendState>,
    pub cull_mode: Option<wgpu::Face>,

    pub draw_range: std::ops::Range<u32>,
    pub instance_range: std::ops::Range<u32>,
    
    /// This must not be varied across pipelines in a single pass.
    pub disable_depth_test: bool,
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
    pub instance_buffer: Option<&'a InstanceBuffer>,
    pub shader: ShaderSource,
    pub disable_depth_test: bool,
    pub output_format: wgpu::TextureFormat,

    /// these fields can be modified at runtime
    pub draw_range: std::ops::Range<u32>,
    pub instance_range: std::ops::Range<u32>,
}

#[derive(Copy, Clone, Debug)]
pub enum PipelineInput<'a> {
    Uniform(&'a Uniform),
    StorageBuffer(&'a StorageBuffer),
    Texture(&'a Texture),
    Sampler(&'a wgpu::Sampler),
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
            PipelineInput::Sampler(sampler) => wgpu::BindingResource::Sampler(sampler),
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
            PipelineInput::Sampler(_) => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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

#[derive(Copy, Clone, Debug)]
pub enum ScalingType {
    Nearest,
    Linear,
}

pub struct TextureCopierDescriptorEx<'a> {
    pub src: &'a Texture,
    pub dst: &'a Texture,
    pub scaling_type: ScalingType,
    pub clear_colour: Option<wgpu::Color>,
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
        dst: &'a Texture,
        clear_colour: wgpu::Color,
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
        assert_eq!(
            std::alloc::Layout::new::<T>(),
            self.layout,
            "Uniform::update: Cannot update a uniform with a different type than what the Uniform was instantiated with"
        );
        ctx.queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(std::slice::from_ref(data)));
    }
}

impl StorageBuffer {
    pub fn update<T: bytemuck::NoUninit>(&self, ctx: &Ctx, data: &[T]) {
        assert_eq!(
            std::alloc::Layout::new::<T>(),
            self.layout,
            "StorageBuffer::update: Cannot update a buffer with a different type than what the buffer was instantiated with"
        );
        ctx.queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(data));
    }

    /// Returns the number of elements in the buffer, not the size in bytes.
    pub fn len(&self) -> u32 {
        (self.buffer.size() / self.layout.pad_to_align().size() as u64) as u32
    }

    pub fn dispatch_count(&self, workgroup_size: u32) -> [u32; 3] {
        assert_ne!(
            workgroup_size,
            0,
            "StorageBuffer::dispatch: workgroup_size cannot be zero"
        );
        [(self.len() + workgroup_size-1)/workgroup_size, 1, 1]
    }

    /// Panics if `T` has a different layout that the buffer's type,
    /// or the buffer size and capacity are not a multiples of `T`'s size.
    pub fn read_to_vec<T: bytemuck::Pod>(&self, ctx: &Ctx) -> Vec<T> {
        assert_eq!(
            std::alloc::Layout::new::<T>(),
            self.layout,
            "StorageBuffer::read_to_vec: Cannot read a buffer into a Vec with a different type"
        );

        let buf_u8 = self.read_to_vec_bytes(ctx);

        let size_t = std::alloc::Layout::new::<T>().pad_to_align().size();
        assert_eq!(buf_u8.len() % size_t, 0, "Error satisfying type size and alignment");
        assert_eq!(buf_u8.capacity() % size_t, 0, "Error satisfying type size and alignment");

        unsafe {
            // Ensure the original vector is not dropped.
            let mut buf = std::mem::ManuallyDrop::new(buf_u8);
            Vec::from_raw_parts(buf.as_mut_ptr() as *mut T,
                                buf.len() / size_t,
                                buf.capacity() / size_t)
        }
    }

    pub fn read_to_vec_bytes(&self, ctx: &Ctx) -> Vec<u8> {
        let (sender, receiver) = std::sync::mpsc::channel::<Vec<u8>>();
        self.read(ctx, sender);
        match receiver.recv() {
            Ok(buf) => buf,
            Err(e) => panic!("reading data buffer failed: {}", e),
        }
    }

    pub fn read(&self, ctx: &Ctx, sender: std::sync::mpsc::Sender<Vec<u8>>) {
        let size = self.buffer.size();

        let intermediate_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&self.buffer, 0, &intermediate_buffer, 0, size);
        ctx.queue.submit(std::iter::once(encoder.finish()));

        let arc_buffer = std::sync::Arc::new(intermediate_buffer);
        let callback_buffer = arc_buffer.clone();

        arc_buffer.slice(..)
            .map_async(
                wgpu::MapMode::Read,
                move |res| {
                    match res {
                        Ok(_) => (),
                        Err(_) => {
                            eprintln!("buffer read failed");
                            return;
                        }
                    };

                    let data = callback_buffer.slice(..).get_mapped_range().to_vec();

                    match sender.send(data) {
                        Ok(_) => (),
                        Err(e) => eprintln!("buffer data send failed: {}", e),
                    }
                }
            );

        ctx.device.poll(wgpu::Maintain::Wait);
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
pub struct InstanceBufferDescriptor<'a, T: bytemuck::NoUninit> {
    pub instances: &'a [T],
    pub attributes: &'a [wgpu::VertexAttribute],
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

#[derive(Debug)]
pub struct InstanceBuffer {
    pub buffer: wgpu::Buffer,
    pub vertex_layout: wgpu::VertexBufferLayout<'static>,
    pub count: u32,
}

impl VertexBufferDescriptor<'static, [f32; 3]> {
    pub const CUBE: Self = VertexBufferDescriptor {
        vertices: &[
            [ 1.0,  1.0,  1.0],
            [-1.0,  1.0,  1.0],
            [ 1.0, -1.0,  1.0],
            [-1.0, -1.0,  1.0],
            [-1.0, -1.0, -1.0],
            [-1.0,  1.0,  1.0],
            [-1.0,  1.0, -1.0],
            [ 1.0,  1.0,  1.0],
            [ 1.0,  1.0, -1.0],
            [ 1.0, -1.0,  1.0],
            [ 1.0, -1.0, -1.0],
            [-1.0, -1.0, -1.0],
            [ 1.0,  1.0, -1.0],
            [-1.0,  1.0, -1.0]
        ],
        attributes: &wgpu::vertex_attr_array!(0 => Float32x3),
        index_buffer: None,
        primitives: wgpu::PrimitiveTopology::TriangleStrip,
    };
}

impl Texture {
    pub fn update<T: bytemuck::NoUninit>(&self, ctx: &Ctx, data: &[T]) {
        let size = self.texture.size();
        self.update_rect(
            ctx, data,
            (size.width, size.height), size.width,
            (0, 0),
        );
    }

    // TODO: type validation
    /// Every X value is in units of `T`.
    /// T **must** match the texel size of the texture.
    pub fn update_rect<T: bytemuck::NoUninit>(
        &self, ctx: &Ctx, data: &[T],
        input_size: (u32, u32),
        input_stride: u32,
        output_offset: (u32, u32),
    ) {
        ctx.queue.write_texture(
            wgpu::ImageCopyTexture {
                origin: wgpu::Origin3d {
                    x: output_offset.0,
                    y: output_offset.1,
                    z: 0,
                },
                ..self.texture.as_image_copy()
            },
            bytemuck::cast_slice(data),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(input_stride * std::mem::size_of::<T>() as u32),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: input_size.0,
                height: input_size.1,
                depth_or_array_layers: 1,
            },
        );
    }

    #[cfg(feature = "images")]
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
        assert!(
            format == wgpu::TextureFormat::Bgra8Unorm 
            || format == wgpu::TextureFormat::Bgra8UnormSrgb 
            || format == wgpu::TextureFormat::Rgba8Unorm
            || format == wgpu::TextureFormat::Rgba8UnormSrgb,
            "Texture::read: to read a texture, the format must be: Bgra8Unorm, Bgra8UnormSrgb, Rgba8Unorm, or Rgba8UnormSrgb"
        );

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

    pub fn dispatch_count(&self, workgroup_size: (u32, u32)) -> [u32; 3] {
        let size = self.texture.size();
        assert_ne!(workgroup_size.0, 0, "Texture::dispatch_count: workgroup_size cannot be zero");
        assert_ne!(workgroup_size.1, 0, "Texture::dispatch_count: workgroup_size cannot be zero");
        let w_width = workgroup_size.0;
        let w_height = workgroup_size.1;
        [(size.width + w_width-1)/w_width, (size.height + w_height-1)/w_height, 1]
    }
}

#[cfg(feature = "winit")]
mod winit_things {
    #[derive(Debug, Copy, Clone)]
    pub enum WindowEvent<'a> { 
        Redraw {
            output: &'a super::RenderTexture,
        },
        Update {
            delta: f32,
            input: Input<'a>,
        }, 
    }

    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub enum WindowTaskEx { Redraw, Exit, }

    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub enum WindowTask { Exit }

    impl From<WindowTask> for WindowTaskEx {
        fn from(task: WindowTask) -> Self {
            match task { WindowTask::Exit => WindowTaskEx::Exit }
        }
    }

    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub struct MouseButtons {
        pub left: Option<KeyState>,
        pub middle: Option<KeyState>,
        pub right: Option<KeyState>,
    }

    pub use winit::keyboard::KeyCode as Key;

    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub enum KeyState {
        JustPressed,
        Held,
    }

    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub struct KeyEvent {
        pub key: Key,
        pub state: KeyState,
    }

    #[derive(Copy, Clone, Debug)]
    pub struct Input<'a> {
        pub key_events: &'a [KeyEvent],
        pub mouse_position: Option<(f32, f32)>,
        
        /// positive -> scroll down, negative -> scroll up
        pub mouse_scroll: f32,
        pub mouse_buttons: MouseButtons,
    }

    impl<'a> Input<'a> {
        pub fn just_pressed(&self, key: Key) -> bool {
            self.key_events.iter().any(|event| event.key == key && event.state == KeyState::JustPressed)
        }

        pub fn held(&self, key: Key) -> bool {
            self.key_events.iter().any(|event| event.key == key)
        }
    }

}

#[cfg(feature = "winit")]
pub use winit_things::*;

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

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum StorageTextureFormat {
    R8Unorm,
    Rg8Unorm,
    Rgba8Unorm,

    R8Snorm,
    Rg8Snorm,
    Rgba8Snorm,

    R8Uint,
    Rg8Uint,
    Rgba8Uint,

    R8Sint,
    Rg8Sint,
    Rgba8Sint,

    R16Unorm,
    Rg16Unorm,
    Rgba16Unorm,

    R16Snorm,
    Rg16Snorm,
    Rgba16Snorm,

    R16Uint,
    Rg16Uint,
    Rgba16Uint,

    R16Sint,
    Rg16Sint,
    Rgba16Sint,

    R16Float,
    Rg16Float,
    Rgba16Float,

    R32Uint,
    Rg32Uint,
    Rgba32Uint,

    R32Sint,
    Rg32Sint,
    Rgba32Sint,

    R32Float,
    Rg32Float,
    Rgba32Float,
}

impl Into<wgpu::TextureFormat> for StorageTextureFormat {
    fn into(self) -> wgpu::TextureFormat {
        match self {
            Self::R8Unorm     => wgpu::TextureFormat::R8Unorm    ,
            Self::Rg8Unorm    => wgpu::TextureFormat::Rg8Unorm   ,
            Self::Rgba8Unorm  => wgpu::TextureFormat::Rgba8Unorm ,
            Self::R8Snorm     => wgpu::TextureFormat::R8Snorm    ,
            Self::Rg8Snorm    => wgpu::TextureFormat::Rg8Snorm   ,
            Self::Rgba8Snorm  => wgpu::TextureFormat::Rgba8Snorm ,
            Self::R8Uint      => wgpu::TextureFormat::R8Uint     ,
            Self::Rg8Uint     => wgpu::TextureFormat::Rg8Uint    ,
            Self::Rgba8Uint   => wgpu::TextureFormat::Rgba8Uint  ,
            Self::R8Sint      => wgpu::TextureFormat::R8Sint     ,
            Self::Rg8Sint     => wgpu::TextureFormat::Rg8Sint    ,
            Self::Rgba8Sint   => wgpu::TextureFormat::Rgba8Sint  ,
            Self::R16Unorm    => wgpu::TextureFormat::R16Unorm   ,
            Self::Rg16Unorm   => wgpu::TextureFormat::Rg16Unorm  ,
            Self::Rgba16Unorm => wgpu::TextureFormat::Rgba16Unorm,
            Self::R16Snorm    => wgpu::TextureFormat::R16Snorm   ,
            Self::Rg16Snorm   => wgpu::TextureFormat::Rg16Snorm  ,
            Self::Rgba16Snorm => wgpu::TextureFormat::Rgba16Snorm,
            Self::R16Uint     => wgpu::TextureFormat::R16Uint    ,
            Self::Rg16Uint    => wgpu::TextureFormat::Rg16Uint   ,
            Self::Rgba16Uint  => wgpu::TextureFormat::Rgba16Uint ,
            Self::R16Sint     => wgpu::TextureFormat::R16Sint    ,
            Self::Rg16Sint    => wgpu::TextureFormat::Rg16Sint   ,
            Self::Rgba16Sint  => wgpu::TextureFormat::Rgba16Sint ,
            Self::R16Float    => wgpu::TextureFormat::R16Float   ,
            Self::Rg16Float   => wgpu::TextureFormat::Rg16Float  ,
            Self::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
            Self::R32Uint     => wgpu::TextureFormat::R32Uint    ,
            Self::Rg32Uint    => wgpu::TextureFormat::Rg32Uint   ,
            Self::Rgba32Uint  => wgpu::TextureFormat::Rgba32Uint ,
            Self::R32Sint     => wgpu::TextureFormat::R32Sint    ,
            Self::Rg32Sint    => wgpu::TextureFormat::Rg32Sint   ,
            Self::Rgba32Sint  => wgpu::TextureFormat::Rgba32Sint ,
            Self::R32Float    => wgpu::TextureFormat::R32Float   ,
            Self::Rg32Float   => wgpu::TextureFormat::Rg32Float  ,
            Self::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
        }
    }
}
