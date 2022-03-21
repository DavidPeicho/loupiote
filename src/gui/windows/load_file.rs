use albedo_rtx::renderer::resources;

use crate::errors::Error;
use crate::gltf_loader::ProxyMesh;
use crate::Scene;
use crate::{gltf_loader, SceneGPU};

pub struct LoadFileWindow {
    pub open: bool,
    pub content: String,
}

impl LoadFileWindow {
    pub fn new() -> Self {
        LoadFileWindow {
            open: false,
            content: String::new(),
        }
    }

    pub fn render(
        &mut self,
        context: &egui::Context,
        app_context: &mut crate::ApplicationContext,
        renderer: &mut crate::Renderer,
    ) -> Result<(), Error> {
        let Self { open, content } = self;
        let mut result: Option<Result<Scene<ProxyMesh>, Error>> = None;
        egui::Window::new("Load Scene")
            .open(open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(content).hint_text("Path to file"));
                    if ui.button("Open").clicked() {
                        result = Some(gltf_loader::load_gltf(content));
                    }
                });
            });

        if let Some(res) = result {
            let mut scene = res?;
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
                SceneGPU::new_from_scene(&scene, &app_context.device, &app_context.queue);
            app_context.scene_gpu.probe_texture = probe_tex;
            app_context.scene_gpu.probe_texture_view = probe_tex_view;
            app_context.scene = scene;
            renderer.set_resources(&app_context.device, &app_context.scene_gpu);
        }
        Ok(())
    }
}
