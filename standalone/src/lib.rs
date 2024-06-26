use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use winit::{
    self,
    event_loop::EventLoopWindowTarget,
    keyboard::{Key, NamedKey},
};

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

mod logger;
use logger::log;

mod input_manager;
use input_manager::InputManager;

mod gui;

mod camera;
use camera::CameraMoveCommand;

use crate::gui::GUIContext;

pub fn run((event_loop, platform): (winit::event_loop::EventLoop<Event>, Plaftorm)) {
    let event_loop_proxy = event_loop.create_proxy();

    log!("\n============================================================");
    log!("                   🚀 Albedo Pathtracer 🚀                   ");
    log!("============================================================\n");

    let init_size = platform.window.inner_size();

    let caps = platform.surface.get_capabilities(&platform.adapter);
    let swapchain_format = caps.formats[0];
    let mut surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: swapchain_format,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        width: init_size.width,
        height: init_size.height,
        desired_maximum_frame_latency: 2,
        #[cfg(target_arch = "wasm32")]
        present_mode: wgpu::PresentMode::Fifo,
        #[cfg(not(target_arch = "wasm32"))]
        present_mode: wgpu::PresentMode::Immediate,
        view_formats: vec![],
    };
    surface_config.width = init_size.width;
    surface_config.height = init_size.height;
    platform
        .surface
        .configure(platform.device.inner(), &surface_config);

    let mut gui = gui::GUI::new(&platform.window, &platform.device.inner(), &surface_config);

    let mut camera_controller = camera::CameraController::from_origin_dir(
        glam::Vec3::new(0.0, 0.0, 5.0),
        glam::Vec3::new(0.0, 0.0, -1.0),
    );
    camera_controller.rotation_enabled = false;

    let renderer = Renderer::new(
        &platform.device,
        (init_size.width, init_size.height),
        swapchain_format,
    );

    let scene = Scene::default();
    let scene_gpu = SceneGPU::new_from_scene(&scene, platform.device.inner(), &platform.queue);

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
        renderer,
        gui,
        settings: Settings::new(),
    };

    app_context.resize(init_size.width, init_size.height);

    app_context.load_blue_noise("./assets/noise_rgb.png");
    app_context
        .renderer
        .use_noise_texture(&app_context.platform.queue, true);

    #[cfg(not(target_arch = "wasm32"))]
    {
        app_context.load_env_path("./assets/uffizi-large.hdr");
        app_context
            .load_file_path("./assets/DamagedHelmet.glb")
            .unwrap();
    }

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
    let _ = winit::event_loop::EventLoop::run(
        event_loop,
        move |event, target: &EventLoopWindowTarget<Event>| {
            app_context
                .gui
                .handle_event(&app_context.platform.window, &event);
            let event_captured = app_context.gui.captured();
            match event {
                winit::event::Event::UserEvent(event) => app_context.event(event),
                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::Resized(size),
                    ..
                } => {
                    // winit bug
                    if size.width == u32::MAX || size.height == u32::MAX {
                        return;
                    }
                    let width = size.width.max(1);
                    let height = size.height.max(1);
                    surface_config.width = width;
                    surface_config.height = height;
                    app_context
                        .platform
                        .surface
                        .configure(app_context.platform.device.inner(), &surface_config);
                    app_context.resize(width, height);
                }

                winit::event::Event::DeviceEvent { event, .. } => match event {
                    winit::event::DeviceEvent::MouseMotion { delta } => {
                        if !event_captured {
                            camera_controller.rotate(
                                (delta.0 / (app_context.width() as f64 * 0.5)) as f32,
                                (delta.1 / (app_context.height() as f64 * 0.5)) as f32,
                            );
                        }
                    }
                    _ => {}
                },

                winit::event::Event::WindowEvent { event, .. } => match event {
                    winit::event::WindowEvent::KeyboardInput {
                        event:
                            winit::event::KeyEvent {
                                logical_key, state, ..
                            },
                        ..
                    } => {
                        match logical_key {
                            Key::Named(NamedKey::Escape) => target.exit(),
                            _ => (),
                        };
                        let direction = match logical_key.as_ref() {
                            Key::Character("s") | Key::Named(NamedKey::ArrowDown) => {
                                CameraMoveCommand::Backward
                            }

                            Key::Character("a") | Key::Named(NamedKey::ArrowLeft) => {
                                CameraMoveCommand::Left
                            }

                            Key::Character("d") | Key::Named(NamedKey::ArrowRight) => {
                                CameraMoveCommand::Right
                            }

                            Key::Character("w") | Key::Named(NamedKey::ArrowUp) => {
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
                            input_manager.process_keyboard_input(&logical_key, &state)
                        {
                            app_context.run_command(cmd);
                        }
                    }
                    winit::event::WindowEvent::CloseRequested => target.exit(),
                    winit::event::WindowEvent::MouseInput { button, state, .. } => {
                        if button == winit::event::MouseButton::Left {
                            camera_controller.rotation_enabled =
                                state == winit::event::ElementState::Pressed;
                        }
                    }
                    winit::event::WindowEvent::RedrawRequested => {
                        // Updates.
                        #[cfg(not(target_arch = "wasm32"))]
                        let (now, delta) = {
                            let now = std::time::Instant::now();
                            (now, now.duration_since(last_time).as_secs_f32())
                        };
                        #[cfg(target_arch = "wasm32")]
                        let (now, delta) = {
                            let now = win_performance.now();
                            (now, ((now - last_time) / 1000.0) as f32)
                        };
                        last_time = now;

                        let frame = app_context
                            .platform
                            .surface
                            .get_current_texture()
                            .expect("Failed to acquire next swap chain texture");
                        let view = frame
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());
                        let timestamp_period = app_context.platform.queue.get_timestamp_period();

                        let (camera_right, camera_up) = camera_controller.update(delta);

                        let mut encoder =
                            app_context.platform.device.inner().create_command_encoder(
                                &wgpu::CommandEncoderDescriptor { label: None },
                            );

                        let renderer = &mut app_context.renderer;
                        renderer.queries.start_frame(timestamp_period);

                        renderer.update_camera(camera_controller.origin, camera_right, camera_up);
                        if !app_context.settings.accumulate || !camera_controller.is_static() {
                            renderer.reset_accumulation(&app_context.platform.queue);
                        }

                        // TODO: Can be done only on change
                        renderer.use_noise_texture(&app_context.platform.queue, app_context.settings.use_blue_noise);

                        // Debug the lightmapper. We need to reset the frame
                        // renderer.reset_accumulation(&app_context.platform.queue);
                        // renderer.lightmap(
                        //     &mut encoder,
                        //     &app_context.scene_gpu,
                        // );
                        renderer.raytrace(&mut encoder, &app_context.platform.queue);
                        renderer.blit(&mut encoder, &view);
                        renderer.accumulate = true;

                        let mut encoder_gui =
                            app_context.platform.device.inner().create_command_encoder(
                                &wgpu::CommandEncoderDescriptor {
                                    label: Some("encoder-gui"),
                                },
                            );
                        // Render GUI.
                        let performance = &mut app_context.gui.windows.performance_info_window;
                        performance.set_global_performance(delta);

                        let gui_cmd_buffers: Vec<wgpu::CommandBuffer> = app_context.gui.render(
                            &mut encoder_gui,
                            &mut GUIContext {
                                platform: &app_context.platform,
                                executor: &app_context.executor,
                                event_loop_proxy: &app_context.event_loop_proxy,
                                renderer: renderer,
                                surface_config: &surface_config,
                                settings: &mut app_context.settings,
                            },
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

                        renderer.queries.end_frame(timestamp_period);

                        app_context.platform.window.request_redraw();
                    }
                    _ => {}
                },
                // winit::event::Event::RedrawEventsCleared => {
                //     #[cfg(not(target_arch = "wasm32"))]
                //     app_context.executor.run_until_stalled();
                //     app_context.platform.window.request_redraw();
                // }
                _ => {}
            }
        },
    );
}

