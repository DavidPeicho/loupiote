use std::{
    path::{self, PathBuf},
    sync::Arc,
    time::Instant,
};

use albedo_lib::{
    load_gltf, BlitMode, Device, GLTFLoaderOptions, ProbeGPU, Renderer, Scene, SceneGPU,
};
use image::GenericImageView;
use winit::{
    application::ApplicationHandler,
    keyboard::{Key, NamedKey},
};

use crate::{
    camera::{CameraController, CameraMoveCommand},
    commands,
    errors::Error,
    event::LoadEvent,
    gui::{GUIContext, GUI},
    input_manager::InputManager,
    logger::log,
    Event, Settings, Spawner,
};

pub struct Plaftorm {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Device,
    pub window: Arc<winit::window::Window>,
    pub surface: wgpu::Surface<'static>,
    pub queue: wgpu::Queue,
    pub surface_config: wgpu::SurfaceConfiguration,
}

pub struct ApplicationContext {
    pub platform: Plaftorm,
    pub event_loop_proxy: crate::EventLoopProxy,
    #[cfg(not(target_arch = "wasm32"))]
    pub executor: Spawner<'static>,
    #[cfg(target_arch = "wasm32")]
    pub executor: Spawner,
    pub renderer: Renderer,
    pub scene: Scene,
    pub scene_gpu: SceneGPU,
    pub probe: Option<ProbeGPU>,
    pub settings: Settings,
    pub gui: GUI,

    pub camera_controller: CameraController,
    pub input_manager: InputManager,

    pub last_time: Instant,
    pub event_captured: bool,

    pub shader_paths: PathBuf,
}

impl ApplicationContext {
    pub fn init(&mut self) {
        self.settings.blit_mode = BlitMode::DenoisedPathrace;
        self.camera_controller = CameraController::from_origin_dir(
            glam::Vec3::new(2.0, 0.0, 2.0),
            glam::Vec3::new(-1.0, 0.0, -1.0).normalize(),
        );
    }

    pub fn run_command(&mut self, command: commands::EditorCommand) {
        match command {
            commands::EditorCommand::ToggleAccumulation => {
                self.settings.accumulate = !self.settings.accumulate
            }
        }
    }

    pub fn resize(&mut self, width_target: u32, height_target: u32) {
        let limits = &self.platform.device.inner().limits();
        let max_bytes_per_pixel = Renderer::max_ssbo_element_in_bytes();
        let max_pixels_count = limits.max_storage_buffer_binding_size / max_bytes_per_pixel;

        let pixels_target_count = width_target * height_target;
        let (width, height) = if max_pixels_count < pixels_target_count {
            let ratio = max_pixels_count as f32 / pixels_target_count as f32;
            (
                (width_target as f32 * ratio) as u32,
                (height_target as f32 * ratio) as u32,
            )
        } else {
            (width_target, height_target)
        };

        self.renderer.resize(
            &self.platform.device,
            &self.scene_gpu,
            self.probe.as_ref(),
            (width, height),
        );

        let dpi = self.platform.window.scale_factor() as f32;
        self.gui.resize(dpi);

        let renderer_size = self.renderer.get_size();

        log!(
            "Resize: {{\n\tDpi={:?}\n\tSurface Size=({:?}, {:?})\n\tTarget Size=({:?}, {:?})\n}}",
            dpi,
            width_target,
            height_target,
            renderer_size.0,
            renderer_size.1,
        );
    }

    pub fn load_blue_noise<P: AsRef<path::Path>>(&mut self, path: P) {
        // @todo: Remove unwrap.
        let img = image::io::Reader::open(path).unwrap().decode().unwrap();

        let (width, height) = img.dimensions();
        let bytes_per_row = width * img.color().bytes_per_pixel() as u32;

        let bytes = img.into_bytes();
        self.renderer.upload_noise_texture(
            &self.platform.device,
            &self.platform.queue,
            &bytes,
            width,
            height,
            bytes_per_row,
        );
    }

    pub fn load_env_path<P: AsRef<path::Path>>(&mut self, path: P) {
        let bytes = std::fs::read(path).unwrap();
        self.load_env(&bytes[..]);
    }

