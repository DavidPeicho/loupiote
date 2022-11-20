use albedo_lib::{Device, ProxyMesh, Renderer, Scene, SceneGPU};
use std::path;

use crate::{commands, Event, Settings, Spawner};

pub struct Plaftorm {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Device,
    pub window: winit::window::Window,
    pub surface: wgpu::Surface,
    pub queue: wgpu::Queue,
    pub size: winit::dpi::PhysicalSize<u32>,
}

pub struct ApplicationContext<'a> {
    pub platform: Plaftorm,
    pub event_loop_proxy: winit::event_loop::EventLoopProxy<Event>,
    pub executor: Spawner<'a>,
    pub scene: Scene<ProxyMesh>,
    pub scene_gpu: SceneGPU,
    pub renderer: Renderer,
    pub limits: wgpu::Limits,
    pub settings: Settings,
}

impl<'a> ApplicationContext<'a> {
    pub fn run_command(&mut self, command: commands::EditorCommand) {
        match command {
            commands::EditorCommand::ToggleAccumulation => {
                self.settings.accumulate = !self.settings.accumulate
            }
        }
    }

    pub fn event(&mut self, event: Event) {
        match event {
            Event::SaveScreenshot(path) => self.save_screenshot(path),
        }
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
}
