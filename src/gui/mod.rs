use egui_winit_platform::{
  Platform,
  PlatformDescriptor
};

mod views;

mod info_window_gui;
use info_window_gui::{InfoWindowGUI};

pub struct GUI {
  platform: Platform,
  render_pass: egui_wgpu_backend::RenderPass,

  info_window: InfoWindowGUI,
}

impl GUI {

  pub fn new(
    device: &wgpu::Device,
    surface_config: &wgpu::SurfaceConfiguration
  ) -> Self {
    GUI {
      platform: Platform::new(PlatformDescriptor {
        physical_width: surface_config.width,
        physical_height: surface_config.height,
        scale_factor: 1.0,
        font_definitions: egui::FontDefinitions::default(),
        style: Default::default(),
      }),
      render_pass: egui_wgpu_backend::RenderPass::new(&device, surface_config.format, 1),

      info_window: InfoWindowGUI::new(),
    }
  }

  pub fn handle_event<T>(&mut self, winit_event: &winit::event::Event<T>) {
    self.platform.handle_event(winit_event);
  }

  pub fn update(&mut self, delta: f64) {
    self.platform.update_time(delta);
  }

  pub fn render(
    &mut self,
    window: &winit::window::Window,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface_config: &wgpu::SurfaceConfiguration,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView
  ) {
    self.platform.begin_frame();

    let egui::FullOutput {
      platform_output,
      needs_repaint,
      textures_delta,
      shapes,
    } = self.platform.end_frame(Some(&window));
    let paint_jobs = self.platform.context().tessellate(shapes);

    // Upload all resources for the GPU.
    let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
      physical_width: surface_config.width,
      physical_height: surface_config.height,
      scale_factor: window.scale_factor() as f32,
    };

    self.render_gui();

    self.render_pass.add_textures(&device, &queue, &textures_delta).unwrap();
    self.render_pass.update_buffers(&device, &queue, &paint_jobs, &screen_descriptor);
    self.render_pass.execute(
      encoder,
      view,
      &paint_jobs,
      &screen_descriptor,
      None
    ).unwrap();
  }

  pub fn info_window_mut(&mut self) -> &mut InfoWindowGUI {
    &mut self.info_window
  }

  fn render_gui(&mut self) {
    self.render_menu_bar();
    self.render_info_window();
  }

  fn render_menu_bar(&self) {
    use egui::*;
    TopBottomPanel::top("menu_bar").show(&self.platform.context(), |ui| {
      trace!(ui);
      menu::bar(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("Organize windows").clicked() {
                ui.ctx().memory().reset_areas();
                ui.close_menu();
            }
            if ui
                .button("Reset egui memory")
                .on_hover_text("Forget scroll, positions, sizes etc")
                .clicked()
            {
                *ui.ctx().memory() = Default::default();
                ui.close_menu();
            }
        });
      });
    });
  }

  fn render_info_window(&mut self) {
    self.info_window.render(&self.platform.context());
  }

}
