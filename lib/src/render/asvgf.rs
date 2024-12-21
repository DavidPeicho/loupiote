use albedo_backend::{data::ShaderCache, gpu};
use albedo_rtx::{passes::{ATrousPass, CompositingPass, TemporalAccumulationPass}, Ray};

use crate::Device;

pub struct PingPongResources {
    pub radiance_img: wgpu::Texture,
    pub radiance: wgpu::TextureView,
    pub gbuffer: wgpu::TextureView,
    pub moments: wgpu::TextureView,
    pub history: gpu::Buffer<u32>,
}

impl PingPongResources {
    pub fn new(device: &Device, size: &(u32, u32), index: usize) -> Self {
        let radiance_img: wgpu::Texture = {
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
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
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
                format: wgpu::TextureFormat::Rgba32Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            })
        };

        let moments = device.create_texture(&wgpu::TextureDescriptor {
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
            radiance: radiance_img.create_view(&wgpu::TextureViewDescriptor::default()),
            radiance_img,
            gbuffer: gbuffer.create_view(&wgpu::TextureViewDescriptor::default()),
            moments: moments.create_view(&wgpu::TextureViewDescriptor::default()),
            history
        }
    }
}

pub struct ScreenResources {
    pub pingpong: Vec<PingPongResources>,
    pub motion: wgpu::TextureView,
    pub radiance_img_temp: wgpu::Texture,
    pub radiance_temp: wgpu::TextureView,
}

impl ScreenResources {
    pub fn new(device: &Device, size: &(u32, u32)) -> Self {
        let motion_vectors: wgpu::Texture = device.create_texture(&wgpu::TextureDescriptor {
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

        let pingpong = {
            let mut vec: Vec<_> = Vec::with_capacity(2);
            vec.push(PingPongResources::new(device, size, 0));
            vec.push(PingPongResources::new(device, size, 1));
            vec
        };

        let radiance_img_temp: wgpu::Texture = {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&"Temporary Radiance Render Target"),
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
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };

        Self {
            pingpong,
            radiance_temp: radiance_img_temp.create_view(&wgpu::TextureViewDescriptor::default()),
            radiance_img_temp,
            motion: motion_vectors.create_view(&wgpu::TextureViewDescriptor::default()),
        }
    }
}

pub struct ASVGFPasses {
    pub temporal: albedo_rtx::passes::TemporalAccumulationPass,
    pub atrous: albedo_rtx::passes::ATrousPass,
    pub composit: albedo_rtx::passes::CompositingPass,
}

pub(crate) struct ASVGF {
    pub resources: ScreenResources,

    pub passes: ASVGFPasses,
    pub temporal_bindgroup: Vec<wgpu::BindGroup>,
    pub atrous_bindgroup: Vec<[wgpu::BindGroup; 2]>,

    current_frame_back: bool,
}

impl ASVGF {
    pub fn new(device: &Device, spp: &ShaderCache, size: &(u32, u32), out: &wgpu::TextureView, rays: &gpu::Buffer<Ray>) -> Self {
        let resources = ScreenResources::new(device, size);

        let passes = ASVGFPasses {
            temporal: TemporalAccumulationPass::new(device, spp, None),
            atrous: ATrousPass::new(device, spp, None),
            composit: CompositingPass::new_inlined(device, spp)
        };

        let temporal_bindgroup = resources.pingpong.iter().enumerate().map(|(i, res)| {
            let previous = &resources.pingpong[1 - i];
            passes.temporal.create_frame_bind_groups(device, &res.radiance, &res.moments, &res.history, &rays, &previous.gbuffer, &res.gbuffer, &resources.motion, &previous.radiance, device.sampler_nearest(), &previous.history, &previous.moments)
        }).collect();

        let atrous_bindgroup = resources.pingpong.iter().enumerate().map(|(i, res)| {
            // @todo: Use temporary radiance to not overwrite temporally accumulated one
            passes.atrous.create_frame_bind_groups(device, out, &res.gbuffer, &resources.radiance_temp, device.sampler_nearest())
        }).collect();

        Self {
            resources,

            temporal_bindgroup,
            atrous_bindgroup,

            passes,
            current_frame_back: true
        }
    }

    pub fn start(&mut self) {
        self.current_frame_back = !self.current_frame_back;
    }

    pub fn temporal_pass(&mut self, encoder: &mut wgpu::CommandEncoder, dispatch_size: &(u32, u32, u32)) {
        self.passes.temporal.dispatch(encoder, self.curr_temporal_bindgroup(), dispatch_size);
    }

    pub fn render(&mut self, encoder: &mut wgpu::CommandEncoder, dispatch_size: &(u32, u32, u32), out_texture: &wgpu::Texture) {
        self.temporal_pass(encoder, dispatch_size);

        // Copy current frame radiance into temporary input for atrous
        let curr_radiance: &wgpu::Texture = &self.resources.pingpong[self.current_frame_back as usize].radiance_img;
        encoder.copy_texture_to_texture(wgpu::ImageCopyTexture {
            texture: &curr_radiance,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },wgpu::ImageCopyTexture {
            texture: &self.resources.radiance_img_temp,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        }, self.resources.radiance_img_temp.size());

        let atrou_bindgroups = self.curr_atrou_bindgroup();
        self.passes.atrous.dispatch(encoder, atrou_bindgroups, &out_texture, &curr_radiance, dispatch_size);

        // A-Trous uses an even number of calls, so result is in `radiance_img_temp`
        // Thus, re-using the first a-trou bind group is fine
        self.passes.composit.dispatch(encoder, &atrou_bindgroups[0], dispatch_size);
    }

    pub fn reload_shaders(&mut self, device: &wgpu::Device, spp: &ShaderCache) {
        match CompositingPass::new(device, spp) {
            Ok(pass) => self.passes.composit = pass,
            Err(e) => println!("Failed to reload compositing pass (compositing.comp), reason:\n{:?}", e)
        };
    }

    pub fn curr_frame(&self) -> usize {
        self.current_frame_back as usize
    }

    fn curr_temporal_bindgroup(&self) -> &wgpu::BindGroup {
        &self.temporal_bindgroup[self.current_frame_back as usize]
    }
    fn curr_atrou_bindgroup(&self) -> &[wgpu::BindGroup; 2] {
        &self.atrous_bindgroup[self.current_frame_back as usize]
    }
}
