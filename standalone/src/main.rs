use winit;

use albedo_lib::*;

mod app;
use app::*;

mod async_exec;
use async_exec::Spawner;

mod event;
use event::*;

mod commands;

mod settings;
use settings::Settings;

mod errors;
mod utils;

mod logger;
use logger::log;

mod input_manager;
use input_manager::InputManager;

mod gui;

mod camera;
use camera::CameraMoveCommand;

fn run((event_loop, platform): (winit::event_loop::EventLoop<Event>, Plaftorm)) {
    let event_loop_proxy = event_loop.create_proxy();

    log!("\n============================================================");
    log!("                   🚀 Albedo Pathtracer 🚀                   ");
    log!("============================================================\n");

    let swapchain_format = platform.surface.get_supported_formats(&platform.adapter)[0];
    let mut surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: swapchain_format,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        width: platform.size.width,
        height: platform.size.height,
        #[cfg(target_arch = "wasm32")]
        present_mode: wgpu::PresentMode::Fifo,
        #[cfg(not(target_arch = "wasm32"))]
        present_mode: wgpu::PresentMode::Immediate,
    };

    let surface = unsafe { platform.instance.create_surface(&platform.window) };
    surface.configure(&platform.device.inner(), &surface_config);

    let limits = platform.device.inner().limits();

    let mut camera_controller = camera::CameraController::from_origin_dir(
        glam::Vec3::new(0.0, 0.0, 5.0),
        glam::Vec3::new(0.0, 0.0, -1.0),
    );
    camera_controller.rotation_enabled = false;
    camera_controller.move_speed_factor = 0.01;
    camera_controller.rot_speed_factor = glam::Vec2::new(0.01, 0.01);

    let renderer = Renderer::new(
        &platform.device,
        (platform.size.width, platform.size.height),
        swapchain_format,
    );

    let scene = Scene::default();
    let scene_gpu = SceneGPU::new_from_scene(&scene, platform.device.inner(), &platform.queue);

    let mut gui = gui::GUI::new(&platform.device.inner(), &event_loop, &surface_config);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let adapter_info = platform.adapter.get_info();
        gui.windows.scene_info_window.adapter_name = adapter_info.name;
    }

    let mut app_context = ApplicationContext {
        platform,
        event_loop_proxy,
        executor: Spawner::new(),
        probe: None,
        scene,
        scene_gpu,
        limits,
        renderer,
        gui,
        settings: Settings::new(),
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        app_context.load_env_path("./assets/uffizi-large.hdr");
        app_context.load_file_path("./assets/DamagedHelmet.glb");
    }

    app_context.resize(
        app_context.platform.size.width.max(1),
        app_context.platform.size.height.max(1),
    );

    #[cfg(not(target_arch = "wasm32"))]
    let mut last_time = std::time::Instant::now();

    #[cfg(target_arch = "wasm32")]
    let win_performance = web_sys::window()
        .unwrap()
        .performance()
        .expect("performance should be available");
    #[cfg(target_arch = "wasm32")]
    let mut last_time = win_performance.now();

    // let mut hotwatch = hotwatch::Hotwatch::new().expect("hotwatch failed to initialize!");
    // watch_shading_shader(&mut hotwatch, &device, &renderer);

    let input_manager = InputManager::new();
    event_loop.run(move |event, _, control_flow| {
        app_context.gui.handle_event(&event);
        let event_captured = app_context.gui.captured();
        match event {
            winit::event::Event::UserEvent(event) => app_context.event(event),
            winit::event::Event::WindowEvent {
                event:
                    winit::event::WindowEvent::Resized(size)
                    | winit::event::WindowEvent::ScaleFactorChanged {
                        new_inner_size: &mut size,
                        ..
                    },
                ..
            } => {
                let width = size.width.max(1);
                let height = size.height.max(1);
                surface_config.width = width;
                surface_config.height = height;
                surface.configure(app_context.platform.device.inner(), &surface_config);
                app_context.resize(width, height);
            }

            winit::event::Event::DeviceEvent { event, .. } => match event {
                winit::event::DeviceEvent::MouseMotion { delta } => {
                    if !event_captured {
                        camera_controller.rotate(
                            (delta.0 / (app_context.platform.size.width as f64 * 0.5)) as f32,
                            (delta.1 / (app_context.platform.size.height as f64 * 0.5)) as f32,
                        );
                    }
                }
                _ => {}
            },

            winit::event::Event::WindowEvent { event, .. } => match event {
                winit::event::WindowEvent::KeyboardInput {
                    input:
                        winit::event::KeyboardInput {
                            virtual_keycode: Some(virtual_keycode),
                            state,
                            ..
                        },
                    ..
                } => {
                    match virtual_keycode {
                        winit::event::VirtualKeyCode::Escape => {
                            *control_flow = winit::event_loop::ControlFlow::Exit
                        }
                        _ => (),
                    };
                    let direction =
                        match virtual_keycode {
                            winit::event::VirtualKeyCode::S
                            | winit::event::VirtualKeyCode::Down => CameraMoveCommand::Backward,
                            winit::event::VirtualKeyCode::A
                            | winit::event::VirtualKeyCode::Left => CameraMoveCommand::Left,
                            winit::event::VirtualKeyCode::D
                            | winit::event::VirtualKeyCode::Right => CameraMoveCommand::Right,
                            winit::event::VirtualKeyCode::W | winit::event::VirtualKeyCode::Up => {
                                CameraMoveCommand::Forward
                            }
                            _ => CameraMoveCommand::None,
                        };
                    if !event_captured {
                        match state {
                            winit::event::ElementState::Pressed => {
                                camera_controller.set_command(direction)
                            }
                            winit::event::ElementState::Released => {
                                camera_controller.unset_command(direction)
                            }
                        };
                    }
                    if let Some(cmd) =
                        input_manager.process_keyboard_input(&virtual_keycode, &state)
                    {
                        app_context.run_command(cmd);
                    }
                }
                winit::event::WindowEvent::CloseRequested => {
                    *control_flow = winit::event_loop::ControlFlow::Exit;
                }
                winit::event::WindowEvent::MouseInput { button, state, .. } => {
                    if button == winit::event::MouseButton::Left {
                        camera_controller.rotation_enabled =
                            state == winit::event::ElementState::Pressed;
                    }
                }
                _ => {}
            },
            winit::event::Event::RedrawEventsCleared => {
                #[cfg(not(target_arch = "wasm32"))]
                app_context.executor.run_until_stalled();
                app_context.platform.window.request_redraw();
            }
            winit::event::Event::RedrawRequested(_) => {
                // Updates.
                #[cfg(not(target_arch = "wasm32"))]
                let (now, delta) = {
                    let now = std::time::Instant::now();
                    (now, now.duration_since(last_time).as_secs_f32())
                };
                #[cfg(target_arch = "wasm32")]
                let (now, delta) = {
                    let now = win_performance.now();
                    (now, (now - last_time) as f32)
                };
                last_time = now;

                let frame = surface
                    .get_current_texture()
                    .expect("Failed to acquire next swap chain texture");
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let (camera_right, camera_up) = camera_controller.update(delta);

                let mut encoder = app_context
                    .platform
                    .device
                    .inner()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                let renderer = &mut app_context.renderer;
                renderer.update_camera(camera_controller.origin, camera_right, camera_up);
                if !app_context.settings.accumulate || !camera_controller.is_static() {
                    renderer.reset_accumulation();
                }
                renderer.raytrace(&mut encoder, &app_context.platform.queue);
                renderer.blit(&mut encoder, &view);
                renderer.accumulate = true;

                let mut encoder_gui = app_context.platform.device.inner().create_command_encoder(
                    &wgpu::CommandEncoderDescriptor {
                        label: Some("encoder-gui"),
                    },
                );
                // Render GUI.
                app_context
                    .gui
                    .windows
                    .performance_info_window
                    .set_global_performance(delta);

                let gui_cmd_buffers = app_context.gui.render(
                    &mut app_context.settings,
                    &app_context.platform,
                    &app_context.executor,
                    &app_context.event_loop_proxy,
                    &surface_config,
                    &mut encoder_gui,
                    &view,
                );

                app_context.platform.queue.submit(
                    std::iter::once(encoder.finish()).chain(
                        gui_cmd_buffers
                            .into_iter()
                            .chain(std::iter::once(encoder_gui.finish())),
                    ),
                );

                frame.present();
            }
            _ => {}
        }
    });
}

