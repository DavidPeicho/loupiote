use albedo_backend::gpu;
use albedo_rtx::{passes::GBufferPass, Intersection, RTGeometryBindGroupLayout};
use wgpu;

use crate::Device;

pub struct ScreenSpaceTextures {
    pub gbuffer: wgpu::TextureView,
    pub motion: wgpu::TextureView
}

impl ScreenSpaceTextures {
    pub fn new(device: &Device, size: &(u32, u32)) -> Self {
        let gbuffer: wgpu::Texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GBuffer Render Target"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let motion_vectors = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Motion Vectors Texture"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        Self {
            gbuffer: gbuffer.create_view(&wgpu::TextureViewDescriptor::default()),
            motion: motion_vectors.create_view(&wgpu::TextureViewDescriptor::default())
        }
    }
}

pub struct ASVGFPasses {
    pub gbuffer: albedo_rtx::passes::GBufferPass
}

pub(crate) struct ASVFG {
    // history: wgpu::Buffer,
    // Previous frame gbuffer.
    // gbuffer: wgpu::TextureView,
    pub textures: ScreenSpaceTextures,
    pub gbuffer_bindgroup: wgpu::BindGroup,
    pub passes: ASVGFPasses,
}

impl ASVFG {
    pub fn new(device: &Device, size: &(u32, u32), geometry_layout: &RTGeometryBindGroupLayout, intersections: &gpu::Buffer<Intersection>) -> Self {
        let passes = ASVGFPasses {
            gbuffer: GBufferPass::new(device, geometry_layout, None),
        };
        let textures = ScreenSpaceTextures::new(device, size);

        ASVFG {
            gbuffer_bindgroup: passes.gbuffer.create_frame_bind_groups(device, size, &textures.gbuffer, &textures.motion, intersections),
            textures,
            passes
        }
    }

    pub fn resize(&mut self, device: &Device, size: &(u32, u32), intersections: &gpu::Buffer<Intersection>) {
        self.textures = ScreenSpaceTextures::new(device, size);

        // Re-create bind groups based on screen resources
        self.gbuffer_bindgroup = self.passes.gbuffer.create_frame_bind_groups(device, size, &self.textures.gbuffer, &self.textures.motion, intersections);
    }
}
