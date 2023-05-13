use std::convert::TryInto;

use albedo_backend::gpu;

use albedo_rtx::passes;
use albedo_rtx::uniforms::{Camera, Intersection, PerDrawUniforms, Ray, Uniform};

use crate::device::Device;
use crate::errors::Error;
use crate::scene::SceneGPU;
use crate::ProbeGPU;

struct BindGroups {
    generate_ray_pass: wgpu::BindGroup,
    intersection_pass: wgpu::BindGroup,
    shading_pass: wgpu::BindGroup,
    accumulate_pass: wgpu::BindGroup,
    #[cfg(feature = "accumulate_read_write")]
    accumulate_pass2: wgpu::BindGroup,
    blit_pass: wgpu::BindGroup,
    #[cfg(feature = "accumulate_read_write")]
    blit_pass2: wgpu::BindGroup,
    lightmap_pass: wgpu::BindGroup,
}

impl BindGroups {
    fn new(
        device: &Device,
        size: (u32, u32),
        ray_buffer: &gpu::Buffer<Ray>,
        intersection_buffer: &gpu::Buffer<Intersection>,
        render_target_view: &wgpu::TextureView,
        #[cfg(feature = "accumulate_read_write")] render_target_view2: &wgpu::TextureView,
        scene_resources: &SceneGPU,
        probe: Option<&ProbeGPU>,
        global_uniforms: &gpu::Buffer<PerDrawUniforms>,
        camera_uniforms: &gpu::Buffer<Camera>,
        ray_pass_desc: &passes::RayPass,
        intersector_pass_desc: &passes::IntersectorPass,
        shading_pass_desc: &passes::ShadingPass,
        accumulation_pass_desc: &passes::AccumulationPass,
        blit_pass: &passes::BlitPass,
        lightmap_pass: &passes::LightmapPass,
    ) -> Self {
        let texture_info_view = match &scene_resources.atlas {
            Some(atlas) => atlas.info_texture_view(),
            _ => device.default_textures().non_filterable_1d(),
        };
        let atlas_view = match &scene_resources.atlas {
            Some(atlas) => atlas.texture_view(),
            _ => device.default_textures().filterable_2darray(),
        };
        let probe = match probe {
            Some(p) => &p.view,
            _ => device.default_textures().filterable_2d(),
        };
        BindGroups {
            generate_ray_pass: ray_pass_desc.create_frame_bind_groups(
                device.inner(),
                &ray_buffer,
                &camera_uniforms.try_into().unwrap(),
            ),
            intersection_pass: intersector_pass_desc.create_frame_bind_groups(
                device.inner(),
                &intersection_buffer,
                &scene_resources.instance_buffer,
                &scene_resources.bvh_buffer.inner(),
                &scene_resources.index_buffer,
                &scene_resources.vertex_buffer.inner(),
                &scene_resources.light_buffer,
                &ray_buffer,
            ),
            shading_pass: shading_pass_desc.create_frame_bind_groups(
                device.inner(),
                &ray_buffer,
                &scene_resources.bvh_buffer.inner(),
                &intersection_buffer,
                &scene_resources.instance_buffer,
                &scene_resources.index_buffer,
                &scene_resources.vertex_buffer.inner(),
                &scene_resources.light_buffer,
                &scene_resources.materials_buffer,
                probe,
                texture_info_view,
                atlas_view,
                &global_uniforms.try_into().unwrap(),
                device.sampler_nearest(),
                device.sampler_linear(),
            ),
            #[cfg(not(feature = "accumulate_read_write"))]
            accumulate_pass: accumulation_pass_desc.create_frame_bind_groups(
                device.inner(),
                &ray_buffer,
                global_uniforms,
                &render_target_view,
            ),
            #[cfg(feature = "accumulate_read_write")]
            accumulate_pass: accumulation_pass_desc.create_frame_bind_groups(
                device.inner(),
                &ray_buffer,
                global_uniforms,
                &render_target_view,
                &render_target_view2,
                &render_target_sampler,
            ),
            #[cfg(feature = "accumulate_read_write")]
            accumulate_pass2: accumulation_pass_desc.create_frame_bind_groups(
                device.inner(),
                &ray_buffer,
                global_uniforms,
                &render_target_view2,
                &render_target_view,
                &device.sampler_nearest(),
            ),
            blit_pass: blit_pass.create_frame_bind_groups(
                device.inner(),
                &render_target_view,
                &device.sampler_nearest(),
                global_uniforms,
            ),
            #[cfg(feature = "accumulate_read_write")]
            blit_pass2: blit_pass.create_frame_bind_groups(
                device.inner(),
                &render_target_view2,
                &device.sampler_nearest(),
                global_uniforms,
            ),
            lightmap_pass: lightmap_pass.create_frame_bind_groups(
                device.inner(),
                &scene_resources.instance_buffer,
                &scene_resources.bvh_buffer.inner(),
                &scene_resources.index_buffer,
                &scene_resources.vertex_buffer.inner(),
                global_uniforms,
            ),
        }
    }
}

