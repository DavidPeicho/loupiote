use albedo_backend::gpu;
use albedo_rtx::{passes::{GBufferPass, TemporalAccumulationPass}, uniforms, Intersection, RTGeometryBindGroupLayout, Ray};
use wgpu;

use crate::Device;

pub struct ScreenSpaceTextures {
    pub radiance: wgpu::TextureView,
    pub gbuffer: wgpu::TextureView,
    pub motion: wgpu::TextureView
}

impl ScreenSpaceTextures {
    pub fn new(device: &Device, size: &(u32, u32), index: usize) -> Self {
        let radiance: wgpu::Texture = {
            let label = format!("Radiance Render Target {}", index);
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&label),
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
                    | wgpu::TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            })
        };
        let gbuffer: wgpu::Texture = {
            let label = format!("GBuffer Render Target {}", index);
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&label),
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
                    | wgpu::TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            })
        };
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
            format: wgpu::TextureFormat::Rg32Float, // @todo: Use F16
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });

        Self {
            radiance: radiance.create_view(&wgpu::TextureViewDescriptor::default()),
            gbuffer: gbuffer.create_view(&wgpu::TextureViewDescriptor::default()),
            motion: motion_vectors.create_view(&wgpu::TextureViewDescriptor::default())
        }
    }
}

pub struct ASVGFPasses {
    pub gbuffer: albedo_rtx::passes::GBufferPass,
    pub temporal: albedo_rtx::passes::TemporalAccumulationPass,
}

pub(crate) struct ASVFG {
    pub textures: Vec<ScreenSpaceTextures>,
    pub passes: ASVGFPasses,
    pub gbuffer_bindgroup: Vec<wgpu::BindGroup>,
    pub temporal_bindgroup: Vec<wgpu::BindGroup>,

    current_frame_back: bool,

    prev_model_to_screen: glam::Mat4
}

impl ASVFG {
    pub fn new(device: &Device, size: &(u32, u32), geometry_layout: &RTGeometryBindGroupLayout, intersections: &gpu::Buffer<Intersection>, rays: &gpu::Buffer<Ray>) -> Self {
        let passes = ASVGFPasses {
            gbuffer: GBufferPass::new(device, geometry_layout, None),
            temporal: TemporalAccumulationPass::new(device, None)
        };
        let textures = {
            let mut vec = Vec::with_capacity(2);
            vec.push(ScreenSpaceTextures::new(device, size, 0));
            vec.push(ScreenSpaceTextures::new(device, size, 1));
            vec
        };

        let gbuffer_bindgroup = textures.iter().map(|texture| {
            passes.gbuffer.create_frame_bind_groups(device, size, &texture.gbuffer, &texture.motion, intersections)
        }).collect();

        let temporal_bindgroup = textures.iter().enumerate().map(|(i, texture)| {
            let previous = &textures[1 - i];
            passes.temporal.create_frame_bind_groups(device, size, &texture.radiance, &rays, &previous.gbuffer, &texture.gbuffer, &texture.motion, &previous.radiance, device.sampler_nearest())
        }).collect();

        ASVFG {
            gbuffer_bindgroup,
            temporal_bindgroup,
            textures,
            passes,
            current_frame_back: true,
            prev_model_to_screen: glam::Mat4::IDENTITY
        }
    }

    pub fn start(&mut self) {
        self.current_frame_back = !self.current_frame_back;
    }

    pub fn gbuffer_pass(&mut self, encoder: &mut wgpu::CommandEncoder, geometry_bindgroup: &wgpu::BindGroup, dispatch_size: &(u32, u32, u32)) {
        self.passes.gbuffer.dispatch(encoder, &geometry_bindgroup,self.curr_gbuffer_bindgroup(), dispatch_size, &self.prev_model_to_screen);
    }

    pub fn render(&mut self, encoder: &mut wgpu::CommandEncoder, geometry_bindgroup: &wgpu::BindGroup, dispatch_size: &(u32, u32, u32)) {
        self.gbuffer_pass(encoder, geometry_bindgroup, dispatch_size);
        self.passes.temporal.dispatch(encoder, self.curr_temporal_bindgroup(), dispatch_size);
    }

    pub fn end(&mut self, camera: &uniforms::Camera, dispatch_size: &(u32, u32, u32)) {
        self.prev_model_to_screen = {
            // todo: Use matrix at an early stage.
            let aspect = dispatch_size.0 as f32 / dispatch_size.1 as f32;
            let perspective = glam::Mat4::perspective_infinite_lh(camera.v_fov, aspect, 0.01);

            let view = {
                let dir = camera.up.cross(camera.right).normalize().extend(0.0);
                let rot = glam::Mat4::from_cols(camera.right.normalize().extend(0.0), camera.up.normalize().extend(0.0), dir, glam::Vec4::W);
                glam::Mat4::from_translation(camera.origin) * rot
            };
            let inv_view = view.inverse();
            perspective * inv_view
        };
    }

    pub fn resize(&mut self, device: &Device, size: &(u32, u32), intersections: &gpu::Buffer<Intersection>, rays: &gpu::Buffer<Ray>) {
        self.textures = {
            let mut vec = Vec::with_capacity(2);
            vec.push(ScreenSpaceTextures::new(device, size, 0));
            vec.push(ScreenSpaceTextures::new(device, size, 1));
            vec
        };
        self.gbuffer_bindgroup = self.textures.iter().map(|texture| {
            self.passes.gbuffer.create_frame_bind_groups(device, size, &texture.gbuffer, &texture.motion, intersections)
        }).collect();
        self.temporal_bindgroup = self.textures.iter().enumerate().map(|(i, texture)| {
            let previous = &self.textures[1 - i];
            self.passes.temporal.create_frame_bind_groups(device, size, &texture.radiance, &rays, &previous.gbuffer, &texture.gbuffer, &texture.motion, &previous.radiance, device.sampler_nearest())
        }).collect();
    }

    fn curr_gbuffer_bindgroup(&self) -> &wgpu::BindGroup {
        &self.gbuffer_bindgroup[self.current_frame_back as usize]
    }
    fn curr_temporal_bindgroup(&self) -> &wgpu::BindGroup {
        &self.temporal_bindgroup[self.current_frame_back as usize]
    }
}
