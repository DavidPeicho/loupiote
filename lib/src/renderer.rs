use std::fmt::Debug;

use albedo_backend::data::ShaderCache;
use albedo_backend::gpu::{self, QueriesOptions};

use albedo_rtx::passes::{PrimaryRayPass, ShadingPass};
use albedo_rtx::uniforms::{Camera, Intersection, PerDrawUniforms, Ray, Uniform};
use albedo_rtx::{passes, DenoiseResources, RadianceParameters, RaytraceResources};
use glam::Mat4;
use wgpu::naga::FastHashMap;

use crate::device::Device;
use crate::errors::Error;
use crate::render::ASVGF;
use crate::scene::SceneGPU;
use crate::ProbeGPU;

fn get_downsampled_size(size: &(u32, u32), factor: f32) -> (u32, u32) {
    let w = size.0 as f32;
    let h = size.1 as f32;
    ((w * factor) as u32, (h * factor) as u32)
}

struct RenderTargets {
    main: wgpu::TextureView,
    main_texture: wgpu::Texture,
    second: wgpu::TextureView,
}

impl RenderTargets {
    pub fn new(device: &Device, size: (u32, u32)) -> Self {
        let main_texture: wgpu::Texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Main Render Target"),
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
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let render_target2 = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Second Render Target"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        Self {
            main: main_texture.create_view(&wgpu::TextureViewDescriptor::default()),
            main_texture,
            second: render_target2.create_view(&wgpu::TextureViewDescriptor::default()),
        }
    }
}

struct BindGroups {
    generate_ray_pass: wgpu::BindGroup,
    intersection_pass: wgpu::BindGroup,
    shading_pass: wgpu::BindGroup,
    primary_rays: [wgpu::BindGroup; 2],
    accumulate_pass: wgpu::BindGroup,
    accumulate_pass2: wgpu::BindGroup,
    blit_pass: wgpu::BindGroup,
    blit_pass2: wgpu::BindGroup,
}

impl BindGroups {
    fn new(
        device: &Device,
        render_targets: &RenderTargets,
        resources: &RaytraceResources,
        denoise_res: &DenoiseResources,
        ray_pass_desc: &passes::RayPass,
        intersector_pass_desc: &passes::IntersectorPass,
        shading_pass_desc: &passes::ShadingPass,
        primary_rays_pass_desc: &passes::PrimaryRayPass,
        accumulation_pass_desc: &passes::AccumulationPass,
        blit_pass: &passes::BlitPass,
    ) -> Self {
        let denoise_pong = denoise_res.pong();

        BindGroups {
            generate_ray_pass: ray_pass_desc.create_frame_bind_groups(
                device,
                resources.rays,
                resources.camera_uniforms,
            ),
            intersection_pass: intersector_pass_desc.create_frame_bind_groups(
                device,
                resources.intersections,
                resources.rays,
            ),
            shading_pass: shading_pass_desc.bgl.as_bind_group(device, resources, None),
            primary_rays: [
                primary_rays_pass_desc
                    .bgl
                    .as_bind_group(device, resources, Some(denoise_res)),
                primary_rays_pass_desc
                    .bgl
                    .as_bind_group(device, resources, Some(&denoise_pong)),
            ],
            accumulate_pass: accumulation_pass_desc.create_frame_bind_groups(
                device,
                resources.rays,
                resources.global_uniforms,
                &render_targets.main,
                &render_targets.second,
                &device.sampler_nearest(),
            ),
            accumulate_pass2: accumulation_pass_desc.create_frame_bind_groups(
                device,
                resources.rays,
                resources.global_uniforms,
                &render_targets.second,
                &render_targets.main,
                &device.sampler_nearest(),
            ),
            blit_pass: blit_pass.create_frame_bind_groups(
                device,
                &render_targets.main,
                &device.sampler_nearest(),
                resources.global_uniforms,
            ),
            blit_pass2: blit_pass.create_frame_bind_groups(
                device,
                &render_targets.second,
                &device.sampler_nearest(),
                resources.global_uniforms,
            ),
        }
    }
}

