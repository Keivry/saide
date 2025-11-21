use {crate::v4l2_capture::Yu12Frame, eframe::egui_wgpu::CallbackTrait, std::sync::Arc};

/// Custom wgpu render callback for YUV frame
struct YuvRenderCallback {
    frame: Arc<Yu12Frame>,
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
        resources.upload_frame(device, queue, &self.frame);
        Vec::new()
    }
}

/// Shared resources for YUV rendering (stored in egui_wgpu callback resources)
pub struct YuvRenderResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: Option<YuvTextures>,
    bind_group: Option<wgpu::BindGroup>,
    cached_dimensions: (u32, u32),
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

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            textures: None,
            bind_group: None,
            cached_dimensions: (0, 0),
        }
    }

    fn upload_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &Yu12Frame) {
        let (width, height) = (frame.width, frame.height);

        if self.cached_dimensions != (width, height) {
            self.textures = Some(Self::create_textures(device, width, height));
            self.cached_dimensions = (width, height);

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
}

impl VideoApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        frame_receiver: crossbeam_channel::Receiver<Arc<Yu12Frame>>,
        video_width: u32,
        video_height: u32,
    ) -> Self {
        // Initialize wgpu render resources
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
        }
    }
}

impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        // Receive latest frame (drain channel to get most recent)
        while let Ok(frame) = self.frame_receiver.try_recv() {
            self.current_frame = Some(frame);
        }

        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            let available_size = ui.available_size();

            // Calculate aspect-correct size
            let aspect = self.video_width as f32 / self.video_height as f32;
            let (render_width, render_height) = if available_size.x / available_size.y > aspect {
                (available_size.y * aspect, available_size.y)
            } else {
                (available_size.x, available_size.x / aspect)
            };

            // Center the video
            let offset_x = (available_size.x - render_width) / 2.0;
            let offset_y = (available_size.y - render_height) / 2.0;

            let rect = eframe::egui::Rect::from_min_size(
                ui.min_rect().min + eframe::egui::vec2(offset_x, offset_y),
                eframe::egui::vec2(render_width, render_height),
            );

            if let Some(frame) = &self.current_frame {
                let callback = eframe::egui_wgpu::Callback::new_paint_callback(
                    rect,
                    YuvRenderCallback {
                        frame: Arc::clone(frame),
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

        // Request continuous repaints for video
        ctx.request_repaint();
    }
}
