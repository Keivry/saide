use {
    super::DecodedFrame,
    eframe::{
        egui,
        egui_wgpu::{self, CallbackTrait},
    },
    std::sync::Arc,
    tracing::debug,
};

/// Custom wgpu render callback for NV12 frame
pub struct Nv12RenderCallback {
    frame: Arc<DecodedFrame>,
}

impl CallbackTrait for Nv12RenderCallback {
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let resources = callback_resources.get::<Nv12RenderResources>().unwrap();
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
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources = callback_resources.get_mut::<Nv12RenderResources>().unwrap();
        resources.upload_frame(device, queue, &self.frame);
        Vec::new()
    }
}

/// Shared resources for NV12 rendering
pub struct Nv12RenderResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    y_texture: Option<wgpu::Texture>,
    uv_texture: Option<wgpu::Texture>,
    y_texture_view: Option<wgpu::TextureView>,
    uv_texture_view: Option<wgpu::TextureView>,
    bind_group: Option<wgpu::BindGroup>,
    cached_dimensions: (u32, u32),
}

impl Nv12RenderResources {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("NV12 Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("nv12_shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("NV12 Bind Group Layout"),
            entries: &[
                // Y texture
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
                // UV texture
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
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("NV12 Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("NV12 Render Pipeline"),
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
            label: Some("NV12 Sampler"),
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
            y_texture: None,
            uv_texture: None,
            y_texture_view: None,
            uv_texture_view: None,
            bind_group: None,
            cached_dimensions: (0, 0),
        }
    }

    fn upload_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &DecodedFrame) {
        let (width, height) = (frame.width, frame.height);
        let needs_rebuild = self.cached_dimensions != (width, height);

        if needs_rebuild {
            // Create Y texture (R8Unorm, full resolution)
            let y_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("NV12 Y Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            // Create UV texture (Rg8Unorm, half resolution)
            let uv_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("NV12 UV Texture"),
                size: wgpu::Extent3d {
                    width: width / 2,
                    height: height / 2,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rg8Unorm,  // RG for interleaved UV
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            let y_texture_view = y_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let uv_texture_view = uv_texture.create_view(&wgpu::TextureViewDescriptor::default());

            self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("NV12 Bind Group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&y_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&uv_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            }));

            self.y_texture = Some(y_texture);
            self.uv_texture = Some(uv_texture);
            self.y_texture_view = Some(y_texture_view);
            self.uv_texture_view = Some(uv_texture_view);
            self.cached_dimensions = (width, height);
        }

        // Upload NV12 data
        let y_size = (width * height) as usize;
        let uv_size = y_size / 2;

        if frame.data.len() < y_size + uv_size {
            eprintln!("WARN: Invalid NV12 data size: expected {}, got {}", 
                y_size + uv_size, frame.data.len());
            return;
        }

        // Debug: Print data info on first upload
        if needs_rebuild {
            debug!("NV12 texture upload: {}x{} (Y: {}x{}, UV: {}x{})",
                width, height, width, height, width/2, height/2);
        }

        // Upload Y plane
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: self.y_texture.as_ref().unwrap(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.data[..y_size],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        // Upload UV plane (interleaved, perfect for Rg8Unorm)
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: self.uv_texture.as_ref().unwrap(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.data[y_size..y_size + uv_size],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width), // UV interleaved, 2 bytes per UV pair
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: width / 2,
                height: height / 2,
                depth_or_array_layers: 1,
            },
        );
    }
}

pub fn new_nv12_render_callback(frame: Arc<DecodedFrame>) -> Nv12RenderCallback {
    Nv12RenderCallback { frame }
}
