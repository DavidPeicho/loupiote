use albedo_backend::{ComputePass, GPUBuffer, UniformBuffer};

use albedo_rtx::passes;
use albedo_rtx::uniforms::{Camera, Intersection, PerDrawUniforms, Ray};

use crate::device::Device;
use crate::errors::Error;
use crate::scene::SceneGPU;

struct ScreenBoundResourcesGPU {
    ray_buffer: GPUBuffer<Ray>,
    intersection_buffer: GPUBuffer<Intersection>,
    render_target: wgpu::Texture,
    render_target_view: wgpu::TextureView,
}

impl ScreenBoundResourcesGPU {
    fn new(device: &wgpu::Device, size: (u32, u32)) -> Self {
        let render_target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Render Target"),
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
        });
        let pixel_count = (size.0 * size.0) as usize;
        ScreenBoundResourcesGPU {
            ray_buffer: GPUBuffer::new_with_usage_count(
                &device,
                wgpu::BufferUsages::STORAGE,
                pixel_count as usize,
            ),
            intersection_buffer: GPUBuffer::new_with_count(&device, pixel_count),
            render_target_view: render_target.create_view(&wgpu::TextureViewDescriptor::default()),
            render_target,
        }
    }
}

struct BindGroups {
    generate_ray_pass: wgpu::BindGroup,
    intersection_pass: wgpu::BindGroup,
    shading_pass: wgpu::BindGroup,
    accumulate_pass: wgpu::BindGroup,
    blit_pass: wgpu::BindGroup,
}

impl BindGroups {
    fn new(
        device: &Device,
        screen_resources: &ScreenBoundResourcesGPU,
        scene_resources: &SceneGPU,
        global_uniforms: &UniformBuffer<PerDrawUniforms>,
        camera_uniforms: &UniformBuffer<Camera>,
        render_target_sampler: &wgpu::Sampler,
        filtered_sampler_2d: &wgpu::Sampler,
        ray_pass_desc: &passes::RayPass,
        intersector_pass_desc: &passes::IntersectorPass,
        shading_pass_desc: &passes::ShadingPassDescriptor,
        accumulation_pass_desc: &passes::AccumulationPassDescriptor,
        blit_pass: &passes::BlitPass,
    ) -> Self {
        BindGroups {
            generate_ray_pass: ray_pass_desc.create_frame_bind_groups(
                device.inner(),
                &screen_resources.ray_buffer,
                camera_uniforms,
            ),
            intersection_pass: intersector_pass_desc.create_frame_bind_groups(
                device.inner(),
                &screen_resources.intersection_buffer,
                &scene_resources.instance_buffer,
                &scene_resources.bvh_buffer.inner(),
                &scene_resources.index_buffer,
                &scene_resources.vertex_buffer.inner(),
                &scene_resources.light_buffer,
                &screen_resources.ray_buffer,
            ),
            shading_pass: shading_pass_desc.create_frame_bind_groups(
                device.inner(),
                &screen_resources.ray_buffer,
                &scene_resources.bvh_buffer.inner(),
                &screen_resources.intersection_buffer,
                &scene_resources.instance_buffer,
                &scene_resources.index_buffer,
                &scene_resources.vertex_buffer.inner(),
                &scene_resources.light_buffer,
                &scene_resources.materials_buffer,
                scene_resources.probe_texture_view.as_ref().unwrap(),
                scene_resources.atlas.as_ref().unwrap().info_texture_view(),
                scene_resources.atlas.as_ref().unwrap().texture_view(),
                global_uniforms,
                device.sampler_nearest(),
            ),
            accumulate_pass: accumulation_pass_desc.create_frame_bind_groups(
                device.inner(),
                &screen_resources.ray_buffer,
                &screen_resources.render_target_view,
                global_uniforms,
            ),
            blit_pass: blit_pass.create_frame_bind_groups(
                device.inner(),
                &screen_resources.render_target_view,
                &render_target_sampler,
                global_uniforms,
            ),
        }
    }
}

pub struct Passes {
    pub rays: passes::RayPass,
    pub intersection: passes::IntersectorPass,
    pub shading: passes::ShadingPassDescriptor,
    pub accumulation: passes::AccumulationPassDescriptor,
    pub blit: passes::BlitPass,
}

pub struct Renderer {
    screen_bound_resources: ScreenBoundResourcesGPU,
    downsampled_screen_bound_resources: ScreenBoundResourcesGPU,

    camera: Camera,
    camera_uniforms: UniformBuffer<Camera>,
    global_uniforms: PerDrawUniforms,
    global_uniforms_buffer: UniformBuffer<PerDrawUniforms>,

    pub passes: Passes,
    fullscreen_bindgroups: Option<BindGroups>,
    downsample_bindgroups: Option<BindGroups>,

    size: (u32, u32),

    pub downsample_factor: f32,
    pub accumulate: bool,
}

