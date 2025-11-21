use {
    crate::v4l2_capture::Yu12Frame,
    eframe::egui_wgpu::CallbackTrait,
    std::sync::{Arc, Mutex},
    wgpu::util::DeviceExt,
};

/// Shared rotation state between windows
pub type RotationState = Arc<Mutex<u32>>;

/// Toolbar viewport ID
const TOOLBAR_VIEWPORT: &str = "toolbar_viewport";

/// Custom wgpu render callback for YUV frame
struct YuvRenderCallback {
    frame: Arc<Yu12Frame>,
    rotation: u32,
}

impl CallbackTrait for YuvRenderCallback {
    fn paint(
        &self,
        _info: eframe::egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &eframe::egui_wgpu::CallbackResources,
    ) {
        let resources = callback_resources.get::<YuvRenderResources>().unwrap();
        if let Some(bind_group) = &resources.bind_group {
            render_pass.set_pipeline(&resources.pipeline);
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }
    }

    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &eframe::egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut eframe::egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources = callback_resources.get_mut::<YuvRenderResources>().unwrap();
        resources.upload_frame(device, queue, &self.frame, self.rotation);
        Vec::new()
    }
}

/// Shared resources for YUV rendering
pub struct YuvRenderResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    textures: Option<YuvTextures>,
    bind_group: Option<wgpu::BindGroup>,
    cached_dimensions: (u32, u32),
    cached_rotation: u32,
}

struct YuvTextures {
    y: wgpu::Texture,
    u: wgpu::Texture,
    v: wgpu::Texture,
    y_view: wgpu::TextureView,
    u_view: wgpu::TextureView,
    v_view: wgpu::TextureView,
}

impl YuvRenderResources {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("YUV Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("yuv_shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("YUV Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("YUV Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("YUV Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("YUV Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Rotation Uniform Buffer"),
            contents: bytemuck::cast_slice(&[0u32; 4]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            uniform_buffer,
            textures: None,
            bind_group: None,
            cached_dimensions: (0, 0),
            cached_rotation: u32::MAX,
        }
    }

    fn upload_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &Yu12Frame,
        rotation: u32,
    ) {
        let (width, height) = (frame.width, frame.height);
        let needs_rebuild = self.cached_dimensions != (width, height);

        if needs_rebuild {
            self.textures = Some(Self::create_textures(device, width, height));
            self.cached_dimensions = (width, height);
        }

        if self.cached_rotation != rotation {
            queue.write_buffer(
                &self.uniform_buffer,
                0,
                bytemuck::cast_slice(&[rotation, 0, 0, 0]),
            );
            self.cached_rotation = rotation;
        }

        if needs_rebuild {
            let textures = self.textures.as_ref().unwrap();
            self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("YUV Bind Group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&textures.y_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&textures.u_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&textures.v_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                ],
            }));
        }

        let textures = self.textures.as_ref().unwrap();
        let y_size = (width * height) as usize;
        let uv_width = width / 2;
        let uv_height = height / 2;
        let uv_size = (uv_width * uv_height) as usize;

        let y_data = &frame.data[..y_size];
        let u_data = &frame.data[y_size..y_size + uv_size];
        let v_data = &frame.data[y_size + uv_size..];

        Self::write_texture(queue, &textures.y, y_data, width, height);
        Self::write_texture(queue, &textures.u, u_data, uv_width, uv_height);
        Self::write_texture(queue, &textures.v, v_data, uv_width, uv_height);
    }

    fn write_texture(
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        data: &[u8],
        width: u32,
        height: u32,
    ) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn create_textures(device: &wgpu::Device, width: u32, height: u32) -> YuvTextures {
        let create_tex = |label, w, h| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };

        let y = create_tex("Y Texture", width, height);
        let u = create_tex("U Texture", width / 2, height / 2);
        let v = create_tex("V Texture", width / 2, height / 2);

        YuvTextures {
            y_view: y.create_view(&wgpu::TextureViewDescriptor::default()),
            u_view: u.create_view(&wgpu::TextureViewDescriptor::default()),
            v_view: v.create_view(&wgpu::TextureViewDescriptor::default()),
            y,
            u,
            v,
        }
    }
}

