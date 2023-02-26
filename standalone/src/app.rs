use std::path;

use albedo_lib::{load_gltf, Device, GLTFLoaderOptions, ProbeGPU, Renderer, Scene, SceneGPU};

use crate::{
    commands, errors::Error, event::LoadEvent, gui::GUI, logger::log, Event, Settings, Spawner,
};

pub struct Plaftorm {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Device,
    pub window: winit::window::Window,
    pub surface: wgpu::Surface,
    pub queue: wgpu::Queue,
}

pub struct ApplicationContext {
    pub platform: Plaftorm,
    pub event_loop_proxy: crate::EventLoopProxy,
    #[cfg(not(target_arch = "wasm32"))]
    pub executor: Spawner<'static>,
    #[cfg(target_arch = "wasm32")]
    pub executor: Spawner,
    pub renderer: Renderer,
    pub scene: Scene,
    pub scene_gpu: SceneGPU,
    pub probe: Option<ProbeGPU>,
    pub settings: Settings,
    pub gui: GUI,
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
            Event::Load(load) => match load {
                LoadEvent::GLTF(data) => self
                    .load_file(&data[..])
                    .unwrap_or_else(|_| self.gui.set_error("failed to load gltf")),
                LoadEvent::Env(data) => self.load_env(&data[..]),
            },
        }
    }

    pub fn resize(&mut self, width_target: u32, height_target: u32) {
        let limits = &self.platform.device.inner().limits();
        let max_bytes_per_pixel = Renderer::max_ssbo_element_in_bytes();
        let max_pixels_count = limits.max_storage_buffer_binding_size / max_bytes_per_pixel;

        let pixels_target_count = width_target * height_target;
        let (width, height) = if max_pixels_count < pixels_target_count {
            let ratio = max_pixels_count as f32 / pixels_target_count as f32;
            (
                (width_target as f32 * ratio) as u32,
                (height_target as f32 * ratio) as u32,
            )
        } else {
            (width_target, height_target)
        };

        self.renderer.resize(
            &self.platform.device,
            &self.scene_gpu,
            self.probe.as_ref(),
            (width, height),
        );

        let dpi = self.platform.window.scale_factor() as f32;
        self.gui.resize(dpi);

        let renderer_size = self.renderer.get_size();
        let renderer_down_size = self.renderer.get_downsampled_size();

        log!(
            "Resize: {{\n\tDpi={:?}\n\tSurface Size=({:?}, {:?})\n\tTarget Size=({:?}, {:?})\n\tMoving Size({:?}, {:?})\n}}",
            dpi,
            width_target,
            height_target,
            renderer_size.0,
            renderer_size.1,
            renderer_down_size.0,
            renderer_down_size.1
        );
    }

    pub fn load_env_path<P: AsRef<path::Path>>(&mut self, path: P) {
        let bytes = std::fs::read(path).unwrap();
        self.load_env(&bytes[..]);
    }

    pub fn load_env(&mut self, data: &[u8]) {
        let decoder = image::codecs::hdr::HdrDecoder::new(data).unwrap();
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
        self.renderer
            .set_resources(&self.platform.device, &self.scene_gpu, self.probe.as_ref());

        log!("Environment: {{");
        log!("\tWidth = {}", metadata.width);
        log!("\tHeight = {}", metadata.height);
        log!("}}");
    }

    pub fn load_file_path<P: AsRef<path::Path>>(&mut self, path: P) -> Result<(), Error> {
        let bytes = std::fs::read(path).unwrap();
        self.load_file(&bytes[..])
    }

    pub fn load_file(&mut self, data: &[u8]) -> Result<(), Error> {
        log!("Loading GLB...");
        let limits = &self.platform.device.inner().limits();
        let scene = load_gltf(
            data,
            &GLTFLoaderOptions {
                atlas_max_size: limits.max_texture_dimension_1d,
            },
        )?;
        self.scene = scene;
        self.scene_gpu = SceneGPU::new_from_scene(
            &self.scene,
            self.platform.device.inner(),
            &self.platform.queue,
        );

        // Update GUI information.
        self.gui
            .windows
            .scene_info_window
            .set_meshes_count(self.scene.meshes.len());
        self.gui
            .windows
            .scene_info_window
            .set_bvh_nodes_count(self.scene.blas.nodes.len());

        if let Some(atlas) = &self.scene.atlas {
            log!("Texture Atlas: {{");
            log!("\tTextures count = {}", atlas.textures().len());
            log!("\tLayers count = {}", atlas.layer_count());
            log!("}}");
        }

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

    pub fn width(&self) -> u32 {
        self.renderer.get_size().0
    }

    pub fn height(&self) -> u32 {
        self.renderer.get_size().0
    }
}