pub async fn setup() -> (winit::event_loop::EventLoop<Event>, Plaftorm) {
    let event_loop: winit::event_loop::EventLoop<Event> =
        winit::event_loop::EventLoopBuilder::with_user_event()
            .build()
            .unwrap();
    let mut builder = winit::window::WindowBuilder::new();
    builder = builder.with_title("Albedo Pathtracer");

    let window = builder.build(&event_loop).unwrap();

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{prelude::*, JsCast};
        use winit::platform::web::WindowExtWebSys;

        let canvas = window.canvas();

        // On wasm, append the canvas to the document body
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.body())
            .and_then(|body| body.append_child(&web_sys::Element::from(canvas)).ok())
            .expect("couldn't append canvas to document body");
    }

    let backends = wgpu::util::backend_bits_from_env().unwrap_or_default();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        flags: wgpu::InstanceFlags::from_build_config().with_env(),
        dx12_shader_compiler: wgpu::Dx12Compiler::default(),
        gles_minor_version: wgpu::Gles3MinorVersion::default(),
    });

    let window = Arc::new(window);
    let surface = instance.create_surface(window.clone()).unwrap();

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .expect("No suitable GPU adapters found on the system!");

    let required_features: wgpu::Features =
        wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;

    let needed_limits = wgpu::Limits {
        max_storage_buffers_per_shader_stage: 8,
        max_storage_buffer_binding_size: 256 * 1024 * 1024,
        ..wgpu::Limits::default()
    };
    let trace_dir: Result<String, std::env::VarError> = std::env::var("WGPU_TRACE");

    println!(
        "Adater name: {} / Backend: {:?}",
        adapter.get_info().name,
        adapter.get_info().backend
    );

    let features = adapter.features();
    if features.contains(wgpu::Features::TIMESTAMP_QUERY) {
        log!("Adapter supports timestamp queries.");
    } else {
        log!("Adapter does not support timestamp queries.");
    }
    let timestamps_inside_passes = features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES);
    if timestamps_inside_passes {
        log!("Adapter supports timestamp queries within passes.");
    } else {
        log!("Adapter does not support timestamp queries within passes.");
    }

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: features | required_features,
                required_limits: needed_limits,
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
        },
    )
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
/// It works so well it's almost sad for Emscripten
pub fn main_wasm() {
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
            let is_control_flow_exception = error.dyn_ref::<js_sys::Error>().map_or(false, |e| {
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