/// Main application state
pub struct VideoApp {
    frame_receiver: crossbeam_channel::Receiver<Arc<Yu12Frame>>,
    current_frame: Option<Arc<Yu12Frame>>,
    video_width: u32,
    video_height: u32,
    rotation: RotationState,
    last_rotation: u32,
    toolbar_open: bool,
}

impl VideoApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        frame_receiver: crossbeam_channel::Receiver<Arc<Yu12Frame>>,
        video_width: u32,
        video_height: u32,
    ) -> Self {
        if let Some(wgpu_state) = cc.wgpu_render_state.as_ref() {
            let resources = YuvRenderResources::new(&wgpu_state.device, wgpu_state.target_format);
            wgpu_state
                .renderer
                .write()
                .callback_resources
                .insert(resources);
        }

        Self {
            frame_receiver,
            current_frame: None,
            video_width,
            video_height,
            rotation: Arc::new(Mutex::new(0)),
            last_rotation: 0,
            toolbar_open: true,
        }
    }

    fn current_rotation(&self) -> u32 { *self.rotation.lock().unwrap() }

    fn effective_dimensions(&self) -> (u32, u32) {
        let rotation = self.current_rotation();
        if rotation & 1 == 0 {
            (self.video_width, self.video_height)
        } else {
            (self.video_height, self.video_width)
        }
    }
}

impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        // Receive latest frame
        while let Ok(frame) = self.frame_receiver.try_recv() {
            self.current_frame = Some(frame);
        }

        let rotation = self.current_rotation();

        // Check if rotation changed, resize window to match video
        if rotation != self.last_rotation {
            let (w, h) = self.effective_dimensions();
            ctx.send_viewport_cmd(eframe::egui::ViewportCommand::InnerSize(
                eframe::egui::vec2(w as f32, h as f32),
            ));
            self.last_rotation = rotation;
        }

        // Spawn independent toolbar window
        if self.toolbar_open {
            let rotation_state = Arc::clone(&self.rotation);
            ctx.show_viewport_deferred(
                eframe::egui::ViewportId::from_hash_of(TOOLBAR_VIEWPORT),
                eframe::egui::ViewportBuilder::default()
                    .with_title("Toolbar")
                    .with_inner_size([120.0, 40.0])
                    .with_resizable(false)
                    .with_always_on_top(),
                move |ctx, _class| {
                    eframe::egui::CentralPanel::default().show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("\u{21BB} Rotate").clicked() {
                                let mut r = rotation_state.lock().unwrap();
                                *r = (*r + 1) % 4;
                            }
                            let r = *rotation_state.lock().unwrap();
                            ui.label(format!("{}°", r * 90));
                        });
                    });
                },
            );
        }

        // Main video panel - fills entire window
        eframe::egui::CentralPanel::default()
            .frame(eframe::egui::Frame::NONE)
            .show(ctx, |ui| {
                let rect = ui.max_rect();

                if let Some(frame) = &self.current_frame {
                    let callback = eframe::egui_wgpu::Callback::new_paint_callback(
                        rect,
                        YuvRenderCallback {
                            frame: Arc::clone(frame),
                            rotation,
                        },
                    );
                    ui.painter().add(callback);
                } else {
                    ui.painter()
                        .rect_filled(rect, 0.0, eframe::egui::Color32::from_gray(32));
                    ui.painter().text(
                        rect.center(),
                        eframe::egui::Align2::CENTER_CENTER,
                        "Waiting for video...",
                        eframe::egui::FontId::proportional(24.0),
                        eframe::egui::Color32::GRAY,
                    );
                }
            });

        ctx.request_repaint();
    }
}
