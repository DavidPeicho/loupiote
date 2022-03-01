use std::num::NonZeroU32;

use albedo_backend::{shader_bindings, ComputePass, GPUBuffer, UniformBuffer};

use albedo_rtx::mesh::Mesh;
use albedo_rtx::passes::{
    AccumulationPass,
    BVHDebugPass,
    BlitPass,
    GPUIntersector,
    GPURadianceEstimator,
    GPURayGenerator,
};
use albedo_rtx::renderer::resources;

use winit::{
    event::{self},
    event_loop::EventLoop,
};

mod gltf_loader;
use gltf_loader::load_gltf;

mod camera;
use camera::CameraMoveCommand;



struct App {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    window: winit::window::Window,
    event_loop: EventLoop<()>,
    surface: wgpu::Surface,
    queue: wgpu::Queue,
    size: winit::dpi::PhysicalSize<u32>,
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
    } = pollster::block_on(setup());

    println!("Window Size = {}x{}", size.width, size.height);

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

    let pixel_count = size.width * size.height;

    let render_target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Render Target"),
        size: wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
    });
    let render_target_view = render_target.create_view(&wgpu::TextureViewDescriptor::default());
    let render_target_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
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

    let mut global_uniforms = resources::GlobalUniformsGPU::new();
    let mut global_uniforms_buffer = UniformBuffer::new(&device);

    let mut camera_buffer = UniformBuffer::new(&device);
    camera_buffer.update(&queue, &camera);

    let mut instance_buffer = GPUBuffer::from_data(&device, &scene.instances);
    instance_buffer.update(&queue, &scene.instances);

    let mut materials_buffer = GPUBuffer::from_data(&device, &scene.materials);
    materials_buffer.update(&queue, &scene.materials);

    let mut bvh_buffer = GPUBuffer::from_data(&device, &scene.node_buffer);
    bvh_buffer.update(&queue, &scene.node_buffer);

    let mut index_buffer = GPUBuffer::from_data(&device, &scene.index_buffer);
    index_buffer.update(&queue, &scene.index_buffer);

    let mut vertex_buffer = GPUBuffer::from_data(&device, &scene.vertex_buffer);
    vertex_buffer.update(&queue, &scene.vertex_buffer);

    let mut light_buffer = GPUBuffer::from_data(&device, &lights);
    light_buffer.update(&queue, &lights);

    let mut scene_buffer = UniformBuffer::new(&device);
    scene_buffer.update(
        &queue,
        &resources::SceneSettingsGPU {
            light_count: lights.len() as u32,
            instance_count: scene.instances.len() as u32,
        },
    );

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
    let probe_view = probe_texture.create_view(&wgpu::TextureViewDescriptor::default());
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

    let ray_buffer = GPUBuffer::new_with_usage_count(
        &device,
        wgpu::BufferUsages::STORAGE,
        pixel_count as usize
    );
    let intersection_buffer = GPUBuffer::new_with_count(&device, pixel_count as usize);

    let mut generate_ray_pass = GPURayGenerator::new(&device);
    let mut intersector_pass = GPUIntersector::new(&device);
    let mut shade_pass = GPURadianceEstimator::new(&device);
    let mut accumulation_pass = AccumulationPass::new(&device);
    let mut bvh_debug_pass = BVHDebugPass::new(&device);
    let mut blit_pass = BlitPass::new(&device, swapchain_format);

    generate_ray_pass.bind_buffers(&device, &ray_buffer, &camera_buffer);
    intersector_pass.bind_buffers(
        &device,
        &intersection_buffer,
        &instance_buffer,
        &bvh_buffer,
        &index_buffer,
        &vertex_buffer,
        &light_buffer,
        &ray_buffer,
        &scene_buffer,
    );
    shade_pass.bind_buffers(
        &device,
        &ray_buffer,
        &intersection_buffer,
        &instance_buffer,
        &index_buffer,
        &vertex_buffer,
        &light_buffer,
        &materials_buffer,
        &scene_buffer,
        &probe_view,
        &filtered_sampler_2d,
    );
    shade_pass.bind_target(&device, &global_uniforms_buffer);

    let accumulation_bind_groups = accumulation_pass.create_bind_groups(
        &device,
        &ray_buffer,
        &render_target_view,
        &global_uniforms_buffer,
    );

    blit_pass.bind(
        &device,
        &render_target_view,
        &render_target_sampler,
        &global_uniforms_buffer,
    );
    bvh_debug_pass.bind_buffers(
        &device,
        &ray_buffer,
        &instance_buffer,
        &bvh_buffer,
        &index_buffer,
        &vertex_buffer,
        &scene_buffer,
    );

    const STATIC_NUM_BOUNCES: usize = 5;
    const MOVING_NUM_BOUNCES: usize = 2;

    let mut nb_bounces = STATIC_NUM_BOUNCES;

    #[cfg(not(target_arch = "wasm32"))]
    let mut last_update_inst = std::time::Instant::now();
    let mut last_time = std::time::Instant::now();

    let debug_bvh = false;

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
                camera_buffer.update(&queue, &camera);

                global_uniforms.frame_count = if debug_bvh { 1 } else { global_uniforms.frame_count };
                global_uniforms_buffer.update(&queue, &global_uniforms);

                // Renders.

                let dispatch_size = (size.width, size.height, 1);
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                generate_ray_pass.run(&mut encoder, size.width, size.height);

                if debug_bvh {
                    bvh_debug_pass.run(&mut encoder, size.width, size.height);
                } else {
                    for _ in 0..nb_bounces {
                        intersector_pass.run(&device, &mut encoder, size.width, size.height);
                        shade_pass.run(&mut encoder, size.width, size.height);
                    }
                }

                accumulation_pass.dispatch(
                    &mut encoder,
                    &accumulation_bind_groups,
                    dispatch_size,
                );
                blit_pass.run(&view, &queue, &mut encoder);
                queue.submit(Some(encoder.finish()));
                frame.present();

                global_uniforms.frame_count += 1;
            }
            _ => {}
        }
    });
}
