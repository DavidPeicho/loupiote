use egui_winit_platform::{Platform, PlatformDescriptor};
use winit::event::{self};

use albedo_rtx::renderer::resources;

use crate::errors::Error;
use crate::gltf_loader::{load_gltf, GLTFLoaderOptions};
use crate::SceneGPU;

use self::windows::ErrorWindow;
mod views;
mod windows;

pub struct GUI {
    platform: Platform,
    render_pass: egui_wgpu_backend::RenderPass,
    is_event_handled: bool,

    error_window: Option<windows::ErrorWindow>,
    pub scene_info_window: windows::SceneInfoWindow,
    pub performance_info_window: windows::PerformanceInfoWindow,
}

impl GUI {
    pub fn new(
        device: &wgpu::Device,
        window: &winit::window::Window,
        surface_config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        GUI {
            platform: Platform::new(PlatformDescriptor {
                physical_width: surface_config.width,
                physical_height: surface_config.height,
                scale_factor: window.scale_factor(),
                font_definitions: egui::FontDefinitions::default(),
                style: Default::default(),
            }),
            render_pass: egui_wgpu_backend::RenderPass::new(&device, surface_config.format, 1),
            is_event_handled: false,

            error_window: None,
            scene_info_window: windows::SceneInfoWindow::new(),
            performance_info_window: windows::PerformanceInfoWindow::new(),
        }
    }

    pub fn handle_event<T>(&mut self, winit_event: &winit::event::Event<T>) -> bool {
        match winit_event {
            event::Event::WindowEvent { event, .. } => match event {
                event::WindowEvent::MouseInput { button, state, .. } => {
                    if *button == winit::event::MouseButton::Left {
                        self.is_event_handled = if *state == event::ElementState::Pressed {
                            self.platform.captures_event(winit_event)
                        } else {
                            false
                        }
                    }
                }
                event::WindowEvent::KeyboardInput {
                    input: event::KeyboardInput { state, .. },
                    ..
                } => {
                    self.is_event_handled = if *state == event::ElementState::Pressed {
                        self.platform.captures_event(winit_event)
                    } else {
                        false
                    }
                }
                _ => {}
            },
            _ => {}
        }
        self.platform.handle_event(winit_event);
        self.is_event_handled
    }

    pub fn render(
        &mut self,
        app_context: &mut crate::ApplicationContext,
        renderer: &mut crate::Renderer,
        surface_config: &wgpu::SurfaceConfiguration,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        delta: f64,
    ) {
        self.platform.update_time(delta);
        self.platform.begin_frame();

        if let Err(error) = self.render_gui(&self.platform.context(), app_context, renderer) {
            self.error_window = Some(ErrorWindow::new(error.into()));
        }
        if let Some(error_window) = &mut self.error_window {
            error_window.render(&self.platform.context());
            if !error_window.open {
                self.error_window = None;
            }
        }

        let egui::FullOutput {
            textures_delta,
            shapes,
            ..
        } = self.platform.end_frame(Some(&app_context.window));
        let paint_jobs = self.platform.context().tessellate(shapes);

        // Upload all resources for the GPU.
        let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
            physical_width: surface_config.width,
            physical_height: surface_config.height,
            scale_factor: app_context.window.scale_factor() as f32,
        };

        self.render_pass
            .add_textures(
                app_context.device.inner(),
                &app_context.queue,
                &textures_delta,
            )
            .unwrap();
        self.render_pass.update_buffers(
            &app_context.device.inner(),
            &app_context.queue,
            &paint_jobs,
            &screen_descriptor,
        );
        self.render_pass
            .execute(encoder, view, &paint_jobs, &screen_descriptor, None)
            .unwrap();
        self.render_pass.remove_textures(textures_delta).unwrap();
    }

    fn render_gui(
        &mut self,
        context: &egui::Context,
        app_context: &mut crate::ApplicationContext,
        renderer: &mut crate::Renderer,
    ) -> Result<(), Error> {
        self.render_menu_bar(context, app_context, renderer)?;
        self.scene_info_window.render(context);
        self.performance_info_window.render(context);

        Ok(())
    }

    fn render_menu_bar(
        &mut self,
        context: &egui::Context,
        app_context: &mut crate::ApplicationContext,
        renderer: &mut crate::Renderer,
    ) -> Result<(), Error> {
        use egui::*;
        let mut result = Ok(());
        TopBottomPanel::top("menu_bar").show(context, |ui| {
            trace!(ui);
            menu::bar(ui, |ui| {
                let menu_res = self.render_file_menu(ui, app_context, renderer);
                ui.menu_button("Windows", |ui| {
                    if ui.button("Scene Information").clicked() {
                        self.scene_info_window.open = true;
                        ui.ctx().memory().reset_areas();
                    }
                    if ui.button("Performance Information").clicked() {
                        self.performance_info_window.open = true;
                        ui.ctx().memory().reset_areas();
                    }
                });
                let screenshot_res = self.render_screenshot_menu(ui, app_context, renderer);
            });
        });
        result
    }

    fn render_file_menu(
        &mut self,
        ui: &mut egui::Ui,
        app_context: &mut crate::ApplicationContext,
        renderer: &mut crate::Renderer,
    ) -> Result<(), Error> {
        let mut file_path: Option<std::path::PathBuf> = None;
        ui.menu_button("File", |ui| {
            if ui.button("Load").clicked() {
                ui.ctx().memory().reset_areas();
                file_path = rfd::FileDialog::new()
                    .set_parent(&app_context.window)
                    .pick_file();
            }
        });
        if let Some(path) = file_path {
            let mut scene = load_gltf(
                &path,
                &GLTFLoaderOptions {
                    atlas_max_size: app_context.limits.max_texture_dimension_1d,
                },
            )?;
            // @todo: parse from file.
            scene.lights = vec![resources::LightGPU::from_matrix(
                glam::Mat4::from_scale_rotation_translation(
                    glam::Vec3::new(1.0, 1.0, 1.0),
                    glam::Quat::from_rotation_x(1.5),
                    glam::Vec3::new(0.0, 3.0, 0.75),
                ),
            )];

            let probe_tex = std::mem::take(&mut app_context.scene_gpu.probe_texture);
            let probe_tex_view = std::mem::take(&mut app_context.scene_gpu.probe_texture_view);
            app_context.scene_gpu =
                SceneGPU::new_from_scene(&scene, app_context.device.inner(), &app_context.queue);
            app_context.scene_gpu.probe_texture = probe_tex;
            app_context.scene_gpu.probe_texture_view = probe_tex_view;
            app_context.scene = scene;
            renderer.set_resources(&app_context.device.inner(), &app_context.scene_gpu);
        }
        Ok(())
    }

    fn render_screenshot_menu(
        &mut self,
        ui: &mut egui::Ui,
        app_context: &mut crate::ApplicationContext,
        renderer: &mut crate::Renderer,
    ) -> Result<(), Error> {
        let mut file_path: Option<std::path::PathBuf> = None;
        if ui.button("📷").clicked() {
            #[cfg(not(target_arch = "wasm32"))]
            {
                file_path = rfd::FileDialog::new()
                    .add_filter("image", &["png", "jpg"])
                    .set_parent(&app_context.window)
                    .save_file();
            }
        }
        if let Some(path) = file_path {
            let size = renderer.get_size();
            let bytes = pollster::block_on(
                renderer.read_pixels(app_context.device.inner(), &app_context.queue),
            )?;
            if let Some(output) =
                image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(size.0, size.1, &bytes[..])
            {
                output.save(path)?;
            }
        }
        Ok(())
    }
}
