use std::num::NonZeroU32;

use albedo_backend::{shader_bindings, ComputePass, GPUBuffer, UniformBuffer};

use albedo_rtx::mesh::Mesh;
use albedo_rtx::passes::{
    AccumulationPassDescriptor,
    RayGeneratorPassDescriptor,
    IntersectorPassDescriptor,
    ShadingPassDescriptor,
    BVHDebugPass,
    BlitPass,
};
use albedo_rtx::renderer::resources::{
    self,
    RayGPU,
    IntersectionGPU,
    InstanceGPU,
    MaterialGPU,
    BVHNodeGPU,
    VertexGPU,
    LightGPU,
    SceneSettingsGPU,
    CameraGPU, GlobalUniformsGPU
};

use wgpu::{Texture, TextureView, Device, BindGroup};
use winit::{
    event::{self},
    event_loop::EventLoop,
};

mod gltf_loader;
use gltf_loader::load_gltf;

mod camera;
use camera::CameraMoveCommand;

struct ScreenBoundResourcesGPU {
    ray_buffer: GPUBuffer<RayGPU>,
    intersection_buffer: GPUBuffer<IntersectionGPU>,
    render_target: Texture,
    render_target_view: TextureView,
}

impl ScreenBoundResourcesGPU {
    fn new(device: &Device, width: u32, height: u32) -> Self {
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

struct SceneResourcesGPU {
    global_uniforms_buffer: UniformBuffer<GlobalUniformsGPU>,
    camera_buffer: UniformBuffer<CameraGPU>,
    instance_buffer: GPUBuffer<InstanceGPU>,
    materials_buffer: GPUBuffer<MaterialGPU>,
    bvh_buffer: GPUBuffer<BVHNodeGPU>,
    index_buffer: GPUBuffer<u32>,
    vertex_buffer: GPUBuffer<VertexGPU>,
    light_buffer: GPUBuffer<LightGPU>,
    scene_buffer: UniformBuffer<SceneSettingsGPU>,
    probe_texture: Texture,
    probe_texture_view: TextureView,
}

struct BindGroups {
    generate_ray_pass: BindGroup,
    intersection_pass: BindGroup,
    shading_pass: BindGroup,
    accumulate_pass: BindGroup,
    blit_pass: BindGroup,
}

impl BindGroups {
    fn new(
        device: &wgpu::Device,
        screen_resources: &ScreenBoundResourcesGPU,
        scene_resources: &SceneResourcesGPU,
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
                &scene_resources.camera_buffer
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
                &scene_resources.scene_buffer,
            ),
            shading_pass: shading_pass_desc.create_frame_bind_groups(&device,
                &screen_resources.ray_buffer,
                &screen_resources.intersection_buffer,
                &scene_resources.instance_buffer,
                &scene_resources.index_buffer,
                &scene_resources.vertex_buffer,
                &scene_resources.light_buffer,
                &scene_resources.materials_buffer,
                &scene_resources.scene_buffer,
                &scene_resources.probe_texture_view,
                &filtered_sampler_2d,
                &scene_resources.global_uniforms_buffer
            ),
            accumulate_pass: accumulation_pass_desc.create_frame_bind_groups(
                &device,
                &screen_resources.ray_buffer,
                &screen_resources.render_target_view,
                &scene_resources.global_uniforms_buffer,
            ),
            blit_pass: blit_pass.create_frame_bind_groups(
                &device,
                &screen_resources.render_target_view,
                &render_target_sampler,
                &scene_resources.global_uniforms_buffer,
            )
        }
    }
}

struct App {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    window: winit::window::Window,
    event_loop: EventLoop<()>,
    surface: wgpu::Surface,
    queue: wgpu::Queue,
    size: winit::dpi::PhysicalSize<u32>,
    downsample_size: winit::dpi::PhysicalSize<u32>,
}

impl App {

