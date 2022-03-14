use egui_winit_platform::{
  Platform,
  PlatformDescriptor
};

pub struct GUI {
  platform: Platform,
  render_pass: egui_wgpu_backend::RenderPass,
}

impl GUI {

  pub fn new(
    device: &wgpu::Device,
    surface_config: &wgpu::SurfaceConfiguration
  ) -> Self {
    let mut platform = Platform::new(PlatformDescriptor {
      physical_width: surface_config.width,
      physical_height: surface_config.height,
      scale_factor: 1.0,
      font_definitions: egui::FontDefinitions::default(),
      style: Default::default(),
    });
    let mut render_pass = egui_wgpu_backend::RenderPass::new(&device, surface_config.format, 1);
    GUI {
      platform,
      render_pass
    }
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
    // let mut frame =  epi::Frame::new(epi::backend::FrameData {
    //   info: epi::IntegrationInfo {
    //       name: "egui_example",
    //       web_info: None,
    //       cpu_usage: previous_frame_time,
    //       native_pixels_per_point: Some(window.scale_factor() as _),
    //       prefer_dark_mode: None,
    //   },
    //   output: app_output,
    //   repaint_signal: repaint_signal.clone(),
    // });
    egui::TopBottomPanel::top("wrap_app_top_bar").show(&self.platform.context(), |ui| {
      egui::trace!(ui);
      ui.label("Test Bitch");
    });

    self._render_internal();

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

  fn _render_internal(&self) {

  }

}