pub struct Passes {
    pub rays: passes::RayPass,
    pub intersection: passes::IntersectorPass,
    pub shading: passes::ShadingPass,
    pub accumulation: passes::AccumulationPass,
    pub blit: passes::BlitPass,
    pub lightmap: passes::LightmapPass,
}

pub struct Renderer {
    camera: Camera,
    global_uniforms: PerDrawUniforms,

    camera_uniforms: gpu::Buffer<Camera>,
    global_uniforms_buffer: gpu::Buffer<PerDrawUniforms>,
    ray_buffer: gpu::Buffer<Ray>,
    intersection_buffer: gpu::Buffer<Intersection>,
    render_target_view: wgpu::TextureView,
    #[cfg(feature = "accumulate_read_write")]
    render_target_view2: wgpu::TextureView,

    pub passes: Passes,
    bindgroups: Option<BindGroups>,

    size: (u32, u32),

    pub downsample_factor: f32,
    pub accumulate: bool,
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

    pub fn new(device: &Device, size: (u32, u32), swapchain_format: wgpu::TextureFormat) -> Self {
        let render_target = gpu::TextureBuilder::new_2d(size.0, size.1)
            .label(Some("Main"))
            .format(wgpu::TextureFormat::Rgba32Float)
            .usage(
                wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
            )
            .build(device.into());

        #[cfg(feature = "accumulate_read_write")]
        let render_target2 = gpu::TextureBuilder::new_2d(size.0, size.1)
            .label(Some("Main"))
            .format(wgpu::TextureFormat::Rgba32Float)
            .usage(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING)
            .build(device.into());

        let pixel_count: u64 = size.0 as u64 * size.1 as u64;

        Self {
            camera: Default::default(),
            camera_uniforms: gpu::Buffer::new_uniform(device.into(), 1, None),
            global_uniforms: PerDrawUniforms {
                frame_count: 1,
                seed: 0,
                ..Default::default()
            },
            ray_buffer: gpu::Buffer::new_storage(
                device.into(),
                pixel_count as u64,
                Some(gpu::BufferInitDescriptor {
                    label: Some("Ray Buffer"),
                    usage: wgpu::BufferUsages::STORAGE,
                }),
            ),
            intersection_buffer: gpu::Buffer::new_storage(device.into(), pixel_count, None),
            render_target_view: render_target.create_view(&wgpu::TextureViewDescriptor::default()),
            #[cfg(feature = "accumulate_read_write")]
            render_target_view2: render_target2
                .create_view(&wgpu::TextureViewDescriptor::default()),
            global_uniforms_buffer: gpu::Buffer::new_uniform(device.into(), 1, None),
            passes: Passes {
                rays: passes::RayPass::new(device.into(), None),
                intersection: passes::IntersectorPass::new(device.into(), None),
                shading: passes::ShadingPass::new(device.into()),
                accumulation: passes::AccumulationPass::new(device.into(), None),
                blit: passes::BlitPass::new(device.into(), swapchain_format),
                lightmap: passes::LightmapPass::new(device.into(), swapchain_format),
            },
            bindgroups: None,
            size,
            downsample_factor: 0.25,
            accumulate: false,
        }
    }

    pub fn update_camera(&mut self, origin: glam::Vec3, right: glam::Vec3, up: glam::Vec3) {
        self.camera.origin = origin;
        self.camera.right = right;
        self.camera.up = up;
    }

    pub fn resize(
        &mut self,
        device: &Device,
        scene_resources: &SceneGPU,
        probe: Option<&ProbeGPU>,
        size: (u32, u32),
    ) {
        self.size = size;

        let pixel_count: u64 = size.0 as u64 * size.1 as u64;

        let render_target = gpu::TextureBuilder::new_2d(size.0, size.1)
            .label(Some("Main"))
            .format(wgpu::TextureFormat::Rgba32Float)
            .usage(
                wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
            )
            .build(device.into());
        #[cfg(feature = "accumulate_read_write")]
        let render_target2 = gpu::TextureBuilder::new_2d(size.0, size.1)
            .label(Some("Main"))
            .format(wgpu::TextureFormat::Rgba32Float)
            .usage(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING)
            .build(device.into());

        self.ray_buffer = gpu::Buffer::new_storage(
            device.into(),
            pixel_count as u64,
            Some(gpu::BufferInitDescriptor {
                label: Some("Ray Buffer"),
                usage: wgpu::BufferUsages::STORAGE,
            }),
        );
        self.intersection_buffer = gpu::Buffer::new_storage(device.into(), pixel_count, None);
        self.render_target_view =
            render_target.create_view(&wgpu::TextureViewDescriptor::default());
        #[cfg(feature = "accumulate_read_write")]
        {
            self.render_target_view2 =
                render_target2.create_view(&wgpu::TextureViewDescriptor::default());
        }

        self.set_resources(device, scene_resources, probe);
    }

