use std::num::NonZeroU32;

use albedo_backend::GPUBuffer;
use albedo_rtx::accel;
use albedo_rtx::renderer;
use albedo_rtx::renderer::resources::{BVHNodeGPU, InstanceGPU, LightGPU, MaterialGPU, VertexGPU};

pub struct Scene<T: albedo_rtx::mesh::Mesh> {
    pub meshes: Vec<T>,
    pub bvhs: Vec<accel::BVH>,
    pub instances: Vec<renderer::resources::InstanceGPU>,
    pub materials: Vec<renderer::resources::MaterialGPU>,
    pub node_buffer: Vec<renderer::resources::BVHNodeGPU>,
    pub vertex_buffer: Vec<renderer::resources::VertexGPU>,
    pub index_buffer: Vec<u32>,
    pub lights: Vec<renderer::resources::LightGPU>,
}

pub struct SceneGPU {
    pub instance_buffer: GPUBuffer<InstanceGPU>,
    pub materials_buffer: GPUBuffer<MaterialGPU>,
    pub bvh_buffer: GPUBuffer<BVHNodeGPU>,
    pub index_buffer: GPUBuffer<u32>,
    pub vertex_buffer: GPUBuffer<VertexGPU>,
    pub light_buffer: GPUBuffer<LightGPU>,
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
            probe_texture: None,
            probe_texture_view: None,
        }
    }

    pub fn new_from_scene<T>(
        scene: &Scene<T>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> SceneGPU
    where
        T: albedo_rtx::mesh::Mesh,
    {
        let mut resources = SceneGPU::new(
            &device,
            &scene.instances,
            &scene.materials,
            &scene.node_buffer,
            &scene.index_buffer,
            &scene.vertex_buffer,
            &scene.lights,
        );
        resources.instance_buffer.update(&queue, &scene.instances);
        resources.materials_buffer.update(&queue, &scene.materials);
        resources.bvh_buffer.update(&queue, &scene.node_buffer);
        resources.index_buffer.update(&queue, &scene.index_buffer);
        resources.vertex_buffer.update(&queue, &scene.vertex_buffer);
        resources.light_buffer.update(&queue, &scene.lights);
        resources
    }

    pub fn upload_probe(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[u8],
        width: u32,
        height: u32,
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
                bytes_per_row: NonZeroU32::new(
                    std::mem::size_of::<image::hdr::Rgbe8Pixel>() as u32 * width,
                ),
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
