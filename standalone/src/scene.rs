use std::num::NonZeroU32;

use albedo_backend::GPUBuffer;
use albedo_rtx::accel;
use albedo_rtx::renderer;
use albedo_rtx::renderer::resources::{
    BVHNodeGPU, InstanceGPU, LightGPU, MaterialGPU, TextureInfoGPU, VertexGPU,
};
use albedo_rtx::texture;

pub struct ImageData {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

impl ImageData {
    pub fn new(data: Vec<u8>, width: u32, height: u32) -> Self {
        ImageData {
            data,
            width,
            height,
        }
    }
    pub fn data(&self) -> &[u8] {
        self.data.as_slice()
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
}

pub struct Scene<T: albedo_rtx::mesh::Mesh> {
    pub meshes: Vec<T>,
    pub bvhs: Vec<accel::BVH>,
    pub instances: Vec<renderer::resources::InstanceGPU>,
    pub materials: Vec<renderer::resources::MaterialGPU>,
    pub node_buffer: Vec<renderer::resources::BVHNodeGPU>,
    pub vertex_buffer: Vec<renderer::resources::VertexGPU>,
    pub index_buffer: Vec<u32>,
    pub lights: Vec<renderer::resources::LightGPU>,
    pub atlas: Option<texture::TextureAtlas>,
}

pub struct TextureAtlasGPU {
    pub texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    pub info_texture: wgpu::Texture,
    pub info_view: wgpu::TextureView,
}

impl TextureAtlasGPU {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        atlas: &texture::TextureAtlas,
    ) -> TextureAtlasGPU {
        let atlas_extent = wgpu::Extent3d {
            width: atlas.size(),
            height: atlas.size(),
            depth_or_array_layers: atlas.layer_count() as u32,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Texture Atlas"),
            size: atlas_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
            },
            atlas.data(),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: NonZeroU32::new(4 * atlas.size()),
                rows_per_image: NonZeroU32::new(atlas.size()),
            },
            atlas_extent
        );

        let info_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Info Texture"),
            size: wgpu::Extent3d {
                width: atlas.textures().len() as u32,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D1,
            format: wgpu::TextureFormat::Rgba32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        });
        let info_data_bytes = atlas.textures().len() * std::mem::size_of::<TextureAtlasGPU>();
        let info_data_raw = unsafe {
            std::slice::from_raw_parts(atlas.textures().as_ptr() as *const u8, info_data_bytes)
        };
        // queue.write_texture(
        //     wgpu::ImageCopyTexture {
        //         texture: &info_texture,
        //         aspect: wgpu::TextureAspect::All,
        //         mip_level: 0,
        //         origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
        //     },
        //     info_data_raw,
        //     wgpu::ImageDataLayout {
        //         offset: 0,
        //         bytes_per_row: NonZeroU32::new(info_data_bytes as u32),
        //         rows_per_image: None,
        //     },
        //     wgpu::Extent3d {
        //         width,
        //         height,
        //         depth_or_array_layers: 1,
        //     },
        // );

        let mut buffer = GPUBuffer::from_data(&device, atlas.textures());
        buffer.update(&queue, atlas.textures());
        TextureAtlasGPU {
            texture: texture,
            texture_view: view,
            info_view: info_texture.create_view(&wgpu::TextureViewDescriptor::default()),
            info_texture,
        }
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.texture_view
    }
    pub fn info_texture(&self) -> &wgpu::Texture {
        &self.info_texture
    }
    pub fn info_texture_view(&self) -> &wgpu::TextureView {
        &self.info_view
    }

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
    pub atlas: Option<TextureAtlasGPU>,
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
            atlas: None,
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
            device,
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

        // Build atlas and copy to GPU.
        if let Some(atlas) = scene.atlas.as_ref() {
            resources.atlas = Some(TextureAtlasGPU::new(device, queue, atlas));
        }

        // Upload texture atlas.
        // @todo
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
                    std::mem::size_of::<image::codecs::hdr::Rgbe8Pixel>() as u32 * width,
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
