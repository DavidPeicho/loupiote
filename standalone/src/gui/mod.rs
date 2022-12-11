use egui_winit;

use albedo_lib::{load_gltf, GLTFLoaderOptions};
use albedo_rtx::uniforms;

use crate::{errors::Error, ApplicationContext, Event, LoadEvent};

mod toolbar;
mod views;
mod windows;

pub struct Windows {
    pub scene_info_window: windows::SceneInfoWindow,
    pub performance_info_window: windows::PerformanceInfoWindow,
}

pub struct GUI {
    platform: egui_winit::State,
    context: egui::Context,
    renderer: egui_wgpu::Renderer,
    error_window: Option<windows::ErrorWindow>,
    captured: bool,
    pub windows: Windows,
}

impl GUI {
    pub fn new(
        context: &ApplicationContext,
        event_loop: &winit::event_loop::EventLoop<Event>,
        surface_config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        GUI {
            platform: egui_winit::State::new(event_loop),
            context: Default::default(),
            renderer: egui_wgpu::Renderer::new(
                context.platform.device.inner(),
                surface_config.format,
                None,
                1,
            ),
            captured: false,
            error_window: None,
            windows: Windows {
                scene_info_window: windows::SceneInfoWindow::new(),
                performance_info_window: windows::PerformanceInfoWindow::new(),
            },
        }
    }

    pub fn resize(&mut self, context: &ApplicationContext) {
        self.platform
            .set_pixels_per_point(context.platform.window.scale_factor() as f32);
    }

    pub fn handle_event<T>(&mut self, winit_event: &winit::event::Event<T>) -> bool {
        use winit::event::*;
        match winit_event {
            Event::WindowEvent { event, .. } => {
                let consumed = self.platform.on_event(&self.context, &event).consumed;
                self.captured = match event {
                    WindowEvent::CursorMoved { .. } => self.context.wants_pointer_input(),
                    _ => consumed,
                };
            }
            _ => (),
        };
        self.captured
    }

    pub fn render(
        &mut self,
        context: &mut crate::ApplicationContext,
        surface_config: &wgpu::SurfaceConfiguration,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) -> Vec<wgpu::CommandBuffer> {
        let windows = &mut self.windows;
        let raw_inputs = self.platform.take_egui_input(&context.platform.window);
        let egui::FullOutput {
            shapes,
            textures_delta,
            platform_output,
            ..
        } = self.context.run(raw_inputs, |egui_ctx| {
            render_menu_bar(egui_ctx, windows, context).unwrap();
            windows.scene_info_window.render(egui_ctx);
            windows.performance_info_window.render(egui_ctx);
        });

        self.platform.handle_platform_output(
            &context.platform.window,
            &self.context,
            platform_output,
        );

        // self.error_window = Some(ErrorWindow::new(error.into()));
        // if let Some(error_window) = &mut self.error_window {
        //     error_window.render(&self.context);
        //     if !error_window.open {
        //         self.error_window = None;
        //     }
        // }

        let platform = &context.platform;
        let screen_descriptor = egui_wgpu::renderer::ScreenDescriptor {
            size_in_pixels: [surface_config.width, surface_config.height],
            pixels_per_point: platform.window.scale_factor() as f32,
        };
        let paint_jobs = self.context.tessellate(shapes);

        let user_cmd_bufs = {
            for (id, image_delta) in &textures_delta.set {
                self.renderer.update_texture(
                    &platform.device.inner(),
                    &platform.queue,
                    *id,
                    image_delta,
                );
            }
            self.renderer.update_buffers(
                &platform.device.inner(),
                &platform.queue,
                encoder,
                &paint_jobs,
                &screen_descriptor,
            )
        };

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
                label: Some("egui main render pass"),
            });
            rpass.push_debug_group("egui_pass");
            self.renderer
                .render(&mut rpass, &paint_jobs, &screen_descriptor);
        }
        {
            for id in &textures_delta.free {
                self.renderer.free_texture(id);
            }
        }
        user_cmd_bufs
    }

    pub fn captured(&self) -> bool {
        self.captured
    }
}

fn render_menu_bar(
    context: &egui::Context,
    windows: &mut Windows,
    app_context: &mut crate::ApplicationContext,
) -> Result<(), Error> {
    use egui::*;
    let mut result = Ok(());
    TopBottomPanel::top("menu_bar").show(context, |ui| {
        trace!(ui);
        menu::bar(ui, |ui| {
            let menu_res = render_file_menu(ui, app_context);
            ui.menu_button("Windows", |ui| {
                if ui.button("Scene Information").clicked() {
                    windows.scene_info_window.open = true;
                    ui.ctx().memory().reset_areas();
                }
                if ui.button("Performance Information").clicked() {
                    windows.performance_info_window.open = true;
                    ui.ctx().memory().reset_areas();
                }
            });
            toolbar::render_toolbar_gui(ui, app_context);
            let screenshot_res = render_screenshot_menu(ui, app_context);
        });
    });
    result
}

fn render_file_menu(
    ui: &mut egui::Ui,
    context: &mut crate::ApplicationContext,
) -> Result<(), Error> {
    let platform = &context.platform;
    ui.menu_button("File", |ui| {
        if ui.button("Load").clicked() {
            ui.ctx().memory().reset_areas();

            let dialog = rfd::AsyncFileDialog::new()
                .set_parent(&platform.window)
                .pick_file();
            let event_loop_proxy = context.event_loop_proxy.clone();
            context.executor.spawn_local(async move {
                let handle = dialog.await;
                if let Some(file) = handle {
                    let data = file.read().await;
                    let event = if file.file_name().ends_with("glb") {
                        Event::Load(LoadEvent::GLTF(data))
                    } else {
                        Event::Load(LoadEvent::Env(data))
                    };
                    // @todo: support wasm.
                    event_loop_proxy.send_event(event).ok();
                }
            });
        }
    });
    Ok(())
}

fn render_screenshot_menu(
    ui: &mut egui::Ui,
    context: &mut crate::ApplicationContext,
) -> Result<(), Error> {
    let platform = &context.platform;
    // @todo: support wasm.
    #[cfg(not(target_arch = "wasm32"))]
    if ui.button("📷").clicked() {
        let dialog = rfd::AsyncFileDialog::new()
            .add_filter("image", &["png", "jpg"])
            .set_parent(&platform.window)
            .save_file();
        let event_loop_proxy = context.event_loop_proxy.clone();
        context.executor.spawn_local(async move {
            let handle = dialog.await;
            if let Some(file) = handle {
                // @todo: support wasm.
                #[cfg(not(target_arch = "wasm32"))]
                event_loop_proxy
                    .send_event(Event::SaveScreenshot(file.path().to_path_buf()))
                    .ok();
            }
        });
    }
    Ok(())
}
