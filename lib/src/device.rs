use std::ops::Deref;

use wgpu;

pub struct DefaultTextures {
    filterable_2d: wgpu::TextureView,
    filterable_2darray: wgpu::TextureView,
    non_filterable_1d: wgpu::TextureView,
}

impl DefaultTextures {
    pub fn new(device: &wgpu::Device) -> Self {
        let filterable_2d = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Default Filterable 2D Array Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let non_filterable_1d = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Default Non-Filterable Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D1,
            format: wgpu::TextureFormat::R8Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        Self {
            filterable_2d: filterable_2d.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                ..wgpu::TextureViewDescriptor::default()
            }),
            filterable_2darray: filterable_2d.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..wgpu::TextureViewDescriptor::default()
            }),
            non_filterable_1d: non_filterable_1d.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D1),
                ..wgpu::TextureViewDescriptor::default()
            }),
        }
    }
}

impl DefaultTextures {
    pub fn filterable_2d(&self) -> &wgpu::TextureView {
        &self.filterable_2d
    }
    pub fn filterable_2darray(&self) -> &wgpu::TextureView {
        &self.filterable_2darray
    }
    pub fn non_filterable_1d(&self) -> &wgpu::TextureView {
        &self.non_filterable_1d
    }
}

pub struct Device {
    inner: wgpu::Device,
    default_textures: DefaultTextures,
    default_buffer: wgpu::Buffer,
    sampler_nearest: wgpu::Sampler,
    sampler_linear: wgpu::Sampler,
}

impl Device {
    pub fn new(device: wgpu::Device) -> Self {
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
        let default_textures = DefaultTextures::new(&device);
        Self {
            inner: device,
            default_textures,
            default_buffer,
            sampler_nearest,
            sampler_linear,
        }
    }

    pub fn inner(&self) -> &wgpu::Device {
        &self.inner
    }
    pub fn inner_mut(&mut self) -> &mut wgpu::Device {
        &mut self.inner
    }
    pub fn default_textures(&self) -> &DefaultTextures {
        &self.default_textures
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

impl Deref for Device {
    type Target = wgpu::Device;

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}