    fn get_downsampled_size(size: winit::dpi::PhysicalSize<u32>, factor: f32) -> winit::dpi::PhysicalSize<u32> {
        let w = size.width as f32;
        let h = size.height as f32;
        winit::dpi::PhysicalSize::new((w * factor) as u32, (h * factor) as u32)
    }

}

async fn setup() -> App {
    let event_loop = EventLoop::new();
    let mut builder = winit::window::WindowBuilder::new();
    builder = builder.with_title("Albedo Pathtracer");

    let window = builder.build(&event_loop).unwrap();

    let instance = wgpu::Instance::new( wgpu::Backends::PRIMARY);
    let (size, surface) = unsafe {
        let size = window.inner_size();
        let surface = instance.create_surface(&window);
        (size, surface)
    };
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false
        })
        .await
        .expect("No suitable GPU adapters found on the system!");

    let optional_features: wgpu::Features = wgpu::Features::default();
    let required_features: wgpu::Features = wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;

    let adapter_features: wgpu::Features = wgpu::Features::default();
    let needed_limits = wgpu::Limits {
        max_storage_buffers_per_shader_stage: 8,
        ..wgpu::Limits::default()
    };
    let trace_dir = std::env::var("WGPU_TRACE");

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: (optional_features & adapter_features) | required_features,
                limits: needed_limits,
            },
            trace_dir.ok().as_ref().map(std::path::Path::new),
        )
        .await
        .expect("Unable to find a suitable GPU adapter!");

    App {
        instance,
        adapter,
        device,
        window,
        event_loop,
        surface,
        queue,
        size,
        downsample_size: App::get_downsampled_size(size, 0.25)
    }
}

