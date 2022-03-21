use egui_winit_platform::{Platform, PlatformDescriptor};
use winit::event::{self};

use self::windows::ErrorWindow;
mod views;
mod windows;

use crate::errors::Error;

pub struct GUI {
    platform: Platform,
    render_pass: egui_wgpu_backend::RenderPass,
    is_event_handled: bool,

    error_window: Option<windows::ErrorWindow>,
    pub scene_info_window: windows::SceneInfoWindow,
    pub load_file_window: windows::LoadFileWindow,
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
            load_file_window: windows::LoadFileWindow::new(),
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
        window: &winit::window::Window,
        app_context: &mut crate::ApplicationContext,
        renderer: &mut crate::Renderer,
        surface_config: &wgpu::SurfaceConfiguration,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        delta: f64,
    ) {
        self.platform.update_time(delta);
        self.platform.begin_frame();

        let context = self.platform.context();

        if let Err(error) = self.render_gui(&context, app_context, renderer) {
            self.error_window = Some(ErrorWindow::new(error.into()));
        }
        if let Some(error_window) = &mut self.error_window {
            error_window.render(&context);
            if !error_window.open {
                self.error_window = None;
            }
        }

        let egui::FullOutput {
            textures_delta,
            shapes,
            ..
        } = self.platform.end_frame(Some(&window));
        let paint_jobs = self.platform.context().tessellate(shapes);

        // Upload all resources for the GPU.
        let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
            physical_width: surface_config.width,
            physical_height: surface_config.height,
            scale_factor: window.scale_factor() as f32,
        };

        self.render_pass
            .add_textures(&app_context.device, &app_context.queue, &textures_delta)
            .unwrap();
        self.render_pass.update_buffers(
            &app_context.device,
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
        self.render_menu_bar(context);

        self.load_file_window
            .render(context, app_context, renderer)?;
        self.scene_info_window.render(context);
        self.performance_info_window.render(context);

        Ok(())
    }

    fn render_menu_bar(&mut self, context: &egui::Context) {
        use egui::*;
        TopBottomPanel::top("menu_bar").show(context, |ui| {
            trace!(ui);
            menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Load").clicked() {
                        self.load_file_window.open = true;
                        ui.ctx().memory().reset_areas();
                    }
                });
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
            });
        });
    }
}