pub struct Passes {
    pub rays: passes::RayPass,
    pub intersection: passes::IntersectorPass,
    pub shading: passes::ShadingPass,
    pub primary_rays: passes::PrimaryRayPass,
    pub accumulation: passes::AccumulationPass,
    pub blit: passes::BlitPass,
    pub lightmap: passes::LightmapPass,
    pub blit_texture: passes::BlitTexturePass,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlitMode {
    Pahtrace,
    DenoisedPathrace,
    Temporal,
    GBuffer,
    MotionVector,
}

pub struct Renderer {
    render_targets: RenderTargets,

    ray_buffer: gpu::Buffer<Ray>,
    intersection_buffer: gpu::Buffer<Intersection>,

    camera_uniforms: gpu::Buffer<Camera>,
    global_uniforms: PerDrawUniforms,
    global_uniforms_buffer: gpu::Buffer<PerDrawUniforms>,
    radiance_parameters_buffer: gpu::Buffer<RadianceParameters>,

    pub shaders: ShaderCache,
    pub passes: Passes,

    asvgf: Option<ASVGF>,

    geometry_bindgroup_layout: albedo_rtx::RTGeometryBindGroupLayout,
    surface_bindgroup_layout: albedo_rtx::RTSurfaceBindGroupLayout,
    geometry_bindgroup: Option<wgpu::BindGroup>,
    surface_bindgroup: Option<wgpu::BindGroup>,

    frame_bindgroups: Option<BindGroups>,
    debug_blit_bindgroup: Vec<wgpu::BindGroup>,

    // Textures
    texture_blue_noise: Option<wgpu::TextureView>,

    size: (u32, u32),

    mode: BlitMode,
    frame_back: bool,

    prev_model_to_screen: glam::Mat4,

    pub downsample_factor: f32,
    pub accumulate: bool,
    pub queries: gpu::Queries,
}

impl Renderer {
    pub fn max_ssbo_element_in_bytes() -> u32 {
        [
            Ray::size_in_bytes(),
            Intersection::size_in_bytes(),
            Camera::size_in_bytes(),
            PerDrawUniforms::size_in_bytes(),
        ]
        .iter()
        .fold(0, |max, &val| std::cmp::max(max, val))
    }

    pub fn new(
        device: &Device,
        original_size: (u32, u32),
        swapchain_format: wgpu::TextureFormat,
    ) -> Self {
        let downsample_factor = 0.5;
        let size = get_downsampled_size(&original_size, downsample_factor);
        let pixel_count: u64 = size.0 as u64 * size.1 as u64;

        let geometry_bindgroup_layout = albedo_rtx::RTGeometryBindGroupLayout::new(device);
        let surface_bindgroup_layout = albedo_rtx::RTSurfaceBindGroupLayout::new(device);
        let render_targets = RenderTargets::new(device, size);

        let intersection_buffer = gpu::Buffer::new_storage(device, pixel_count, None);
        let ray_buffer: gpu::Buffer<Ray> = gpu::Buffer::new_storage(
            &device,
            pixel_count as u64,
            Some(gpu::BufferInitDescriptor {
                label: Some("Ray Buffer"),
                usage: wgpu::BufferUsages::STORAGE,
            }),
        );

        let mut shaders = ShaderCache::new();
        shaders.add_embedded::<albedo_rtx::shaders::AlbedoRtxShaderImports>();

        let empty_defines = FastHashMap::default();
        let asvgf = Some(ASVGF::new(
            device,
            &shaders,
            &size,
            &render_targets.main,
            &ray_buffer,
        ));

        let passes = Passes {
            rays: passes::RayPass::new(device, &shaders, None),
            intersection: passes::IntersectorPass::new(
                device,
                &shaders,
                &geometry_bindgroup_layout,
                None,
            ),
            shading: passes::ShadingPass::new_inlined(
                device,
                &shaders,
                &empty_defines,
                &geometry_bindgroup_layout,
                &surface_bindgroup_layout,
            ),
            primary_rays: passes::PrimaryRayPass::new_inlined(
                device,
                &shaders,
                &geometry_bindgroup_layout,
                &surface_bindgroup_layout,
            ),
            accumulation: passes::AccumulationPass::new(device, &shaders, None),
            blit: passes::BlitPass::new(device, &shaders, swapchain_format),
            lightmap: passes::LightmapPass::new(device, &shaders, swapchain_format),
            blit_texture: passes::BlitTexturePass::new(device, &shaders, swapchain_format),
        };

        Self {
            render_targets,

            camera_uniforms: gpu::Buffer::new_uniform(device, 1, None),
            global_uniforms: PerDrawUniforms {
                frame_count: 1,
                seed: 0,
                ..Default::default()
            },

            ray_buffer,
            intersection_buffer,

            asvgf,

            global_uniforms_buffer: gpu::Buffer::new_uniform(device, 1, None),
            radiance_parameters_buffer: gpu::Buffer::new_uniform(device, 1, None),

            shaders,
            passes,

            geometry_bindgroup_layout,
            surface_bindgroup_layout,
            geometry_bindgroup: None,
            surface_bindgroup: None,

            frame_bindgroups: None,
            debug_blit_bindgroup: Vec::new(),

            texture_blue_noise: None,

            size,
            downsample_factor,

            frame_back: true,
            mode: BlitMode::Pahtrace,

            prev_model_to_screen: glam::Mat4::IDENTITY,

            queries: gpu::Queries::new(device, QueriesOptions::new(10)),
            accumulate: false,
        }
    }

