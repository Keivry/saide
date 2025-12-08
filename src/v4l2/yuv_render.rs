use {
    super::v4l2_capture::Yu12Frame,
    eframe::{
        egui,
        egui_wgpu::{self, CallbackTrait},
    },
    std::sync::Arc,
    wgpu::util::DeviceExt,
};

/// Custom wgpu render callback for YUV frame
pub(crate) struct YuvRenderCallback {
    frame: Arc<Yu12Frame>,

    // Rotation in degrees (0, 90, 180, 270)
    rotation: u32,
}

impl CallbackTrait for YuvRenderCallback {
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
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
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
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
    combined: wgpu::Texture,
    combined_view: wgpu::TextureView,
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
                // Binding 0: Combined YUV texture
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
                // Binding 1: Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Binding 2: Rotation uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
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
                        resource: wgpu::BindingResource::TextureView(&textures.combined_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
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

        // Upload Y plane to rows [0, height)
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &textures.combined,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            y_data,
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

        // Upload V plane to rows [height, height + height/2)
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &textures.combined,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: height,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            v_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(uv_width),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: uv_width,
                height: uv_height,
                depth_or_array_layers: 1,
            },
        );

        // Upload U plane to rows [height + height/2, height*2)
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &textures.combined,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: height + uv_height,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            u_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(uv_width),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: uv_width,
                height: uv_height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn create_textures(device: &wgpu::Device, width: u32, height: u32) -> YuvTextures {
        // Combined texture: Y (height) + V (height/2) + U (height/2) = height * 2
        let combined_height = height * 2;

        let combined = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("YUV Combined Texture"),
            size: wgpu::Extent3d {
                width,
                height: combined_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        YuvTextures {
            combined_view: combined.create_view(&wgpu::TextureViewDescriptor::default()),
            combined,
        }
    }
}

pub fn new_yuv_render_callback(frame: Arc<Yu12Frame>, rotation: u32) -> YuvRenderCallback {
    YuvRenderCallback { frame, rotation }
}
