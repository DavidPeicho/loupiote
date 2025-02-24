use albedo_backend::gpu::{self, Atlas2D, TextureAtlas, TextureId};
use albedo_rtx::uniforms::{BVHNode, Instance, Light, Material, Vertex};
use albedo_rtx::{BLASArray, BVHPrimitive};

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
    pub materials: Vec<Material>,
    pub blas: BLASArray,
    pub lights: Vec<Light>,
    pub images: Vec<ImageData>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            materials: vec![Material {
                ..Default::default()
            }],
            blas: BLASArray {
                entries: vec![Default::default()],
                nodes: vec![Default::default()],
                primitives: vec![Default::default()],
                vertices: vec![Default::default()],
                instances: vec![Default::default()],
            },
            lights: vec![Light::new()],
            images: vec![],
        }
    }
}

pub struct SceneGPU {
    pub instance_buffer: gpu::Buffer<Instance>,
    pub materials_buffer: gpu::Buffer<Material>,
    pub bvh_buffer: gpu::Buffer<BVHNode>,
    pub bvh_tri_buffer: gpu::Buffer<BVHPrimitive>,
    pub vertex_buffer: gpu::Buffer<Vertex>,
    pub light_buffer: gpu::Buffer<Light>,
    pub atlas: TextureAtlas,
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
        let rbge8_bytes: u32 = 4 as u32;
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
                bytes_per_row: Some(rbge8_bytes * width),
                rows_per_image: Some(height),
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
        bvh_tris: &[BVHPrimitive],
        vertices: &[Vertex],
        lights: &[Light],
    ) -> Self {
        SceneGPU {
            instance_buffer: gpu::Buffer::new_storage_with_data(&device, instances, None),
            materials_buffer: gpu::Buffer::new_storage_with_data(&device, materials, None),
            bvh_buffer: gpu::Buffer::new_storage_with_data(&device, bvh, None),
            bvh_tri_buffer: gpu::Buffer::new_storage_with_data(&device, bvh_tris, None),
            vertex_buffer: gpu::Buffer::new_storage_with_data(
                &device,
                vertices,
                Some(gpu::BufferInitDescriptor {
                    label: None,
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            ),
            light_buffer: gpu::Buffer::new_storage_with_data(&device, lights, None),
            atlas: TextureAtlas::new(device, 2, 1),
        }
    }

    pub fn new_from_scene(scene: &Scene, device: &wgpu::Device, queue: &wgpu::Queue) -> SceneGPU {
        let mut resources = SceneGPU::new(
            device,
            &scene.blas.instances,
            &scene.materials,
            &scene.blas.nodes,
            &scene.blas.primitives,
            &scene.blas.vertices,
            &scene.lights,
        );
        resources
            .instance_buffer
            .update(&queue, &scene.blas.instances);
        resources.materials_buffer.update(&queue, &scene.materials);
        resources.bvh_buffer.update(&queue, &scene.blas.nodes);
        resources
            .bvh_tri_buffer
            .update(&queue, &scene.blas.primitives);
        resources.vertex_buffer.update(&queue, &scene.blas.vertices);
        resources.light_buffer.update(&queue, &scene.lights);

        // Build atlas and copy to GPU.
        let limits = device.limits();
        let mut atlas: Atlas2D = Atlas2D::new(limits.max_texture_dimension_1d);
        for img in &scene.images {
            atlas.reserve(img.width, img.height);
        }

        resources.atlas = TextureAtlas::from_atlas2d(device, atlas, None);
        for (i, img) in scene.images.iter().enumerate() {
            resources
                .atlas
                .upload(queue, TextureId::new(i as u32), &img.data);
        }

        resources
    }
}
