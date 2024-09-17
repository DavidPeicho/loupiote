use std::sync::Arc;

use camera::CameraController;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use winit::{self, event_loop::EventLoop};

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
        present_mode: wgpu::PresentMode::Fifo,
        view_formats: vec![],
    };
    surface_config.width = init_size.width;
    surface_config.height = init_size.height;
    platform
        .surface
        .configure(platform.device.inner(), &surface_config);

    let mut gui = gui::GUI::new(&platform.window, &platform.device.inner(), &surface_config);

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
        camera_controller: CameraController::new(),
        input_manager: InputManager::new(),

        last_time: std::time::Instant::now(),
        event_captured: false,
    };
    app_context.init();
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

    // let mut hotwatch = hotwatch::Hotwatch::new().expect("hotwatch failed to initialize!");
    // watch_shading_shader(&mut hotwatch, &device, &renderer);
    event_loop.run_app(&mut app_context).unwrap();
    //     event_loop,
    //     move |event, target: &EventLoopWindowTarget<Event>| {
    //         app_context
    //             .gui
    //             .handle_event(&app_context.platform.window, &event);
    //         let event_captured = app_context.gui.captured();
    //         match event {
    //             winit::event::Event::UserEvent(event) => ,
    //             winit::event::Event::WindowEvent {
    //                 event: winit::event::WindowEvent::Resized(size),
    //                 ..
    //             } => {
    //             }

    //             winit::event::Event::DeviceEvent { event, .. } => match event {
    //                 winit::event::DeviceEvent::MouseMotion { delta } => {
    //                     if !event_captured {
    //                         camera_controller.rotate(
    //                             (delta.0 / (app_context.width() as f64 * 0.5)) as f32,
    //                             (delta.1 / (app_context.height() as f64 * 0.5)) as f32,
    //                         );
    //                     }
    //                 }
    //                 _ => {}
    //             },

    //             winit::event::Event::WindowEvent { event, .. } => match event {

    //                 }
    //                 winit::event::WindowEvent::CloseRequested => target.exit(),

    //                 _ => {}
    //             },
    //             // winit::event::Event::RedrawEventsCleared => {
    //             //     #[cfg(not(target_arch = "wasm32"))]
    //             //     app_context.executor.run_until_stalled();
    //             //     app_context.platform.window.request_redraw();
    //             // }
    //             _ => {}
    //         }
    //     },
    // );
}

pub async fn setup() -> (winit::event_loop::EventLoop<Event>, Plaftorm) {
    let event_loop: EventLoop<Event> = winit::event_loop::EventLoop::with_user_event().build().unwrap();
    let window_attributes = winit::window::Window::default_attributes().with_title("Loupiote");
    let window = event_loop.create_window(window_attributes).unwrap();

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
        max_push_constant_size: 128,
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
                memory_hints: wgpu::MemoryHints::Performance
            },
            trace_dir.ok().as_ref().map(std::path::Path::new),
        )
        .await
        .expect("Unable to find a suitable GPU adapter!");

    let caps = surface.get_capabilities(&adapter);
    let swapchain_format = caps.formats[0];
    let surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: swapchain_format,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        width: 1,
        height: 1,
        desired_maximum_frame_latency: 2,
        present_mode: wgpu::PresentMode::Fifo,
        view_formats: vec![],
    };

    (
        event_loop,
        Plaftorm {
            instance,
            adapter,
            device: Device::new(device),
            window,
            surface,
            queue,
            surface_config
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