    pub fn load_env(&mut self, data: &[u8]) {
        let decoder = image::codecs::hdr::HdrDecoder::new(data).unwrap();
        let metadata = decoder.metadata();
        let image_data = decoder.read_image_native().unwrap();
        let image_data_raw = unsafe {
            std::slice::from_raw_parts(
                image_data.as_ptr() as *const u8,
                image_data.len() * std::mem::size_of::<image::codecs::hdr::Rgbe8Pixel>(),
            )
        };
        self.probe = Some(ProbeGPU::new(
            self.platform.device.inner(),
            &self.platform.queue,
            image_data_raw,
            metadata.width,
            metadata.height,
        ));
        self.renderer
            .set_resources(&self.platform.device, &self.scene_gpu, self.probe.as_ref());

        log!("Environment: {{");
        log!("\tWidth = {}", metadata.width);
        log!("\tHeight = {}", metadata.height);
        log!("}}");
    }

    pub fn load_file_path<P: AsRef<path::Path>>(&mut self, path: P) -> Result<(), Error> {
        let bytes = std::fs::read(path).unwrap();
        self.load_file(&bytes[..])
    }

    pub fn load_file(&mut self, data: &[u8]) -> Result<(), Error> {
        log!("Loading GLB...");
        let limits = &self.platform.device.inner().limits();
        let scene = load_gltf(
            data,
            &GLTFLoaderOptions {
                atlas_max_size: limits.max_texture_dimension_1d,
            },
        )?;
        log!("GLB loaded!");
        self.scene = scene;
        self.scene_gpu = SceneGPU::new_from_scene(
            &self.scene,
            self.platform.device.inner(),
            &self.platform.queue,
        );

        // Update GUI information.
        self.gui
            .windows
            .scene_info_window
            .set_meshes_count(self.scene.blas.entries.len());
        self.gui
            .windows
            .scene_info_window
            .set_bvh_nodes_count(self.scene.blas.nodes.len());

        if let Some(atlas) = &self.scene.atlas {
            log!("Texture Atlas: {{");
            log!("\tTextures count = {}", atlas.textures().len());
            log!("\tLayers count = {}", atlas.layer_count());
            log!("}}");
        }

        self.renderer
            .set_resources(&self.platform.device, &self.scene_gpu, self.probe.as_ref());
        Ok(())
    }

    pub fn save_screenshot<P: AsRef<path::Path>>(&self, path: P) {
        // @todo: Doesn't work anymore because executed async.
        let size = self.renderer.get_size();
        // @todo: handle error.
        let bytes = pollster::block_on(
            self.renderer
                .read_pixels(self.platform.device.inner(), &self.platform.queue),
        )
        .unwrap();
        if let Some(output) =
            image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(size.0, size.1, &bytes[..])
        {
            // @todo: handle error.
            output.save(path).unwrap();
        }
    }

    pub fn width(&self) -> u32 {
        self.renderer.get_size().0
    }

    pub fn height(&self) -> u32 {
        self.renderer.get_size().0
    }

    pub fn reload_shaders(&mut self) {
        let Some(s) = self.shader_paths.to_str() else {
            return;
        };
        log!("Reloading shaders {}", s);
        self.renderer
            .reload_shaders(&self.platform.device, &self.shader_paths);
    }
}