    pub fn resize(
        &mut self,
        device: &Device,
        scene_resources: &SceneGPU,
        probe: Option<&ProbeGPU>,
        size: (u32, u32),
    ) {
        self.size = get_downsampled_size(&size, self.downsample_factor);

        let pixel_count: u64 = self.size.0 as u64 * self.size.1 as u64;
        self.ray_buffer = gpu::Buffer::new_storage(
            &device,
            pixel_count as u64,
            Some(gpu::BufferInitDescriptor {
                label: Some("Ray Buffer"),
                usage: wgpu::BufferUsages::STORAGE,
            }),
        );
        self.intersection_buffer = gpu::Buffer::new_storage(device, pixel_count, None);
        // TODO: Only resize if bigger.
        self.render_targets = RenderTargets::new(device, self.size);
        if self.asvgf.is_some() {
            self.asvgf = Some(ASVGF::new(
                device,
                &mut self.shaders,
                &self.size,
                &self.render_targets.main,
                &self.ray_buffer,
            ));
        }
        self.set_resources(device, scene_resources, probe);
        self.debug_blit_bindgroup.clear(); // Re-create the bindgroup
    }

    pub fn reload_shaders<P: AsRef<std::path::Path> + Debug>(
        &mut self,
        device: &Device,
        directory: P,
    ) {
        println!("Reload shaders {:?}", directory);
        self.shaders.add_directory(directory).unwrap();

        // TODO: This assumes that the bind group layout doesn't change.

        let empty_defines = FastHashMap::default();
        match ShadingPass::new(
            device,
            &self.shaders,
            &empty_defines,
            &self.geometry_bindgroup_layout,
            &self.surface_bindgroup_layout,
        ) {
            Ok(pass) => self.passes.shading = pass,
            Err(e) => println!("Failed to reload shading.comp, reason:\n{:?}", e),
        };
        match PrimaryRayPass::new(
            device,
            &self.shaders,
            &self.geometry_bindgroup_layout,
            &self.surface_bindgroup_layout,
        ) {
            Ok(pass) => self.passes.primary_rays = pass,
            Err(e) => println!(
                "Failed to reload primary rays (shading.comp), reason:\n{:?}",
                e
            ),
        };

        if let Some(asvgf) = self.asvgf.as_mut() {
            asvgf.reload_shaders(device, &self.shaders);
        }
    }

