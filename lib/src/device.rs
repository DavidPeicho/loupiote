use wgpu;

pub struct Device {
    device: wgpu::Device,
    default_texture_view: wgpu::TextureView,
    default_buffer: wgpu::Buffer,
    sampler_nearest: wgpu::Sampler,
    sampler_linear: wgpu::Sampler,
}

impl Device {
    pub fn new(device: wgpu::Device) -> Self {
        let default_texture_view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("null texture"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D1,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
            })
            .create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D1),
                ..wgpu::TextureViewDescriptor::default()
            });
        let default_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 0,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let sampler_nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let sampler_linear = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            device,
            default_texture_view,
            default_buffer,
            sampler_nearest,
            sampler_linear
        }
    }

    pub fn inner(&self) -> &wgpu::Device {
        &self.device
    }
    pub fn inner_mut(&mut self) -> &mut wgpu::Device {
        &mut self.device
    }
    pub fn default_texture_view(&self) -> &wgpu::TextureView {
        &self.default_texture_view
    }
    pub fn default_buffer(&self) -> &wgpu::Buffer {
        &self.default_buffer
    }
    pub fn sampler_nearest(&self) -> &wgpu::Sampler {
        &self.sampler_nearest
    }
    pub fn sampler_linear(&self) -> &wgpu::Sampler {
        &self.sampler_linear
    }
}
