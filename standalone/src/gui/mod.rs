use crate::{Event, LoadEvent};

mod toolbar;
mod views;
mod windows;

pub struct Windows {
    pub scene_info_window: windows::SceneInfoWindow,
    pub performance_info_window: windows::PerformanceInfoWindow,
}

pub struct GUIContext<'a> {
    pub platform: &'a crate::Plaftorm,
    pub executor: &'a crate::Spawner<'static>,
    pub event_loop_proxy: &'a crate::EventLoopProxy,
    pub renderer: &'a mut crate::Renderer,
    pub settings: &'a mut crate::Settings,
}

pub struct GUI {
    platform: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    error_window: Option<windows::ErrorWindow>,
    captured: bool,
    pub windows: Windows,
}

impl GUI {
    pub fn new(
        window: &winit::window::Window,
        device: &wgpu::Device,
        surface_config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        // Create the egui context
        // Create the winit/egui integration.
        let platform = egui_winit::State::new(
            egui::Context::default(),
            egui::ViewportId::default(),
            &window,
            Some(window.scale_factor() as f32),
            None,
            None
        );
        GUI {
            platform,
            renderer: egui_wgpu::Renderer::new(device, surface_config.format, None, 1, true),
            captured: false,
            error_window: None,
            windows: Windows {
                scene_info_window: windows::SceneInfoWindow::new(),
                performance_info_window: windows::PerformanceInfoWindow {
                    open: false,
                    ..Default::default()
                },
            },
        }
    }

    pub fn set_error<S: Into<String>>(&mut self, message: S) {
        self.error_window = Some(windows::ErrorWindow::new(message.into()));
    }

    pub fn resize(&mut self, _: f32) {}

    pub fn handle_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) -> bool {
        use winit::event::*;
        match event {
            winit::event::WindowEvent::Resized(size) => {
                // winit bug
                if size.width == u32::MAX || size.height == u32::MAX {
                    return false;
                }
            }
            _ => (),
        }
        let consumed = self.platform.on_window_event(window, &event).consumed;
        self.captured = match event {
            WindowEvent::CursorMoved { .. } => {
                self.platform.egui_ctx().wants_pointer_input()
            }
            _ => consumed,
        };
        self.captured
    }

    pub fn render<'a, 'b>(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        context: &mut GUIContext,
        view: &wgpu::TextureView,
    ) -> Vec<wgpu::CommandBuffer> {
        let inputs = self.platform.take_egui_input(&context.platform.window);
        self.platform.egui_ctx().begin_frame(inputs);

        let ctx = self.platform.egui_ctx();

        let windows = &mut self.windows;
        render_menu_bar(ctx, context, windows);
        windows.scene_info_window.render(ctx);
        windows.performance_info_window.render(&context, ctx);

        let pixels_per_point = context.platform.window.scale_factor() as f32;
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [context.platform.surface_config.width, context.platform.surface_config.height],
            pixels_per_point,
        };

        let egui::FullOutput {
            shapes,
            textures_delta,
            ..
        } = ctx.end_frame();

        let paint_jobs = ctx.tessellate(shapes, pixels_per_point);

        if let Some(error_window) = &mut self.error_window {
            error_window.render(ctx);
            if !error_window.open {
                self.error_window = None;
            }
        }

        let user_cmd_bufs = {
            for (id, image_delta) in &textures_delta.set {
                self.renderer.update_texture(
                    context.platform.device.inner(),
                    &context.platform.queue,
                    *id,
                    image_delta,
                );
            }
            self.renderer.update_buffers(
                &context.platform.device.inner(),
                &context.platform.queue,
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
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                label: Some("egui main render pass"),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rpass.push_debug_group("egui_pass");
            self.renderer
                .render(&mut rpass.forget_lifetime(), &paint_jobs, &screen_descriptor);
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

fn render_menu_bar(egui_ctx: &egui::Context, context: &mut GUIContext, windows: &mut Windows) {
    use egui::*;
    TopBottomPanel::top("menu_bar").show(egui_ctx, |ui| {
        menu::bar(ui, |ui| {
            render_file_menu(ui, context);
            ui.menu_button("Windows", |ui| {
                if ui.button("Scene Information").clicked() {
                    windows.scene_info_window.open = true;
                    ui.close_menu();
                }
                if ui.button("Performance Information").clicked() {
                    windows.performance_info_window.open = true;
                    ui.close_menu();
                }
            });
            toolbar::render_toolbar_gui(ui, context.settings);
            render_screenshot_menu(ui, context);
        });
    });
}

fn render_file_menu(ui: &mut egui::Ui, context: &GUIContext) {
    ui.menu_button("File", |ui| {
        if ui.button("Load").clicked() {
            ui.close_menu();

            let dialog = rfd::AsyncFileDialog::new()
                .set_parent(context.platform.window.as_ref())
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
}

fn render_screenshot_menu(ui: &mut egui::Ui, context: &GUIContext) {
    // @todo: support wasm.
    #[cfg(not(target_arch = "wasm32"))]
    if ui.button("📷").clicked() {
        let dialog = rfd::AsyncFileDialog::new()
            .add_filter("image", &["png", "jpg"])
            .set_parent(&context.platform.window)
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
}
