use albedo_backend::gpu;
use albedo_rtx::{passes::{GBufferPass, TemporalAccumulationPass}, uniforms, Intersection, RTGeometryBindGroupLayout, Ray};
use wgpu;

use crate::Device;

pub struct ScreenResources {
    pub radiance: wgpu::TextureView,
    pub gbuffer: wgpu::TextureView,
    pub motion: wgpu::TextureView,
    pub history: gpu::Buffer<u32>
}

impl ScreenResources {
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
        let pixel_count = size.0 * size.1;

        let history = {
            let label = format!("History Buffer {}", index);
            gpu::Buffer::new_storage(device, pixel_count as u64, Some(gpu::BufferInitDescriptor {
                label: Some(&label),
                usage: wgpu::BufferUsages::STORAGE
            }))
        };

        Self {
            radiance: radiance.create_view(&wgpu::TextureViewDescriptor::default()),
            gbuffer: gbuffer.create_view(&wgpu::TextureViewDescriptor::default()),
            motion: motion_vectors.create_view(&wgpu::TextureViewDescriptor::default()),
            history
        }
    }
}

pub struct ASVGFPasses {
    pub gbuffer: albedo_rtx::passes::GBufferPass,
    pub temporal: albedo_rtx::passes::TemporalAccumulationPass,
}

pub(crate) struct ASVFG {
    pub resources: Vec<ScreenResources>,

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
        let resources = {
            let mut vec: Vec<_> = Vec::with_capacity(2);
            vec.push(ScreenResources::new(device, size, 0));
            vec.push(ScreenResources::new(device, size, 1));
            vec
        };

        let gbuffer_bindgroup = resources.iter().map(|res| {
            passes.gbuffer.create_frame_bind_groups(device, size, &res.gbuffer, &res.motion, intersections)
        }).collect();

        let temporal_bindgroup = resources.iter().enumerate().map(|(i, res)| {
            let previous = &resources[1 - i];
            passes.temporal.create_frame_bind_groups(device, size, &res.radiance, &res.history, &rays, &previous.gbuffer, &res.gbuffer, &res.motion, &previous.radiance, device.sampler_nearest(), &previous.history)
        }).collect();

        ASVFG {
            resources,

            gbuffer_bindgroup,
            temporal_bindgroup,
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

    pub fn render(&mut self, encoder: &mut wgpu::CommandEncoder, dispatch_size: &(u32, u32, u32)) {
        self.passes.temporal.dispatch(encoder, self.curr_temporal_bindgroup(), dispatch_size);
    }

    pub fn end(&mut self, camera: &uniforms::Camera, dispatch_size: &(u32, u32, u32)) {
        self.prev_model_to_screen = {
            // todo: Use matrix at an early stage.
            let aspect = dispatch_size.0 as f32 / dispatch_size.1 as f32;
            let perspective = glam::Mat4::perspective_lh(camera.v_fov, aspect, 0.01, 100.0);

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
        self.resources = {
            let mut vec = Vec::with_capacity(2);
            vec.push(ScreenResources::new(device, size, 0));
            vec.push(ScreenResources::new(device, size, 1));
            vec
        };
        self.gbuffer_bindgroup = self.resources.iter().map(|res| {
            self.passes.gbuffer.create_frame_bind_groups(device, size, &res.gbuffer, &res.motion, intersections)
        }).collect();
        self.temporal_bindgroup = self.resources.iter().enumerate().map(|(i, res)| {
            let previous = &self.resources[1 - i];
            self.passes.temporal.create_frame_bind_groups(device, size, &res.radiance, &res.history, &rays, &previous.gbuffer, &res.gbuffer, &res.motion, &previous.radiance, device.sampler_nearest(), &previous.history)
        }).collect();
    }

    fn curr_gbuffer_bindgroup(&self) -> &wgpu::BindGroup {
        &self.gbuffer_bindgroup[self.current_frame_back as usize]
    }
    fn curr_temporal_bindgroup(&self) -> &wgpu::BindGroup {
        &self.temporal_bindgroup[self.current_frame_back as usize]
    }
}
