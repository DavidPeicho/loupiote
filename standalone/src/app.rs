use std::path;

use albedo_lib::{load_gltf, Device, GLTFLoaderOptions, ProbeGPU, Renderer, Scene, SceneGPU};

use crate::{commands, errors::Error, Event, Settings, Spawner};

pub struct Plaftorm {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Device,
    pub window: winit::window::Window,
    pub surface: wgpu::Surface,
    pub queue: wgpu::Queue,
    pub size: winit::dpi::PhysicalSize<u32>,
}

pub struct ApplicationContext {
    pub platform: Plaftorm,
    pub event_loop_proxy: winit::event_loop::EventLoopProxy<Event>,
    #[cfg(not(target_arch = "wasm32"))]
    pub executor: Spawner<'static>,
    #[cfg(target_arch = "wasm32")]
    pub executor: Spawner,
    pub renderer: Renderer,
    pub scene: Scene,
    pub scene_gpu: SceneGPU,
    pub probe: Option<ProbeGPU>,
    pub limits: wgpu::Limits,
    pub settings: Settings,
}

impl ApplicationContext {
    pub fn run_command(&mut self, command: commands::EditorCommand) {
        match command {
            commands::EditorCommand::ToggleAccumulation => {
                self.settings.accumulate = !self.settings.accumulate
            }
        }
    }

    pub fn event(&mut self, event: Event) {
        // @todo: handle errors.
        match event {
            Event::SaveScreenshot(path) => self.save_screenshot(path),
            Event::LoadFile(path) => self.load_file(path).unwrap(),
        }
    }

    pub fn load_env<P: AsRef<std::path::Path>>(&mut self, path: P) {
        let file_reader = std::io::BufReader::new(std::fs::File::open(path).unwrap());
        let decoder = image::codecs::hdr::HdrDecoder::new(file_reader).unwrap();
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
    }

    pub fn load_file<P: AsRef<path::Path>>(&mut self, path: P) -> Result<(), Error> {
        let scene = load_gltf(
            &path,
            &GLTFLoaderOptions {
                atlas_max_size: self.limits.max_texture_dimension_1d,
            },
        )?;
        self.scene_gpu =
            SceneGPU::new_from_scene(&scene, self.platform.device.inner(), &self.platform.queue);
        self.scene = scene;
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
}
