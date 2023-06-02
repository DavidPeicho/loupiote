use std::num::NonZeroU32;

use albedo_backend::gpu;
use albedo_bvh::{BLASArray, BVHNode};
use albedo_rtx::texture;
use albedo_rtx::uniforms::{Instance, Light, Material, Vertex};

use crate::ProxyMesh;

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

pub struct Scene {
    pub meshes: Vec<ProxyMesh>,
    pub instances: Vec<Instance>,
    pub materials: Vec<Material>,
    pub blas: BLASArray<Vertex>,
    pub lights: Vec<Light>,
    pub atlas: Option<texture::TextureAtlas>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            meshes: vec![],
            instances: vec![Instance {
                ..Default::default()
            }],
            materials: vec![Material {
                ..Default::default()
            }],
            blas: BLASArray {
                entries: vec![albedo_bvh::BLASEntryDescriptor {
                    node: albedo_bvh::INVALID_INDEX,
                    vertex: albedo_bvh::INVALID_INDEX,
                    index: albedo_bvh::INVALID_INDEX,
                }],
                nodes: vec![BVHNode {
                    ..Default::default()
                }],
                vertices: vec![Vertex {
                    ..Default::default()
                }],
                indices: vec![albedo_bvh::INVALID_INDEX],
            },
            lights: vec![Light::new()],
            atlas: None,
        }
    }
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
            view_formats: &[],
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
            atlas_extent,
        );

        let info_extent = wgpu::Extent3d {
            width: atlas.textures().len() as u32,
            height: 1,
            depth_or_array_layers: 1,
        };
        let info_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Info Texture"),
            size: info_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D1,
            format: wgpu::TextureFormat::Rgba32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let info_data_bytes = atlas.textures().len() * std::mem::size_of::<TextureAtlasGPU>();
        let info_data_raw = unsafe {
            std::slice::from_raw_parts(atlas.textures().as_ptr() as *const u8, info_data_bytes)
        };
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &info_texture,
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
            },
            info_data_raw,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: NonZeroU32::new(info_data_bytes as u32),
                rows_per_image: None,
            },
            info_extent,
        );

        let mut buffer = gpu::Buffer::new_with_data(&device, atlas.textures(), None);
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
    pub instance_buffer: gpu::Buffer<Instance>,
    pub materials_buffer: gpu::Buffer<Material>,
    pub bvh_buffer: gpu::Buffer<BVHNode>,
    pub index_buffer: gpu::Buffer<u32>,
    pub vertex_buffer: gpu::Buffer<Vertex>,
    pub light_buffer: gpu::Buffer<Light>,
    pub atlas: Option<TextureAtlasGPU>,
}

pub struct ProbeGPU {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl ProbeGPU {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> Self {
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
            view_formats: &[],
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

        Self {
            texture: probe_texture,
            view: probe_texture_view,
        }
    }
}

impl SceneGPU {
    pub fn new(
        device: &wgpu::Device,
        instances: &[Instance],
        materials: &[Material],
        bvh: &[BVHNode],
        indices: &[u32],
        vertices: &[Vertex],
        lights: &[Light],
    ) -> Self {
        SceneGPU {
            instance_buffer: gpu::Buffer::new_storage_with_data(&device, instances, None),
            materials_buffer: gpu::Buffer::new_storage_with_data(&device, materials, None),
            bvh_buffer: gpu::Buffer::new_storage_with_data(&device, bvh, None),
            index_buffer: gpu::Buffer::new_storage_with_data(
                &device,
                indices,
                Some(gpu::BufferInitDescriptor {
                    label: None,
                    usage: wgpu::BufferUsages::INDEX,
                }),
            ),
            vertex_buffer: gpu::Buffer::new_storage_with_data(
                &device,
                vertices,
                Some(gpu::BufferInitDescriptor {
                    label: None,
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            ),
            light_buffer: gpu::Buffer::new_storage_with_data(&device, lights, None),
            atlas: None,
        }
    }

    pub fn new_from_scene(scene: &Scene, device: &wgpu::Device, queue: &wgpu::Queue) -> SceneGPU {
        let mut resources = SceneGPU::new(
            device,
            &scene.instances,
            &scene.materials,
            &scene.blas.nodes,
            &scene.blas.indices,
            &scene.blas.vertices,
            &scene.lights,
        );
        resources.instance_buffer.update(&queue, &scene.instances);
        resources.materials_buffer.update(&queue, &scene.materials);
        resources.bvh_buffer.update(&queue, &scene.blas.nodes);
        resources.index_buffer.update(&queue, &scene.blas.indices);
        resources.vertex_buffer.update(&queue, &scene.blas.vertices);
        resources.light_buffer.update(&queue, &scene.lights);

        // Build atlas and copy to GPU.
        if let Some(atlas) = scene.atlas.as_ref() {
            resources.atlas = Some(TextureAtlasGPU::new(device, queue, atlas));
        }

        // Upload texture atlas.
        // @todo
        resources
    }
}