    pub fn raytrace(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        view_transform: &Mat4,
    ) {
        const STATIC_NUM_BOUNCES: u32 = 3;
        const MOVING_NUM_BOUNCES: u32 = 2;

        self.frame_back = !self.frame_back;

        let bindgroups = &self.frame_bindgroups;
        let bindgroups = match bindgroups {
            Some(val) => val,
            None => return,
        };

        let nb_bounces = if !self.accumulate {
            MOVING_NUM_BOUNCES
        } else {
            STATIC_NUM_BOUNCES
        };

        // Step 1:
        //     * Update the frame uniforms.
        //     * Send the uniforms to the GPU.

        let geometry_bindgroup = match &self.geometry_bindgroup {
            Some(val) => val,
            None => return,
        };
        let surface_bindgroup = self.surface_bindgroup.as_ref().unwrap();

        let dispatch_size: (u32, u32, u32) = (self.size.0, self.size.1, 1);

        let camera = {
            let mut camera = Camera {
                ..Default::default()
            };
            camera.dimensions = [self.size.0, self.size.1];
            camera.set_transform(&view_transform);
            camera
        };
        self.camera_uniforms.update(&queue, &[camera]);
        self.global_uniforms.dimensions = [self.size.0, self.size.1];
        self.global_uniforms_buffer
            .update(&queue, &[self.global_uniforms]);

        // Step 1:
        //
        // Generate a ray struct for every fragment.

        self.queries.start("ray generation", encoder);
        self.passes
            .rays
            .dispatch(encoder, &bindgroups.generate_ray_pass, dispatch_size);
        self.queries.end(encoder);

        // Step 2:
        //
        // Primary intersection
        self.global_uniforms.seed += 1;
        self.global_uniforms.bounces = 0;
        self.global_uniforms_buffer
            .update(&queue, &[self.global_uniforms]);
        self.queries.start("primary intersection", encoder);
        self.passes.intersection.dispatch(
            encoder,
            &geometry_bindgroup,
            &bindgroups.intersection_pass,
            dispatch_size,
        );
        self.queries.end(encoder);

        if let Some(asvgf) = self.asvgf.as_mut() {
            asvgf.start();
            // Step 3:
            //
            // First shading ray
            self.queries.start("shading 0", encoder);
            self.passes.primary_rays.dispatch(
                encoder,
                geometry_bindgroup,
                surface_bindgroup,
                &bindgroups.primary_rays[asvgf.curr_frame()],
                dispatch_size,
                &self.prev_model_to_screen,
            );
            self.queries.end(encoder);
        }

        // Alternate between intersection & shading.
        let start_bounce = if self.asvgf.is_none() { 0 } else { 1 };
        for i in start_bounce..nb_bounces {
            // @todo: Use dynamic offset
            self.global_uniforms.seed += 1;
            self.global_uniforms.bounces = i;
            self.global_uniforms_buffer
                .update(&queue, &[self.global_uniforms]);

            self.queries.start(format!("intersection {}", i), encoder);
            self.passes.intersection.dispatch(
                encoder,
                &geometry_bindgroup,
                &bindgroups.intersection_pass,
                dispatch_size,
            );
            self.queries.end(encoder);

            self.queries.start(format!("shading {}", i), encoder);
            self.passes.shading.dispatch(
                encoder,
                geometry_bindgroup,
                surface_bindgroup,
                &bindgroups.shading_pass,
                dispatch_size,
            );
            self.queries.end(encoder);
        }

        match self.mode {
            BlitMode::DenoisedPathrace => {
                let asvgf = self.asvgf.as_mut().unwrap();
                self.queries.start("asvgf", encoder);
                asvgf.render(encoder, &dispatch_size, &self.render_targets.main_texture);
                self.queries.end(encoder);
            }
            BlitMode::Temporal => {
                let asvgf = self.asvgf.as_mut().unwrap();
                asvgf.temporal_pass(encoder, &dispatch_size);
            }
            BlitMode::Pahtrace => {
                // Accumulation
                let accumulate_bindgroup = if self.global_uniforms.frame_count % 2 != 0 {
                    &bindgroups.accumulate_pass
                } else {
                    &bindgroups.accumulate_pass2
                };

                self.passes
                    .accumulation
                    .dispatch(encoder, accumulate_bindgroup, dispatch_size);

                if self.accumulate {
                    self.global_uniforms.frame_count += 1;
                }
            }
            _ => {}
        }

        if let Some(_) = self.asvgf.as_mut() {
            let inv_view = view_transform.inverse();
            let world_to_screen = camera.perspective(0.01, 100.0) * inv_view;
            self.prev_model_to_screen = world_to_screen;
        }

        self.queries.resolve(encoder);
    }

