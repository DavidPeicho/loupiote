use albedo_backend::{ComputePass, GPUBuffer, UniformBuffer};

use albedo_rtx::renderer::resources::{
    RayGPU,
    GlobalUniformsGPU,
    IntersectionGPU,
    CameraGPU,
};
use albedo_rtx::passes::{
    AccumulationPassDescriptor,
    RayGeneratorPassDescriptor,
    IntersectorPassDescriptor,
    ShadingPassDescriptor,
    BlitPass,
};

use crate::scene::SceneGPU;

struct ScreenBoundResourcesGPU {
    ray_buffer: GPUBuffer<RayGPU>,
    intersection_buffer: GPUBuffer<IntersectionGPU>,
    render_target: wgpu::Texture,
    render_target_view: wgpu::TextureView,
}

impl ScreenBoundResourcesGPU {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let render_target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Render Target"),
            size: wgpu::Extent3d {
                width: width,
                height: height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
        });
        let pixel_count = (width * height) as usize;
        ScreenBoundResourcesGPU {
            ray_buffer: GPUBuffer::new_with_usage_count(
                &device,
                wgpu::BufferUsages::STORAGE,
                pixel_count as usize
            ),
            intersection_buffer: GPUBuffer::new_with_count(&device, pixel_count),
            render_target_view: render_target.create_view(&wgpu::TextureViewDescriptor::default()),
            render_target: render_target
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
        blit_pass: &BlitPass
    ) -> Self {
        BindGroups {
            generate_ray_pass: ray_pass_desc.create_frame_bind_groups(
                &device,
                &screen_resources.ray_buffer,
                camera_uniforms
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
                &scene_resources.scene_settings_buffer,
            ),
            shading_pass: shading_pass_desc.create_frame_bind_groups(&device,
                &screen_resources.ray_buffer,
                &screen_resources.intersection_buffer,
                &scene_resources.instance_buffer,
                &scene_resources.index_buffer,
                &scene_resources.vertex_buffer,
                &scene_resources.light_buffer,
                &scene_resources.materials_buffer,
                &scene_resources.scene_settings_buffer,
                scene_resources.probe_texture_view.as_ref().unwrap(),
                &filtered_sampler_2d,
                global_uniforms
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
            )
        }
    }
}

struct Passes {
    pub rays: RayGeneratorPassDescriptor,
    pub intersection: IntersectorPassDescriptor,
    pub shading: ShadingPassDescriptor,
    pub accumulation: AccumulationPassDescriptor,
    pub blit: BlitPass,
}

pub struct Renderer {
    screen_bound_resources: ScreenBoundResourcesGPU,
    downsampled_screen_bound_resources: ScreenBoundResourcesGPU,

    camera_uniforms: UniformBuffer<CameraGPU>,
    global_uniforms: GlobalUniformsGPU,
    global_uniforms_buffer: UniformBuffer<GlobalUniformsGPU>,

    passes: Passes,
    fullscreen_bindgroups: Option<BindGroups>,
    downsample_bindgroups: Option<BindGroups>,

    nearest_sampler: wgpu::Sampler,
    linear_sampler: wgpu::Sampler,

    size: (u32, u32),
    downsample_size: (u32, u32),

    accumulate_last_frame: bool,
    pub accumulate: bool,
}

impl Renderer {

    fn get_downsampled_size(size: (u32, u32), factor: f32) -> (u32, u32) {
        let w = size.0 as f32;
        let h = size.1 as f32;
        ((w * factor) as u32, (h * factor) as u32)
    }

    pub fn new(
        device: &wgpu::Device,
        size: (u32, u32),
        swapchain_format: wgpu::TextureFormat,
        scene_resources: &SceneGPU
    ) -> Self {
        let downsample_size = Renderer::get_downsampled_size(size, 0.25);
        let mut renderer = Renderer {
            screen_bound_resources: ScreenBoundResourcesGPU::new(&device, size.0, size.1),
            downsampled_screen_bound_resources: ScreenBoundResourcesGPU::new(
                &device,
                downsample_size.0,
                downsample_size.1
            ),
            camera_uniforms: UniformBuffer::new(&device),
            global_uniforms: GlobalUniformsGPU::new(),
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
            downsample_size,
            accumulate: true,
            accumulate_last_frame: false,
        };
        renderer._create_bind_groups(device, scene_resources);
        renderer
    }

    pub fn update_camera(
        &mut self,
        queue: &wgpu::Queue,
        origin: glam::Vec3,
        right: glam::Vec3,
        up: glam::Vec3,
    ) {
        self.camera_uniforms.update(&queue, &CameraGPU {
            origin,
            right,
            up,
            ..Default::default()
        });
    }

    pub fn resize(size: (u32, u32)) {
        
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        queue: &wgpu::Queue
    ) -> wgpu::CommandEncoder {
        const WORKGROUP_SIZE: (u32, u32, u32) = (8, 8, 1);
        const STATIC_NUM_BOUNCES: usize = 5;
        const MOVING_NUM_BOUNCES: usize = 2;

        // Step 1:
        //     * Update the frame uniforms.
        //     * Send the uniforms to the GPU.
        //     * Select fullscreen / downsample resolution.

        let mut nb_bounces = STATIC_NUM_BOUNCES;
        let mut bindgroups = &self.fullscreen_bindgroups;
        let mut dispatch_size = (self.size.0, self.size.1, 1);
        if !self.accumulate {
            nb_bounces = MOVING_NUM_BOUNCES;
            self.global_uniforms.frame_count = 1;
            bindgroups = &self.downsample_bindgroups;
            dispatch_size = (
                self.downsample_size.0,
                self.downsample_size.1,
                1
            );
        }
        if !self.accumulate_last_frame && self.accumulate {
            self.global_uniforms.frame_count = 1
        }
        self.global_uniforms_buffer.update(&queue, &self.global_uniforms);

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: None
        });

        // Step 1:
        //
        // Generate a ray struct for every fragment.
        ComputePass::new(
            &mut encoder,
            &self.passes.rays,
            &bindgroups.as_ref().unwrap().generate_ray_pass
        ).dispatch(&(), dispatch_size, WORKGROUP_SIZE);

        // Step 2:
        //
        // Alternate between intersection & shading.
        for _ in 0..nb_bounces {
            ComputePass::new(
                &mut encoder,
                &self.passes.intersection,
                &bindgroups.as_ref().unwrap().intersection_pass
            ).dispatch(&(), dispatch_size, WORKGROUP_SIZE);
            ComputePass::new(
                &mut encoder,
                &self.passes.shading,
                &bindgroups.as_ref().unwrap().shading_pass
            ).dispatch(&(), dispatch_size, WORKGROUP_SIZE);
        }

        // Accumulation.
        ComputePass::new(
            &mut encoder,
            &self.passes.accumulation,
            &bindgroups.as_ref().unwrap().accumulate_pass
        ).dispatch(&(), dispatch_size, WORKGROUP_SIZE);

        self.passes.blit.draw(&mut encoder, &view, &bindgroups.as_ref().unwrap().blit_pass);

        self.global_uniforms.frame_count += 1;
        self.accumulate_last_frame = self.accumulate;

        encoder
    }

    fn _create_bind_groups(
        &mut self,
        device: &wgpu::Device,
        scene_resources: &SceneGPU
    ) {
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
            &self.passes.blit
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
            &self.passes.blit
        ));
    }

}