impl ApplicationHandler<crate::Event> for ApplicationContext {
    fn resumed(&mut self, _: &winit::event_loop::ActiveEventLoop) {
        self.platform.window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        self.gui.handle_event(&self.platform.window, &event);
        self.event_captured = self.gui.captured();

        match event {
            winit::event::WindowEvent::RedrawRequested => {
                // Updates.
                #[cfg(not(target_arch = "wasm32"))]
                let (now, delta) = {
                    let now = std::time::Instant::now();
                    (now, now.duration_since(self.last_time).as_secs_f32())
                };
                #[cfg(target_arch = "wasm32")]
                let (now, delta) = {
                    let win_performance = web_sys::window()
                        .unwrap()
                        .performance()
                        .expect("performance should be available");
                    let now = win_performance.now();
                    (now, ((now - last_time) / 1000.0) as f32)
                };
                self.last_time = now;

                let frame = self
                    .platform
                    .surface
                    .get_current_texture()
                    .expect("Failed to acquire next swap chain texture");
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let timestamp_period = self.platform.queue.get_timestamp_period();

                let view_transform = self.camera_controller.update(delta);

                let mut encoder = self
                    .platform
                    .device
                    .inner()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                let renderer = &mut self.renderer;
                renderer.queries.start_frame(timestamp_period);

                if !self.settings.accumulate || !self.camera_controller.is_static() {
                    renderer.reset_accumulation(&self.platform.queue);
                }

                // TODO: Can be done only on change
                renderer.use_noise_texture(&self.platform.queue, self.settings.use_blue_noise);
                renderer.set_blit_mode(self.settings.blit_mode);

                renderer.raytrace(&mut encoder, &self.platform.queue, &view_transform);
                renderer.blit(&self.platform.device, &mut encoder, &view);
                renderer.accumulate = true;

                let performance = &mut self.gui.windows.performance_info_window;
                performance.set_global_performance(delta);

                self.gui.render(
                    &mut encoder,
                    &mut GUIContext {
                        platform: &self.platform,
                        executor: &self.executor,
                        event_loop_proxy: &self.event_loop_proxy,
                        renderer: renderer,
                        settings: &mut self.settings,
                    },
                    &view,
                );

                self.platform
                    .queue
                    .submit(std::iter::once(encoder.finish()));

                frame.present();

                renderer.queries.end_frame(timestamp_period);

                self.platform.window.request_redraw();
            }
            winit::event::WindowEvent::Resized(size) => {
                // winit bug
                if size.width == u32::MAX || size.height == u32::MAX {
                    return;
                }

                self.platform.surface_config.width = size.width.max(1);
                self.platform.surface_config.height = size.height.max(1);
                self.platform
                    .surface
                    .configure(self.platform.device.inner(), &self.platform.surface_config);
                self.resize(
                    self.platform.surface_config.width,
                    self.platform.surface_config.height,
                );
            }
            winit::event::WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        logical_key, state, ..
                    },
                ..
            } => {
                match logical_key {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
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
                if !self.event_captured {
                    match state {
                        winit::event::ElementState::Pressed => {
                            self.camera_controller.set_command(direction)
                        }
                        winit::event::ElementState::Released => {
                            self.camera_controller.unset_command(direction)
                        }
                    };
                }
                if let Some(cmd) = self
                    .input_manager
                    .process_keyboard_input(&logical_key, &state)
                {
                    self.run_command(cmd);
                }
            }
            winit::event::WindowEvent::MouseInput { button, state, .. } => {
                if button == winit::event::MouseButton::Left {
                    self.camera_controller.rotation_enabled =
                        state == winit::event::ElementState::Pressed;
                }
            }
            _ => {}
        }
    }

    fn new_events(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        cause: winit::event::StartCause,
    ) {
        let _ = (event_loop, cause);
    }

    fn user_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, event: crate::Event) {
        match event {
            Event::SaveScreenshot(path) => self.save_screenshot(path),
            Event::ReloadShaders => self.reload_shaders(),
            Event::Load(load) => match load {
                LoadEvent::GLTF(data) => self
                    .load_file(&data[..])
                    .unwrap_or_else(|_| self.gui.set_error("failed to load gltf")),
                LoadEvent::Env(data) => self.load_env(&data[..]),
            },
        }
    }

    fn device_event(
        &mut self,
        _: &winit::event_loop::ActiveEventLoop,
        _: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        match event {
            winit::event::DeviceEvent::MouseMotion { delta } => {
                if !self.event_captured {
                    self.camera_controller.rotate(
                        (delta.0 / (self.width() as f64 * 0.5)) as f32,
                        (delta.1 / (self.height() as f64 * 0.5)) as f32,
                    );
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let _ = event_loop;
    }

    fn suspended(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let _ = event_loop;
    }

    fn exiting(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let _ = event_loop;
    }

    fn memory_warning(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let _ = event_loop;
    }
}
