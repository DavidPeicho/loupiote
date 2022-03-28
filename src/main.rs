use std::sync::{Arc, Mutex};

use albedo_rtx::renderer::resources::LightGPU;
use winit::{
    event::{self},
    event_loop::EventLoop,
};

mod errors;

mod utils;

mod gltf_loader;
use gltf_loader::load_gltf;

mod gui;

mod scene;
use scene::{Scene, SceneGPU};

mod camera;
use camera::CameraMoveCommand;

mod renderer;
use renderer::Renderer;

struct WindowApp {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    window: winit::window::Window,
    event_loop: EventLoop<EventLoopContext>,
    surface: wgpu::Surface,
    queue: wgpu::Queue,
    size: winit::dpi::PhysicalSize<u32>,
}

pub struct ApplicationContext {
    window: winit::window::Window,
    device: wgpu::Device,
    queue: wgpu::Queue,
    scene: Scene<gltf_loader::ProxyMesh>,
    scene_gpu: SceneGPU,
    wait: bool,
}

enum EventLoopContext {}

async fn setup() -> WindowApp {
    let event_loop: EventLoop<EventLoopContext> = EventLoop::with_user_event();
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
        max_storage_buffer_binding_size: 256 * 1024 * 1024,
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

// fn watch_shading_shader(
//     hotwatch: &mut hotwatch::Hotwatch,
//     device_mutex: &Arc<Mutex<wgpu::Device>>,
//     renderer_mutex: &Arc<Mutex<Renderer>>,
// ) {
//     const PATH: &str = "../../albedo/albedo/crates/albedo_rtx/src/shaders/shading.comp.spv";

//     let device = device_mutex.clone();
//     let renderer = renderer_mutex.clone();
//     hotwatch
//         .watch(PATH, move |event: hotwatch::Event| {
//             if let hotwatch::Event::Write(_) = event {
//                 let file_data = utils::load_file(PATH);
//                 let desc = wgpu::ShaderModuleDescriptor {
//                     label: Some("Shading"),
//                     source: wgpu::util::make_spirv(&file_data[..]),
//                 };
//                 println!("[ SHADER COMPILATION ]: updating '{}'...", PATH);
//                 renderer
//                     .lock()
//                     .unwrap()
//                     .passes
//                     .shading
//                     .set_shader(&device.lock().unwrap(), &desc);
//                 println!("[ SHADER COMPILATION ]: '{}' updated!", PATH);
//             }
//         })
//         .expect("failed to watch file!");
// }

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

    let adapter_info = adapter.get_info();

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

    let mut scene = load_gltf(&"./assets/suzanne-instancing.glb").unwrap();
    scene.lights = vec![LightGPU::from_matrix(
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
    camera_controller.rotation_enabled = false;

    // Per-screen resolution resources, including:
    //  * Rays
    //  * Intersections
    //  * Render Target

    //// GPU Scene:

    //// Load HDRi enviromment.
    let file_reader =
        std::io::BufReader::new(std::fs::File::open("./assets/uffizi-large.hdr").unwrap());
    let decoder = image::codecs::hdr::HdrDecoder::new(file_reader).unwrap();
    let metadata = decoder.metadata();
    let image_data = decoder.read_image_native().unwrap();
    let image_data_raw = unsafe {
        std::slice::from_raw_parts(
            image_data.as_ptr() as *const u8,
            image_data.len() * std::mem::size_of::<image::codecs::hdr::Rgbe8Pixel>(),
        )
    };

    //// Load HDRi enviromment.
    let mut app_context = ApplicationContext {
        window,
        scene_gpu: SceneGPU::new_from_scene(&scene, &device, &queue),
        scene,
        device,
        queue,
        wait: false,
    };
    app_context.scene_gpu.upload_probe(
        &app_context.device,
        &app_context.queue,
        image_data_raw,
        metadata.width,
        metadata.height,
    );

    //// Renderer:

    #[cfg(not(target_arch = "wasm32"))]
    let mut last_update_inst = std::time::Instant::now();
    let mut last_time = std::time::Instant::now();
    let start_time = std::time::Instant::now();

    // let mut hotwatch = hotwatch::Hotwatch::new().expect("hotwatch failed to initialize!");
    // watch_shading_shader(&mut hotwatch, &device, &renderer);

    //
    // Create GUI.
    //
    let mut gui = gui::GUI::new(&app_context.device, &app_context.window, &surface_config);
    gui.scene_info_window.adapter_name = adapter_info.name;
    gui.scene_info_window
        .set_meshes_count(app_context.scene.meshes.len());
    gui.scene_info_window
        .set_bvh_nodes_count(app_context.scene.node_buffer.len());

    let renderer = Arc::new(Mutex::new(Renderer::new(
        &app_context.device,
        (size.width, size.height),
        swapchain_format,
        &app_context.scene_gpu,
    )));

    event_loop.run(move |event, _, control_flow| {
        let event_captured = gui.handle_event(&event);
        match event {
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
                surface_config.width = new_size.0;
                surface_config.height = new_size.1;
                renderer.lock().unwrap().resize(
                    &app_context.device,
                    &app_context.scene_gpu,
                    new_size,
                );
                surface.configure(&app_context.device, &surface_config);
            }

            winit::event::Event::DeviceEvent { event, .. } => match event {
                event::DeviceEvent::MouseMotion { delta } => {
                    if !event_captured {
                        camera_controller.rotate(
                            (delta.0 / (size.width as f64 * 0.5)) as f32,
                            (delta.1 / (size.height as f64 * 0.5)) as f32,
                        );
                    }
                }
                _ => {}
            },

            event::Event::WindowEvent { event, .. } => match event {
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
                    if !event_captured {
                        match state {
                            event::ElementState::Pressed => {
                                camera_controller.set_command(direction)
                            }
                            event::ElementState::Released => {
                                camera_controller.unset_command(direction)
                            }
                        };
                    }
                }
                event::WindowEvent::CloseRequested => {
                    *control_flow = winit::event_loop::ControlFlow::Exit;
                }
                event::WindowEvent::MouseInput { button, state, .. } => {
                    if button == winit::event::MouseButton::Left {
                        camera_controller.rotation_enabled = state == event::ElementState::Pressed;
                    }
                }
                _ => {}
            },

            // event::Event::RedrawEventsCleared => {
            //     #[cfg(not(target_arch = "wasm32"))]
            //     {
            //         // Clamp to some max framerate to avoid busy-looping too much
            //         // (we might be in wgpu::PresentMode::Mailbox, thus discarding superfluous frames)
            //         //
            //         // winit has window.current_monitor().video_modes() but that is a list of all full screen video modes.
            //         // So without extra dependencies it's a bit tricky to get the max refresh rate we can run the window on.
            //         // Therefore we just go with 60fps - sorry 120hz+ folks!
            //         let target_frametime = std::time::Duration::from_secs_f64(1.0 / 60.0);
            //         let time_since_last_frame = last_update_inst.elapsed();
            //         if time_since_last_frame >= target_frametime {
            //             println!("Request Redraw");
            //             app_context.window.request_redraw();
            //             last_update_inst = std::time::Instant::now();
            //         } else {
            //             *control_flow = winit::event_loop::ControlFlow::WaitUntil(
            //                 std::time::Instant::now() + target_frametime - time_since_last_frame,
            //             );
            //         }
            //     }

            //     #[cfg(target_arch = "wasm32")]
            //     window.request_redraw();
            // }

            // event::Event::RedrawRequested(_) => {
            event::Event::MainEventsCleared => {
                // // Record the frame time at the start of the frame.
                let frame_start_time = std::time::Instant::now();

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
                renderer.update_camera(camera_controller.origin, camera_right, camera_up);
                renderer.accumulate = camera_controller.is_static();
                renderer.blit_only = event_captured;
                let encoder = renderer.render(&app_context.device, &view, &app_context.queue);

                let mut encoder_gui =
                    app_context
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("encoder-gui"),
                        });
                // Render GUI.
                gui.performance_info_window
                    .set_global_performance(duration.as_millis() as f64);
                gui.render(
                    &mut app_context,
                    &mut renderer,
                    &surface_config,
                    &mut encoder_gui,
                    &view,
                    start_time.elapsed().as_secs_f64(),
                );

                app_context
                    .queue
                    .submit([encoder.finish(), encoder_gui.finish()]);

                frame.present();

                // // Sleep for the remaining time to cap at 60Hz
                // let elapsed = std::time::Instant::now().duration_since(frame_start_time);
                // let remaining =
                //     std::time::Duration::from_secs_f32(1.0 / 60.0).saturating_sub(elapsed);
                // std::thread::sleep(remaining);
            }
            _ => {}
        }
    });
}
