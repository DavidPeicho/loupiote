use std::sync::{Arc, Mutex};

use albedo_rtx::mesh::Mesh;
use albedo_rtx::renderer::resources::LightGPU;
use winit::{
    event::{self},
    event_loop::EventLoop,
};

mod utils;

mod gltf_loader;
use gltf_loader::load_gltf;

mod scene;
use scene::SceneGPU;

mod camera;
use camera::CameraMoveCommand;

mod renderer;
use renderer::Renderer;

struct WindowApp {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    window: winit::window::Window,
    event_loop: EventLoop<()>,
    surface: wgpu::Surface,
    queue: wgpu::Queue,
    size: winit::dpi::PhysicalSize<u32>,
}

async fn setup() -> WindowApp {
    let event_loop = EventLoop::new();
    let mut builder = winit::window::WindowBuilder::new();
    builder = builder.with_title("Albedo Pathtracer");

    let window = builder.build(&event_loop).unwrap();

    let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);
    let (size, surface) = unsafe {
        let size = window.inner_size();
        let surface = instance.create_surface(&window);
        (size, surface)
    };
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .expect("No suitable GPU adapters found on the system!");

    let optional_features: wgpu::Features = wgpu::Features::default();
    let required_features: wgpu::Features =
        wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;

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

    WindowApp {
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

fn watch_shading_shader(
    hotwatch: &mut hotwatch::Hotwatch,
    device_mutex: &Arc<Mutex<wgpu::Device>>,
    renderer_mutex: &Arc<Mutex<Renderer>>,
) {
    const PATH: &str = "../../albedo/albedo/crates/albedo_rtx/src/shaders/shading.comp.spv";

    let device = device_mutex.clone();
    let renderer = renderer_mutex.clone();
    hotwatch
        .watch(PATH, move |event: hotwatch::Event| {
            if let hotwatch::Event::Write(_) = event {
                let file_data = utils::load_file(PATH);
                let desc = wgpu::ShaderModuleDescriptor {
                    label: Some("Shading"),
                    source: wgpu::util::make_spirv(&file_data[..]),
                };
                println!("[ SHADER COMPILATION ]: updating '{}'...", PATH);
                renderer
                    .lock()
                    .unwrap()
                    .passes
                    .shading
                    .set_shader(&device.lock().unwrap(), &desc);
                println!("[ SHADER COMPILATION ]: '{}' updated!", PATH);
            }
        })
        .expect("failed to watch file!");
}

fn main() {
    let WindowApp {
        instance,
        adapter,
        device,
        window,
        event_loop,
        surface,
        queue,
        size,
    } = pollster::block_on(setup());

    println!("\n============================================================");
    println!("                   🚀 Albedo Pathtracer 🚀                   ");
    println!("============================================================\n");

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
    println!("➡️  Scene\n");

    println!("\tMaterials =\n");
    for mat in &scene.materials {
        println!(
            "\t( color: {}, roughness: {}, metalness: {} ),",
            mat.color, mat.roughness, mat.reflectivity
        );
    }

    println!("➡️  BVH\n");
    println!("\tNode Count = {}", scene.node_buffer.len());
    println!("\tNodes =");
    for (mesh, bvh) in scene.meshes.iter().zip(scene.bvhs.iter()) {
        println!("\t{{");
        println!("\t\tVertices = {}", mesh.vertex_count());
        println!("\t\tTris = {}", mesh.index_count() / 3);
        println!("\t\tNodes = {}", bvh.nodes.len());
        println!("\t\tDepth = {}", bvh.compute_depth());
        println!("\t}}");
    }
    //// Scene Info

    let lights = vec![LightGPU::from_matrix(
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

    // Per-screen resolution resources, including:
    //  * Rays
    //  * Intersections
    //  * Render Target

    //// GPU Scene:

    //// Load HDRi enviromment.
    let file_reader =
        std::io::BufReader::new(std::fs::File::open("./assets/uffizi-large.hdr").unwrap());
    let decoder = image::hdr::HdrDecoder::new(file_reader).unwrap();
    let metadata = decoder.metadata();
    let image_data = decoder.read_image_native().unwrap();
    let image_data_raw = unsafe {
        std::slice::from_raw_parts(
            image_data.as_ptr() as *const u8,
            image_data.len() * std::mem::size_of::<image::hdr::Rgbe8Pixel>(),
        )
    };

    //// Load HDRi enviromment.

    let mut scene_resources_gpu = SceneGPU::new(
        &device,
        &scene.instances,
        &scene.materials,
        &scene.node_buffer,
        &scene.index_buffer,
        &scene.vertex_buffer,
        &lights,
    );
    scene_resources_gpu
        .instance_buffer
        .update(&queue, &scene.instances);
    scene_resources_gpu
        .materials_buffer
        .update(&queue, &scene.materials);
    scene_resources_gpu
        .bvh_buffer
        .update(&queue, &scene.node_buffer);
    scene_resources_gpu
        .index_buffer
        .update(&queue, &scene.index_buffer);
    scene_resources_gpu
        .vertex_buffer
        .update(&queue, &scene.vertex_buffer);
    scene_resources_gpu.light_buffer.update(&queue, &lights);
    scene_resources_gpu.update_globals(&queue, scene.instances.len() as u32, lights.len() as u32);
    scene_resources_gpu.upload_probe(
        &device,
        &queue,
        image_data_raw,
        metadata.width,
        metadata.height,
    );

    //// Renderer:

    #[cfg(not(target_arch = "wasm32"))]
    let mut last_update_inst = std::time::Instant::now();
    let mut last_time = std::time::Instant::now();

    let renderer = Arc::new(Mutex::new(Renderer::new(
        &device,
        (size.width, size.height),
        swapchain_format,
        &scene_resources_gpu,
    )));
    let device = Arc::new(Mutex::new(device));

    let mut hotwatch = hotwatch::Hotwatch::new().expect("hotwatch failed to initialize!");
    watch_shading_shader(&mut hotwatch, &device, &renderer);

    {
        let renderer = renderer.lock().unwrap();
        let size = renderer.get_size();
        let downsampled_size = renderer.get_downsampled_size();
        println!("➡️  Info\n");
        println!("\tDimension = {}x{}", size.0, size.1);
        println!(
            "\tDownsample Dimension = {}x{}",
            downsampled_size.0, downsampled_size.1
        );
    }

    event_loop.run(move |event, _, control_flow| {
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

            event::Event::WindowEvent {
                event:
                    event::WindowEvent::Resized(size)
                    | event::WindowEvent::ScaleFactorChanged {
                        new_inner_size: &mut size,
                        ..
                    },
                ..
            } => {
                let new_size = (size.width.max(1), size.height.max(1));
                let device = device.lock().unwrap();
                surface_config.width = new_size.0;
                surface_config.height = new_size.1;
                surface.configure(&device, &surface_config);
                renderer
                    .lock()
                    .unwrap()
                    .resize(&device, &scene_resources_gpu, new_size);

                println!("\tDimensions = {}x{}", new_size.0, new_size.1);
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
                    // event::WindowEvent::MouseInput { button, state, .. } => {

                    //     // if *button == winit::event::MouseButton::Right {
                    //     //     self.movement_locked = *state == ElementState::Released;
                    //     // }
                    // }
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

                let (camera_right, camera_up) = camera_controller.update(delta);

                let mut renderer = renderer.lock().unwrap();
                let device = device.lock().unwrap();
                renderer.update_camera(camera_controller.origin, camera_right, camera_up);
                renderer.accumulate = camera_controller.is_static();
                let encoder = renderer.render(&device, &view, &queue);
                queue.submit(Some(encoder.finish()));
                frame.present();
            }
            _ => {}
        }
    });
}