impl Renderer {
    pub fn new(
        device: &Device,
        size: (u32, u32),
        swapchain_format: wgpu::TextureFormat,
        scene_resources: &SceneGPU,
    ) -> Self {
        let downsample_factor = 0.25;
        let downsampled_size = (
            (size.0 as f32 * downsample_factor) as u32,
            (size.1 as f32 * downsample_factor) as u32,
        );
        let mut renderer = Renderer {
            screen_bound_resources: ScreenBoundResourcesGPU::new(device.inner(), size),
            downsampled_screen_bound_resources: ScreenBoundResourcesGPU::new(
                device.inner(),
                downsampled_size,
            ),
            camera: Default::default(),
            camera_uniforms: UniformBuffer::new(device.inner()),
            global_uniforms: PerDrawUniforms {
                frame_count: 1,
                seed: 0,
                ..Default::default()
            },
            global_uniforms_buffer: UniformBuffer::new(device.inner()),
            passes: Passes {
                rays: passes::RayPass::new(device.inner(), None),
                intersection: passes::IntersectorPass::new(device.inner()),
                shading: passes::ShadingPassDescriptor::new(device.inner()),
                accumulation: passes::AccumulationPassDescriptor::new(device.inner()),
                blit: passes::BlitPass::new(device.inner(), swapchain_format),
            },
            fullscreen_bindgroups: None,
            downsample_bindgroups: None,
            size,
            downsample_factor,
            accumulate: false,
        };
        renderer.set_resources(device, scene_resources);
        renderer
    }

    pub fn update_camera(&mut self, origin: glam::Vec3, right: glam::Vec3, up: glam::Vec3) {
        self.camera.origin = origin;
        self.camera.right = right;
        self.camera.up = up;
    }

    pub fn resize(&mut self, device: &Device, scene_resources: &SceneGPU, size: (u32, u32)) {
        self.size = size;
        let downsample_size = self.get_downsampled_size();
        self.screen_bound_resources = ScreenBoundResourcesGPU::new(device.inner(), self.size);
        self.downsampled_screen_bound_resources =
            ScreenBoundResourcesGPU::new(device.inner(), downsample_size);
        self.set_resources(device, scene_resources);
    }

    pub fn raytrace(&mut self, encoder: &mut wgpu::CommandEncoder, queue: &wgpu::Queue) {
        const WORKGROUP_SIZE: (u32, u32, u32) = (8, 8, 1);
        const STATIC_NUM_BOUNCES: u32 = 3;
        const MOVING_NUM_BOUNCES: u32 = 2;

        let mut bindgroups = &self.fullscreen_bindgroups;

        // Step 1:
        //     * Update the frame uniforms.
        //     * Send the uniforms to the GPU.
        //     * Select fullscreen / downsample resolution.

        let mut nb_bounces = STATIC_NUM_BOUNCES;
        let mut size = self.size;
        if !self.accumulate {
            nb_bounces = MOVING_NUM_BOUNCES;
            bindgroups = &self.downsample_bindgroups;
            size = self.get_downsampled_size();
        }

        self.camera.dimensions = [size.0, size.1];
        self.camera_uniforms.update(&queue, &self.camera);

        let dispatch_size = (size.0, size.1, 1);
        let dispatch_workoup_size = albedo_rtx::get_dispatch_size(dispatch_size, WORKGROUP_SIZE);

        // Step 1:
        //
        // Generate a ray struct for every fragment.
        self.passes.rays.dispatch(
            encoder,
            &bindgroups.as_ref().unwrap().generate_ray_pass,
            dispatch_workoup_size,
        );

        // Step 2:
        //
        // Alternate between intersection & shading.
        for i in 0..nb_bounces {
            self.global_uniforms.seed += 1;
            self.global_uniforms.bounces = i;
            self.global_uniforms_buffer
                .update(&queue, &self.global_uniforms);
            self.passes.intersection.dispatch(
                encoder,
                &bindgroups.as_ref().unwrap().intersection_pass,
                dispatch_workoup_size,
            );
            ComputePass::new(
                encoder,
                &self.passes.shading,
                &bindgroups.as_ref().unwrap().shading_pass,
            )
            .dispatch(&(), dispatch_size, WORKGROUP_SIZE);
        }

        // Accumulation.
        ComputePass::new(
            encoder,
            &self.passes.accumulation,
            &bindgroups.as_ref().unwrap().accumulate_pass,
        )
        .dispatch(&(), dispatch_size, WORKGROUP_SIZE);

        if self.accumulate {
            self.global_uniforms.frame_count += 1;
        }
    }

    pub fn blit(&mut self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let bindgroups = if self.accumulate {
            &self.fullscreen_bindgroups
        } else {
            &self.downsample_bindgroups
        };
        self.passes
            .blit
            .draw(encoder, &view, &bindgroups.as_ref().unwrap().blit_pass);
    }

    pub fn reset_accumulation(&mut self) {
        self.global_uniforms.frame_count = 1;
        self.global_uniforms.seed = 0;
        self.accumulate = false;
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

    pub fn set_resources(&mut self, device: &Device, scene_resources: &SceneGPU) {
        self.fullscreen_bindgroups = Some(BindGroups::new(
            device,
            &self.screen_bound_resources,
            &scene_resources,
            &self.global_uniforms_buffer,
            &self.camera_uniforms,
            device.sampler_nearest(),
            device.sampler_linear(),
            &self.passes.rays,
            &self.passes.intersection,
            &self.passes.shading,
            &self.passes.accumulation,
            &self.passes.blit,
        ));
        self.downsample_bindgroups = Some(BindGroups::new(
            device,
            &self.downsampled_screen_bound_resources,
            &scene_resources,
            &self.global_uniforms_buffer,
            &self.camera_uniforms,
            device.sampler_nearest(),
            device.sampler_linear(),
            &self.passes.rays,
            &self.passes.intersection,
            &self.passes.shading,
            &self.passes.accumulation,
            &self.passes.blit,
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
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // @todo: this re-create shaders + pipeline layout + life.
        let blit_pass = passes::BlitPass::new(device, wgpu::TextureFormat::Rgba8UnormSrgb);
        blit_pass.draw(
            &mut encoder,
            &view,
            &self.fullscreen_bindgroups.as_ref().unwrap().blit_pass,
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