    pub fn blit(
        &mut self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        if self.debug_blit_bindgroup.is_empty() {
            match self.mode {
                BlitMode::DenoisedPathrace => {
                    self.debug_blit_bindgroup = self.create_debug_bindgroup(
                        device,
                        &self.render_targets.main,
                        &self.render_targets.main,
                    );
                }
                BlitMode::Temporal => {
                    let textures = &self.asvgf.as_ref().unwrap().resources.pingpong;
                    self.debug_blit_bindgroup = self.create_debug_bindgroup(
                        device,
                        &textures[0].radiance,
                        &textures[1].radiance,
                    );
                }
                BlitMode::GBuffer => {
                    let textures = &self.asvgf.as_ref().unwrap().resources.pingpong;
                    self.debug_blit_bindgroup = self.create_debug_bindgroup(
                        device,
                        &textures[0].gbuffer,
                        &textures[1].gbuffer,
                    );
                }
                BlitMode::MotionVector => {
                    let res = &self.asvgf.as_ref().unwrap().resources;
                    self.debug_blit_bindgroup =
                        self.create_debug_bindgroup(device, &res.motion, &res.motion);
                }
                _ => {}
            }
        }

        if self.mode != BlitMode::Pahtrace {
            let index: usize = self.frame_back as usize;
            self.passes
                .blit_texture
                .draw(encoder, &view, &self.debug_blit_bindgroup[index]);
            return;
        }

        let bindgroups: &BindGroups = self.frame_bindgroups.as_ref().unwrap();

        let bindgroup = if self.global_uniforms.frame_count % 2 != 0 {
            &bindgroups.blit_pass
        } else {
            &bindgroups.blit_pass2
        };
        self.passes.blit.draw(encoder, &view, bindgroup);
    }

    pub fn reset_accumulation(&mut self, queue: &wgpu::Queue) {
        self.global_uniforms.frame_count = 1;
        self.accumulate = false;

        if self.mode == BlitMode::Pahtrace {
            // self.global_uniforms.seed = 0;
        }
        self.global_uniforms_buffer
            .update(&queue, &[self.global_uniforms]);
    }

    pub fn upload_noise_texture(
        &mut self,
        device: &Device,
        queue: &wgpu::Queue,
        data: &[u8],
        width: u32,
        height: u32,
        bytes_per_row: u32,
    ) {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Blue Noise Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
            },
            data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
            size,
        );