fn main() {
    let App {
        instance,
        adapter,
        device,
        window,
        event_loop,
        surface,
        queue,
        size,
        downsample_size
    } = pollster::block_on(setup());

    println!("Window Size = {}x{}", size.width, size.height);
    println!("Downsampled Window Size = {}x{}", downsample_size.width, downsample_size.height);

    let swapchain_format = surface.get_preferred_format(&adapter).unwrap();
    let mut surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: swapchain_format,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Mailbox,
    };

    let surface = unsafe { instance.create_surface(&window) };
    surface.configure(&device, &surface_config);

    let scene = load_gltf(&"./assets/cornell-box.glb");
    // let scene = load_gltf(&"./assets/cornell-box-reflections.glb");
    // let scene = load_gltf(&"./assets/suzanne.glb");
    // let scene = load_gltf(&"./assets/meetmat-head.glb");

    //// Scene Info

    println!("Materials = [");
    for mat in &scene.materials {
        println!(
            "\t( color: {}, roughness: {}, metalness: {} ),",
            mat.color, mat.roughness, mat.reflectivity
        );
    }
    println!("]");
    println!("Material count = {}", scene.materials.len());

    println!("BVHs = [");
    for (mesh, bvh) in scene.meshes.iter().zip(scene.bvhs.iter()) {
        println!("\t{{");
        println!("\t\tVertices = {}", mesh.vertex_count());
        println!("\t\tTris = {}", mesh.index_count() / 3);
        println!("\t\tNodes = {}", bvh.nodes.len());
        println!("\t\tDepth = {}", bvh.compute_depth());
        println!("\t}}");
    }
    println!("\tFlattened Nodes = {}", scene.node_buffer.len());
    println!("]");

    //// Scene Info

    let lights = vec![resources::LightGPU::from_matrix(
        glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(1.0, 1.0, 1.0),
            glam::Quat::from_rotation_x(1.5),
            glam::Vec3::new(0.0, 3.0, 0.75),
        ),
    )];

    let mut camera_controller = camera::CameraController::from_origin_dir(
        glam::Vec3::new(0.0, 0.0, 5.0),
        glam::Vec3::new(0.0, 0.0, -1.0),
    );
    camera_controller.move_speed_factor = 0.15;

    let mut camera = resources::CameraGPU::new();

    // Per-screen resolution resources, including:
    //  * Rays
    //  * Intersections
    //  * Render Target
    let screen_bound_resources = ScreenBoundResourcesGPU::new(&device, size.width, size.height);
    let downsampled_screen_bound_resources = ScreenBoundResourcesGPU::new(
        &device,
        downsample_size.width,
        downsample_size.height
    );

    let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    let filtered_sampler_2d = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    /**
     * Scene Resources GPU.
     */

    //// Load HDRi enviromment.
    let file_reader = std::io::BufReader::new(
        std::fs::File::open("./assets/uffizi-large.hdr").unwrap(),
    );
    let decoder = image::hdr::HdrDecoder::new(file_reader).unwrap();
    let metadata = decoder.metadata();
    let image_data = decoder.read_image_native().unwrap();
    let image_data_raw = unsafe {
        std::slice::from_raw_parts(
            image_data.as_ptr() as *const u8,
            image_data.len() * std::mem::size_of::<image::hdr::Rgbe8Pixel>(),
        )
    };
    let probe_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Cubemap"),
        size: wgpu::Extent3d {
            width: metadata.width,
            height: metadata.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    });
    let probe_texture_view = probe_texture.create_view(&wgpu::TextureViewDescriptor::default());
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &probe_texture,
            aspect: wgpu::TextureAspect::All,
            mip_level: 0,
            origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
        },
        image_data_raw,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: NonZeroU32::new(std::mem::size_of::<image::hdr::Rgbe8Pixel>() as u32 * metadata.width),
            rows_per_image: NonZeroU32::new(metadata.height),
        },
        wgpu::Extent3d {
            width: metadata.width,
            height: metadata.height,
            depth_or_array_layers: 1,
        },
    );
    //// Load HDRi enviromment.

    let mut global_uniforms = resources::GlobalUniformsGPU::new();
    let mut scene_resources_gpu = SceneResourcesGPU {
        global_uniforms_buffer: UniformBuffer::new(&device),
        camera_buffer: UniformBuffer::new(&device),
        instance_buffer: GPUBuffer::from_data(&device, &scene.instances),
        materials_buffer: GPUBuffer::from_data(&device, &scene.materials),
        bvh_buffer: GPUBuffer::from_data(&device, &scene.node_buffer),
        index_buffer: GPUBuffer::from_data(&device, &scene.index_buffer),
        vertex_buffer: GPUBuffer::from_data(&device, &scene.vertex_buffer),
        light_buffer: GPUBuffer::from_data(&device, &lights),
        scene_buffer: UniformBuffer::new(&device),
        probe_texture,
        probe_texture_view,
    };
    scene_resources_gpu.camera_buffer.update(&queue, &camera);
    scene_resources_gpu.instance_buffer.update(&queue, &scene.instances);
    scene_resources_gpu.materials_buffer.update(&queue, &scene.materials);
    scene_resources_gpu.bvh_buffer.update(&queue, &scene.node_buffer);
    scene_resources_gpu.index_buffer.update(&queue, &scene.index_buffer);
    scene_resources_gpu.vertex_buffer.update(&queue, &scene.vertex_buffer);
    scene_resources_gpu.light_buffer.update(&queue, &lights);
    scene_resources_gpu.scene_buffer.update(
        &queue,
        &resources::SceneSettingsGPU {
            light_count: lights.len() as u32,
            instance_count: scene.instances.len() as u32,
        },
    );

    // Creates every passes:
    //  * Ray Generator
    //  * Ray Intersector
    //  * Shading
    //  * Accumulation
    //  * Blitting

    let generate_ray_descriptor = RayGeneratorPassDescriptor::new(&device);
    let intersector_pass_descriptor = IntersectorPassDescriptor::new(&device);
    let accumulation_pass_descriptor = AccumulationPassDescriptor::new(&device);
    let shading_pass_descriptor = ShadingPassDescriptor::new(&device);
    let blit_pass = BlitPass::new(&device, swapchain_format);

    let bindgroups_fullres = BindGroups::new(
        &device,
        &screen_bound_resources,
        &scene_resources_gpu,
        &nearest_sampler,
        &filtered_sampler_2d,
        &generate_ray_descriptor,
        &intersector_pass_descriptor,
        &shading_pass_descriptor,
        &accumulation_pass_descriptor,
        &blit_pass
    );
    let bindgroups_downsampled = BindGroups::new(
        &device,
        &downsampled_screen_bound_resources,
        &scene_resources_gpu,
        &nearest_sampler,
        &filtered_sampler_2d,
        &generate_ray_descriptor,
        &intersector_pass_descriptor,
        &shading_pass_descriptor,
        &accumulation_pass_descriptor,
        &blit_pass
    );

    const STATIC_NUM_BOUNCES: usize = 5;
    const MOVING_NUM_BOUNCES: usize = 2;
    const WORKGROUP_SIZE: (u32, u32, u32) = (8, 8, 1);

    let mut nb_bounces = STATIC_NUM_BOUNCES;

    #[cfg(not(target_arch = "wasm32"))]
    let mut last_update_inst = std::time::Instant::now();
    let mut last_time = std::time::Instant::now();

    let mut fullscreen_dimensions = (size.width, size.height);
    let mut downsample_dimensions = (downsample_size.width, downsample_size.height);
    let mut was_moving = false;
    event_loop.run(move |event, _, control_flow| {
        // let _ = (&renderer, &app);
        match event {
            event::Event::RedrawEventsCleared => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    // Clamp to some max framerate to avoid busy-looping too much
                    // (we might be in wgpu::PresentMode::Mailbox, thus discarding superfluous frames)
                    //
                    // winit has window.current_monitor().video_modes() but that is a list of all full screen video modes.
                    // So without extra dependencies it's a bit tricky to get the max refresh rate we can run the window on.
                    // Therefore we just go with 60fps - sorry 120hz+ folks!

                    // @todo: shouldn't limit pathtracer to 60FPS if possible.
                    let target_frametime = std::time::Duration::from_secs_f64(1.0 / 60.0);
                    let time_since_last_frame = last_update_inst.elapsed();
                    if time_since_last_frame >= target_frametime {
                        window.request_redraw();
                        last_update_inst = std::time::Instant::now();
                    } else {
                        *control_flow = winit::event_loop::ControlFlow::WaitUntil(
                            std::time::Instant::now() + target_frametime - time_since_last_frame,
                        );
                    }

                    // spawner.run_until_stalled();
                }
            }

            winit::event::Event::DeviceEvent { event, .. } => {
                match event {
                    event::DeviceEvent::MouseMotion { delta } => {
                        // self.mouse_delta = (self.mouse_delta.0 + delta.0, self.mouse_delta.1 + delta.1);
                        // println!("Velocity = {}, {}", (delta.0 / (size.width as f64)) as f32, (delta.0 / (size.height as f64)) as f32);
                        camera_controller.rotate(
                            (delta.0 / (size.width as f64 * 0.5)) as f32,
                            (delta.1 / (size.height as f64 * 0.5)) as f32,
                        );
                    }
                    _ => {}
                }
            }

            event::Event::WindowEvent { event, .. } => {
                match event {
                    event::WindowEvent::KeyboardInput {
                        input:
                            event::KeyboardInput {
                                virtual_keycode: Some(virtual_keycode),
                                state,
                                ..
                            },
                        ..
                    } => {
                        match virtual_keycode {
                            event::VirtualKeyCode::Escape => {
                                *control_flow = winit::event_loop::ControlFlow::Exit
                            }
                            _ => (),
                        };
                        let direction = match virtual_keycode {
                            event::VirtualKeyCode::S | event::VirtualKeyCode::Down => {
                                CameraMoveCommand::Backward
                            }
                            event::VirtualKeyCode::A | event::VirtualKeyCode::Left => {
                                CameraMoveCommand::Left
                            }
                            event::VirtualKeyCode::D | event::VirtualKeyCode::Right => {
                                CameraMoveCommand::Right
                            }
                            event::VirtualKeyCode::W | event::VirtualKeyCode::Up => {
                                CameraMoveCommand::Forward
                            }
                            _ => CameraMoveCommand::None,
                        };
                        match state {
                            event::ElementState::Pressed => {
                                camera_controller.set_command(direction)
                            }
                            event::ElementState::Released => {
                                camera_controller.unset_command(direction)
                            }
                        };
                    }
                    event::WindowEvent::CloseRequested => {
                        *control_flow = winit::event_loop::ControlFlow::Exit;
                    }
                    event::WindowEvent::MouseInput { button, state, .. } => {

                        // if *button == winit::event::MouseButton::Right {
                        //     self.movement_locked = *state == ElementState::Released;
                        // }
                    }
                    _ => {}
                }
            }

            event::Event::RedrawRequested(_) => {
                let frame = surface
                    .get_current_texture()
                    .expect("Failed to acquire next swap chain texture");
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                // Updates.

                let duration = std::time::Instant::now() - last_time;
                last_time += duration;

                // @todo: this assumes 60FPS, it shouldn't.
                let delta =
                    (duration.as_secs() as f32 + duration.subsec_nanos() as f32 * 1.0e-9) * 60.0;

                let (right, up) = camera_controller.update(delta);
                camera.origin = camera_controller.origin;
                camera.right = right;
                camera.up = up;

                if !camera_controller.is_static() {
                    nb_bounces = MOVING_NUM_BOUNCES;
                    global_uniforms.frame_count = 1;
                } else {
                    nb_bounces = STATIC_NUM_BOUNCES;
                }
                global_uniforms.frame_count = if was_moving && camera_controller.is_static() {
                    1
                } else {
                    global_uniforms.frame_count
                };
                scene_resources_gpu.camera_buffer.update(&queue, &camera);
                scene_resources_gpu.global_uniforms_buffer.update(&queue, &global_uniforms);

                // Renders.

                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                let (bindgroups, dispatch_size) = if camera_controller.is_static() {
                    (&bindgroups_fullres, (fullscreen_dimensions.0, fullscreen_dimensions.1, 1))
                } else {
                    (&bindgroups_downsampled, (downsample_dimensions.0, downsample_dimensions.1, 1))
                };

                // Step 1:
                //
                // Generate a ray struct for every fragment.
                ComputePass::new(
                    &mut encoder,
                    &generate_ray_descriptor,
                    &bindgroups.generate_ray_pass
                ).dispatch(&(), dispatch_size, WORKGROUP_SIZE);

                // Step 2:
                //
                // Alternate between intersection & shading.
                for _ in 0..nb_bounces {
                    ComputePass::new(
                        &mut encoder,
                        &intersector_pass_descriptor,
                        &bindgroups.intersection_pass
                    ).dispatch(&(), dispatch_size, WORKGROUP_SIZE);
                    ComputePass::new(
                        &mut encoder,
                        &shading_pass_descriptor,
                        &bindgroups.shading_pass
                    ).dispatch(&(), dispatch_size, WORKGROUP_SIZE);
                }

                // Accumulation.
                ComputePass::new(
                    &mut encoder,
                    &accumulation_pass_descriptor,
                    &bindgroups.accumulate_pass
                ).dispatch(&(), dispatch_size, WORKGROUP_SIZE);

                blit_pass.draw(&mut encoder, &view, &bindgroups.blit_pass);
                queue.submit(Some(encoder.finish()));
                frame.present();

                global_uniforms.frame_count += 1;
                was_moving = !camera_controller.is_static();
            }
            _ => {}
        }
    });
}