    pub fn lightmap(&mut self, encoder: &mut wgpu::CommandEncoder, scene_resources: &SceneGPU) {
        let bindgroups = match &self.bindgroups {
            Some(b) => b,
            _ => return,
        };

        self.passes.lightmap.draw(
            encoder,
            &self.render_target_view,
            &bindgroups.lightmap_pass,
            &scene_resources.instance_buffer,
            &scene_resources.index_buffer,
            &scene_resources.vertex_buffer.inner(),
        );
    }

    pub fn raytrace(&mut self, encoder: &mut wgpu::CommandEncoder, queue: &wgpu::Queue) {
        const STATIC_NUM_BOUNCES: u32 = 3;
        const MOVING_NUM_BOUNCES: u32 = 2;

        // Step 1:
        //     * Update the frame uniforms.
        //     * Send the uniforms to the GPU.
        //     * Select fullscreen / downsample resolution.

        let mut nb_bounces = STATIC_NUM_BOUNCES;
        let mut size: (u32, u32) = self.size;
        if !self.accumulate {
            nb_bounces = MOVING_NUM_BOUNCES;
            size = self.get_downsampled_size();
        }

        let bindgroups = match &self.bindgroups {
            Some(val) => val,
            None => return,
        };

        let dispatch_size: (u32, u32, u32) = (size.0, size.1, 1);

        self.camera.dimensions = [size.0, size.1];
        self.camera_uniforms.update(&queue, &[self.camera]);
        self.global_uniforms.dimensions = [dispatch_size.0, dispatch_size.1];

        self.global_uniforms_buffer
            .update(&queue, &[self.global_uniforms]);

        // Step 1:
        //
        // Generate a ray struct for every fragment.
        self.passes
            .rays
            .dispatch(encoder, &bindgroups.generate_ray_pass, dispatch_size);

        // Step 2:
        //
        // Alternate between intersection & shading.
        for i in 0..nb_bounces {
            self.global_uniforms.seed += 1;
            self.global_uniforms.bounces = i;
            self.global_uniforms_buffer
                .update(&queue, &[self.global_uniforms]);
            self.passes.intersection.dispatch(
                encoder,
                &bindgroups.intersection_pass,
                dispatch_size,
            );
            self.passes
                .shading
                .dispatch(encoder, &bindgroups.shading_pass, dispatch_size);
        }

        // Accumulation
        #[cfg(not(feature = "accumulate_read_write"))]
        let accumulate_bindgroup = &bindgroups.accumulate_pass;
        #[cfg(feature = "accumulate_read_write")]
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

    pub fn blit(&mut self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let bindgroups = match &self.bindgroups {
            Some(b) => b,
            _ => return,
        };

        #[cfg(not(feature = "accumulate_read_write"))]
        let bindgroup = &bindgroups.blit_pass;
        #[cfg(feature = "accumulate_read_write")]
        let bindgroup = if self.global_uniforms.frame_count % 2 != 0 {
            &bindgroups.blit_pass
        } else {
            &bindgroups.blit_pass2
        };
        self.passes.blit.draw(encoder, &view, bindgroup);
    }

    pub fn reset_accumulation(&mut self, queue: &wgpu::Queue) {
        self.global_uniforms.frame_count = 1;
        self.global_uniforms.seed = 0;
        self.accumulate = false;
        self.global_uniforms_buffer
            .update(&queue, &[self.global_uniforms]);
    }

    pub fn get_size(&self) -> (u32, u32) {
        self.size
    }

    pub fn get_downsampled_size(&self) -> (u32, u32) {
        let w = self.size.0 as f32;
        let h = self.size.1 as f32;
        (
            (w * self.downsample_factor) as u32,
            (h * self.downsample_factor) as u32,
        )
    }

    pub fn set_resources(
        &mut self,
        device: &Device,
        scene_resources: &SceneGPU,
        probe: Option<&ProbeGPU>,
    ) {
        self.bindgroups = Some(BindGroups::new(
            device,
            self.size,
            &self.ray_buffer,
            &self.intersection_buffer,
            &self.render_target_view,
            #[cfg(feature = "accumulate_read_write")]
            &self.render_target_view2,
            scene_resources,
            probe,
            &self.global_uniforms_buffer,
            &self.camera_uniforms,
            &self.passes.rays,
            &self.passes.intersection,
            &self.passes.shading,
            &self.passes.accumulation,
            &self.passes.blit,
            &self.passes.lightmap,
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
        let blit_pass = passes::BlitPass::new(device, wgpu::TextureFormat::Rgba8UnormSrgb);
        blit_pass.draw(
            &mut encoder,
            &view,
            &self.bindgroups.as_ref().unwrap().blit_pass,
        );

        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::ImageCopyBuffer {
                buffer: &gpu_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        std::num::NonZeroU32::new(alignment.padded_bytes() as u32).unwrap(),
                    ),
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
}
