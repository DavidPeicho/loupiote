use egui_winit;
use winit::event::{self};

use albedo_lib::{load_gltf, GLTFLoaderOptions};
use albedo_rtx::uniforms;

use crate::errors::Error;
use crate::SceneGPU;

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
    pub fn new<T>(
        device: &wgpu::Device,
        window: &winit::window::Window,
        event_loop: &winit::event_loop::EventLoopWindowTarget<T>,
        surface_config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        GUI {
            // platform: Platform::new(PlatformDescriptor {
            //     physical_width: surface_config.width,
            //     physical_height: surface_config.height,
            //     scale_factor: window.scale_factor(),
            //     font_definitions: egui::FontDefinitions::default(),
            //     style: Default::default(),
            // }),
            // render_pass: egui_wgpu_backend::RenderPass::new(&device, surface_config.format, 1),
            platform: egui_winit::State::new(event_loop),
            context: Default::default(),
            renderer: egui_wgpu::Renderer::new(device, surface_config.format, None, 1),
            captured: false,
            error_window: None,
            windows: Windows {
                scene_info_window: windows::SceneInfoWindow::new(),
                performance_info_window: windows::PerformanceInfoWindow::new(),
            },
        }
    }

    pub fn resize(&mut self, app_context: &crate::ApplicationContext) {
        self.platform
            .set_pixels_per_point(app_context.window.scale_factor() as f32);
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
        app_context: &mut crate::ApplicationContext,
        renderer: &mut crate::Renderer,
        surface_config: &wgpu::SurfaceConfiguration,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        delta: f64,
    ) -> Vec<wgpu::CommandBuffer> {
        let windows = &mut self.windows;
        let raw_inputs = self.platform.take_egui_input(&app_context.window);
        let egui::FullOutput {
            shapes,
            textures_delta,
            platform_output,
            ..
        } = self.context.run(raw_inputs, |egui_ctx| {
            render_menu_bar(egui_ctx, windows, app_context, renderer).unwrap();
            windows.scene_info_window.render(egui_ctx);
            windows.performance_info_window.render(egui_ctx);
        });

        self.platform
            .handle_platform_output(&app_context.window, &self.context, platform_output);

        // self.error_window = Some(ErrorWindow::new(error.into()));
        // if let Some(error_window) = &mut self.error_window {
        //     error_window.render(&self.context);
        //     if !error_window.open {
        //         self.error_window = None;
        //     }
        // }

        let screen_descriptor = egui_wgpu::renderer::ScreenDescriptor {
            size_in_pixels: [surface_config.width, surface_config.height],
            pixels_per_point: app_context.window.scale_factor() as f32,
        };
        let paint_jobs = self.context.tessellate(shapes);

        let user_cmd_bufs = {
            for (id, image_delta) in &textures_delta.set {
                self.renderer.update_texture(
                    &app_context.device.inner(),
                    &app_context.queue,
                    *id,
                    image_delta,
                );
            }
            self.renderer.update_buffers(
                &app_context.device.inner(),
                &app_context.queue,
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
    renderer: &mut crate::Renderer,
) -> Result<(), Error> {
    use egui::*;
    let mut result = Ok(());
    TopBottomPanel::top("menu_bar").show(context, |ui| {
        trace!(ui);
        menu::bar(ui, |ui| {
            let menu_res = render_file_menu(ui, app_context, renderer);
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
            let screenshot_res = render_screenshot_menu(ui, app_context, renderer);
        });
    });
    result
}

fn render_file_menu(
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
        scene.lights = vec![uniforms::Light::from_matrix(
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
        renderer.set_resources(&app_context.device, &app_context.scene_gpu);
    }
    Ok(())
}

fn render_screenshot_menu(
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