async fn setup() -> (winit::event_loop::EventLoop<Event>, Plaftorm) {
    let event_loop: winit::event_loop::EventLoop<Event> =
        winit::event_loop::EventLoop::with_user_event();
    let mut builder = winit::window::WindowBuilder::new();
    builder = builder.with_title("Albedo Pathtracer");

    let window = builder.build(&event_loop).unwrap();

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{prelude::*, JsCast};
        use winit::platform::web::WindowExtWebSys;

        let canvas = window.canvas();
        canvas.set_width(800);
        canvas.set_height(800);
        canvas.style().set_property("width", "800px").unwrap();
        canvas.style().set_property("height", "800px").unwrap();

        // On wasm, append the canvas to the document body
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.body())
            .and_then(|body| body.append_child(&web_sys::Element::from(canvas)).ok())
            .expect("couldn't append canvas to document body");
    }

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

    (
        event_loop,
        Plaftorm {
            instance,
            adapter,
            device: Device::new(device),
            window,
            surface,
            queue,
            size,
        },
    )
}

fn main() {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{prelude::*, JsCast};

        console_log::init_with_level(log::Level::Error).expect("could not initialize logger");
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));

        wasm_bindgen_futures::spawn_local(async move {
            let setup = setup().await;
            let start_closure = Closure::once_into_js(move || run(setup));

            // make sure to handle JS exceptions thrown inside start.
            // Otherwise wasm_bindgen_futures Queue would break and never handle any tasks again.
            // This is required, because winit uses JS exception for control flow to escape from `run`.
            if let Err(error) = call_catch(&start_closure) {
                let is_control_flow_exception =
                    error.dyn_ref::<js_sys::Error>().map_or(false, |e| {
                        e.message().includes("Using exceptions for control flow", 0)
                    });

                if !is_control_flow_exception {
                    web_sys::console::error_1(&error);
                }
            }

            #[wasm_bindgen]
            extern "C" {
                #[wasm_bindgen(catch, js_namespace = Function, js_name = "prototype.call.call")]
                fn call_catch(this: &JsValue) -> Result<(), JsValue>;
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let setup = pollster::block_on(setup());
        run(setup);
    };
}
