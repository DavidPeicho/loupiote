use std::num::NonZeroU32;

use albedo_backend::{GPUBuffer, UniformBuffer};
use albedo_rtx::renderer::resources::{
    InstanceGPU,
    MaterialGPU,
    BVHNodeGPU,
    VertexGPU,
    LightGPU,
    SceneSettingsGPU,
};

pub struct SceneGPU {
    pub instance_buffer: GPUBuffer<InstanceGPU>,
    pub materials_buffer: GPUBuffer<MaterialGPU>,
    pub bvh_buffer: GPUBuffer<BVHNodeGPU>,
    pub index_buffer: GPUBuffer<u32>,
    pub vertex_buffer: GPUBuffer<VertexGPU>,
    pub light_buffer: GPUBuffer<LightGPU>,
    pub scene_settings_buffer: UniformBuffer<SceneSettingsGPU>,
    pub probe_texture: Option<wgpu::Texture>,
    pub probe_texture_view: Option<wgpu::TextureView>,
}

impl SceneGPU {

    pub fn new(
        device: &wgpu::Device,
        instances: &[InstanceGPU],
        materials: &[MaterialGPU],
        bvh: &[BVHNodeGPU],
        indices: &[u32],
        vertices: &[VertexGPU],
        lights: &[LightGPU],
    ) -> Self {
        SceneGPU {
            instance_buffer: GPUBuffer::from_data(&device, instances),
            materials_buffer: GPUBuffer::from_data(&device, materials),
            bvh_buffer: GPUBuffer::from_data(&device, bvh),
            index_buffer: GPUBuffer::from_data(&device, indices),
            vertex_buffer: GPUBuffer::from_data(&device, vertices),
            light_buffer: GPUBuffer::from_data(&device, lights),
            scene_settings_buffer: UniformBuffer::new(&device),
            probe_texture: None,
            probe_texture_view: None,
        }
    }

    pub fn update_globals(&mut self, queue: &wgpu::Queue, nb_instances: u32, nb_lights: u32) {
        self.scene_settings_buffer.update(
            &queue,
            &SceneSettingsGPU {
                light_count: nb_lights,
                instance_count: nb_instances,
            },
        );
    }

    pub fn upload_probe(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[u8],
        width: u32,
        height: u32
    ) {
        let probe_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Cubemap"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        });

        let probe_texture_view = probe_texture.create_view(&wgpu::TextureViewDescriptor::default());
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &probe_texture,
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
            },
            data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: NonZeroU32::new(std::mem::size_of::<image::hdr::Rgbe8Pixel>() as u32 * width),
                rows_per_image: NonZeroU32::new(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.probe_texture = Some(probe_texture);
        self.probe_texture_view = Some(probe_texture_view);
    }

}
