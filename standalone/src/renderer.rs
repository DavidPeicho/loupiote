use albedo_backend::{ComputePass, GPUBuffer, UniformBuffer};

use albedo_rtx::passes::{
    AccumulationPassDescriptor, BlitPass, IntersectorPassDescriptor, RayGeneratorPassDescriptor,
    ShadingPassDescriptor,
};
use albedo_rtx::renderer::resources::{CameraGPU, GlobalUniformsGPU, IntersectionGPU, RayGPU};

use crate::errors::Error;
use crate::scene::SceneGPU;

struct ScreenBoundResourcesGPU {
    ray_buffer: GPUBuffer<RayGPU>,
    intersection_buffer: GPUBuffer<IntersectionGPU>,
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
        device: &wgpu::Device,
        screen_resources: &ScreenBoundResourcesGPU,
        scene_resources: &SceneGPU,
        global_uniforms: &UniformBuffer<GlobalUniformsGPU>,
        camera_uniforms: &UniformBuffer<CameraGPU>,
        render_target_sampler: &wgpu::Sampler,
        filtered_sampler_2d: &wgpu::Sampler,
        ray_pass_desc: &RayGeneratorPassDescriptor,
        intersector_pass_desc: &IntersectorPassDescriptor,
        shading_pass_desc: &ShadingPassDescriptor,
        accumulation_pass_desc: &AccumulationPassDescriptor,
        blit_pass: &BlitPass,
    ) -> Self {
        BindGroups {
            generate_ray_pass: ray_pass_desc.create_frame_bind_groups(
                &device,
                &screen_resources.ray_buffer,
                camera_uniforms,
            ),
            intersection_pass: intersector_pass_desc.create_frame_bind_groups(
                &device,
                &screen_resources.intersection_buffer,
                &scene_resources.instance_buffer,
                &scene_resources.bvh_buffer,
                &scene_resources.index_buffer,
                &scene_resources.vertex_buffer,
                &scene_resources.light_buffer,
                &screen_resources.ray_buffer,
            ),
            shading_pass: shading_pass_desc.create_frame_bind_groups(
                &device,
                &screen_resources.ray_buffer,
                &scene_resources.bvh_buffer,
                &screen_resources.intersection_buffer,
                &scene_resources.instance_buffer,
                &scene_resources.index_buffer,
                &scene_resources.vertex_buffer,
                &scene_resources.light_buffer,
                &scene_resources.materials_buffer,
                scene_resources.probe_texture_view.as_ref().unwrap(),
                &filtered_sampler_2d,
                global_uniforms,
            ),
            accumulate_pass: accumulation_pass_desc.create_frame_bind_groups(
                &device,
                &screen_resources.ray_buffer,
                &screen_resources.render_target_view,
                global_uniforms,
            ),
            blit_pass: blit_pass.create_frame_bind_groups(
                &device,
                &screen_resources.render_target_view,
                &render_target_sampler,
                global_uniforms,
            ),
        }
    }
}

pub struct Passes {
    pub rays: RayGeneratorPassDescriptor,
    pub intersection: IntersectorPassDescriptor,
    pub shading: ShadingPassDescriptor,
    pub accumulation: AccumulationPassDescriptor,
    pub blit: BlitPass,
}

pub struct Renderer {
    screen_bound_resources: ScreenBoundResourcesGPU,
    downsampled_screen_bound_resources: ScreenBoundResourcesGPU,

    camera: CameraGPU,
    camera_uniforms: UniformBuffer<CameraGPU>,
    global_uniforms: GlobalUniformsGPU,
    global_uniforms_buffer: UniformBuffer<GlobalUniformsGPU>,

    pub passes: Passes,
    fullscreen_bindgroups: Option<BindGroups>,
    downsample_bindgroups: Option<BindGroups>,

    nearest_sampler: wgpu::Sampler,
    linear_sampler: wgpu::Sampler,

    size: (u32, u32),

    pub downsample_factor: f32,
    pub accumulate: bool,
}

impl Renderer {
    pub fn new(
        device: &wgpu::Device,
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
            screen_bound_resources: ScreenBoundResourcesGPU::new(&device, size),
            downsampled_screen_bound_resources: ScreenBoundResourcesGPU::new(
                &device,
                downsampled_size,
            ),
            camera: Default::default(),
            camera_uniforms: UniformBuffer::new(&device),
            global_uniforms: GlobalUniformsGPU {
                frame_count: 1,
                seed: 0,
                ..Default::default()
            },
            global_uniforms_buffer: UniformBuffer::new(&device),
            nearest_sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            }),
            linear_sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            }),
            passes: Passes {
                rays: RayGeneratorPassDescriptor::new(&device),
                intersection: IntersectorPassDescriptor::new(&device),
                shading: ShadingPassDescriptor::new(&device),
                accumulation: AccumulationPassDescriptor::new(&device),
                blit: BlitPass::new(&device, swapchain_format),
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

    pub fn resize(&mut self, device: &wgpu::Device, scene_resources: &SceneGPU, size: (u32, u32)) {
        self.size = size;
        let downsample_size = self.get_downsampled_size();
        self.screen_bound_resources = ScreenBoundResourcesGPU::new(&device, self.size);
        self.downsampled_screen_bound_resources =
            ScreenBoundResourcesGPU::new(&device, downsample_size);
        self.set_resources(device, scene_resources);
    }

    pub fn raytrace(&mut self, encoder: &mut wgpu::CommandEncoder, queue: &wgpu::Queue) {
        const WORKGROUP_SIZE: (u32, u32, u32) = (8, 8, 1);
        const STATIC_NUM_BOUNCES: u32 = 5;
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

        // Step 1:
        //
        // Generate a ray struct for every fragment.
        ComputePass::new(
            encoder,
            &self.passes.rays,
            &bindgroups.as_ref().unwrap().generate_ray_pass,
        )
        .dispatch(&(), dispatch_size, WORKGROUP_SIZE);

        // Step 2:
        //
        // Alternate between intersection & shading.
        for i in 0..nb_bounces {
            self.global_uniforms.seed += 1;
            self.global_uniforms.bounces = i;
            self.global_uniforms_buffer
                .update(&queue, &self.global_uniforms);
            ComputePass::new(
                encoder,
                &self.passes.intersection,
                &bindgroups.as_ref().unwrap().intersection_pass,
            )
            .dispatch(&(), dispatch_size, WORKGROUP_SIZE);
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

    pub fn set_resources(&mut self, device: &wgpu::Device, scene_resources: &SceneGPU) {
        self.fullscreen_bindgroups = Some(BindGroups::new(
            &device,
            &self.screen_bound_resources,
            &scene_resources,
            &self.global_uniforms_buffer,
            &self.camera_uniforms,
            &self.nearest_sampler,
            &self.linear_sampler,
            &self.passes.rays,
            &self.passes.intersection,
            &self.passes.shading,
            &self.passes.accumulation,
            &self.passes.blit,
        ));
        self.downsample_bindgroups = Some(BindGroups::new(
            &device,
            &self.downsampled_screen_bound_resources,
            &scene_resources,
            &self.global_uniforms_buffer,
            &self.camera_uniforms,
            &self.nearest_sampler,
            &self.linear_sampler,
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
        let blit_pass = BlitPass::new(device, wgpu::TextureFormat::Rgba8UnormSrgb);
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
        let buffer_future = buffer_slice.map_async(wgpu::MapMode::Read);

        device.poll(wgpu::Maintain::Wait);

        if let Ok(()) = buffer_future.await {
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