        self.texture_blue_noise =
            Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
    }

    pub fn use_noise_texture(&mut self, queue: &wgpu::Queue, flag: bool) {
        self.radiance_parameters_buffer.update(
            queue,
            &[RadianceParameters {
                use_noise_texture: flag as u32,
            }],
        )
    }

    pub fn set_blit_mode(&mut self, mode: BlitMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.debug_blit_bindgroup.clear(); // Re-create
    }

    pub fn get_size(&self) -> &(u32, u32) {
        &self.size
    }

    pub fn set_resources(
        &mut self,
        device: &Device,
        scene_resources: &SceneGPU,
        probe: Option<&ProbeGPU>,
    ) {
        let texture_info_view = match &scene_resources.atlas {
            Some(atlas) => atlas.info_texture_view(),
            _ => device.default_textures().non_filterable_1d(),
        };
        let texture_atlas_view = match &scene_resources.atlas {
            Some(atlas) => atlas.texture_view(),
            _ => device.default_textures().filterable_2darray(),
        };
        let probe_view = match probe {
            Some(p) => &p.view,
            _ => device.default_textures().filterable_2d(),
        };
        // TODO: It's now required to order the set the blue noise texture first. This is error
        // prone and not really worth.
        let noise_texture = match &self.texture_blue_noise {
            Some(p) => &p,
            _ => device.default_textures().filterable_2d(),
        };

        self.frame_bindgroups = Some(self.create_bind_groups(device));
        self.geometry_bindgroup = Some(self.geometry_bindgroup_layout.create_bindgroup(
            device,
            scene_resources.bvh_buffer.as_storage_slice().unwrap(),
            scene_resources.instance_buffer.as_storage_slice().unwrap(),
            scene_resources.bvh_tri_buffer.as_storage_slice().unwrap(),
            scene_resources.vertex_buffer.as_storage_slice().unwrap(),
            scene_resources.light_buffer.as_storage_slice().unwrap(),
        ));
        self.surface_bindgroup = Some(self.surface_bindgroup_layout.create_bindgroup(
            device,
            scene_resources.materials_buffer.as_storage_slice().unwrap(),
            probe_view,
            texture_info_view,
            texture_atlas_view,
            device.sampler_nearest(),
            device.sampler_linear(),
            noise_texture,
            self.radiance_parameters_buffer.as_uniform_slice().unwrap(),
        ));
        self.global_uniforms.frame_count = 1;
    }

    pub async fn read_pixels(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Vec<u8>, Error> {
        let alignment = albedo_backend::Alignment2D::texture_buffer_copy(
            self.size.0 as usize,
            std::mem::size_of::<u32>(),
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Read Pixel Encoder"),
        });
        let (width, height) = self.size;
        let gpu_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: height as u64 * alignment.padded_bytes() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let texture_extent = wgpu::Extent3d {
            width: width as u32,
            height: height as u32,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            label: None,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // @todo: this re-create shaders + pipeline layout + life.
        let blit_pass =
            passes::BlitPass::new(device, &self.shaders, wgpu::TextureFormat::Rgba8UnormSrgb);
        blit_pass.draw(
            &mut encoder,
            &view,
            &self.frame_bindgroups.as_ref().unwrap().blit_pass,
        );

        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::ImageCopyBuffer {
                buffer: &gpu_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(alignment.padded_bytes() as u32),
                    rows_per_image: None,
                },
            },
            texture_extent,
        );
        queue.submit(Some(encoder.finish()));

        let buffer_slice = gpu_buffer.slice(..);
        // Sets the buffer up for mapping, sending over the result of the mapping back to us when it is finished.
        let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());

        device.poll(wgpu::Maintain::Wait);

        if let Some(Ok(())) = receiver.receive().await {
            let padded_buffer = buffer_slice.get_mapped_range();
            let mut bytes: Vec<u8> = vec![0; alignment.unpadded_bytes_per_row * height as usize];
            // from the padded_buffer we write just the unpadded bytes into the image
            for (padded, bytes) in padded_buffer
                .chunks_exact(alignment.padded_bytes_per_row)
                .zip(bytes.chunks_exact_mut(alignment.unpadded_bytes_per_row))
            {
                bytes.copy_from_slice(&padded[..alignment.unpadded_bytes_per_row]);
            }
            // With the current interface, we have to make sure all mapped views are
            // dropped before we unmap the buffer.
            drop(padded_buffer);
            gpu_buffer.unmap();
            Ok(bytes)
        } else {
            Err(Error::TextureToBufferReadFail)
        }
    }

    fn create_bind_groups(&self, device: &Device) -> BindGroups {
        let resources = RaytraceResources {
            rays: self.ray_buffer.as_storage_slice().unwrap(),
            intersections: self.intersection_buffer.as_storage_slice().unwrap(),
            global_uniforms: self.global_uniforms_buffer.as_uniform_slice().unwrap(),
            camera_uniforms: self.camera_uniforms.as_uniform_slice().unwrap(),
        };
        let asvgf = self.asvgf.as_ref().unwrap();
        let denoise_res = DenoiseResources {
            gbuffer_current: &asvgf.resources.pingpong[0].gbuffer,
            gbuffer_previous: &asvgf.resources.pingpong[1].gbuffer,
            motion: &asvgf.resources.motion,
        };
        BindGroups::new(
            device,
            &self.render_targets,
            &resources,
            &denoise_res,
            &self.passes.rays,
            &self.passes.intersection,
            &self.passes.shading,
            &self.passes.primary_rays,
            &self.passes.accumulation,
            &self.passes.blit,
        )
    }

    fn create_debug_bindgroup(
        &self,
        device: &Device,
        curr: &wgpu::TextureView,
        prev: &wgpu::TextureView,
    ) -> Vec<wgpu::BindGroup> {
        vec![
            self.passes.blit_texture.create_frame_bind_groups(
                device.inner(),
                curr,
                device.sampler_nearest(),
            ),
            self.passes.blit_texture.create_frame_bind_groups(
                device.inner(),
                prev,
                device.sampler_nearest(),
            ),
        ]
    }
}
