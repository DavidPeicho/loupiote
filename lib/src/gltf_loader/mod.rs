use albedo_backend::mesh::IndexDataSlice;
use albedo_bvh::Mesh;
use albedo_rtx::texture;
use albedo_rtx::uniforms;
use pas::Slice;

use gltf::{self, image};

use crate::errors::Error;
use crate::scene::{ImageData, Scene};
use crate::BLAS;

pub struct GLTFLoaderOptions {
    pub atlas_max_size: u32,
}

pub struct ProxyMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Option<Vec<[f32; 2]>>,
    pub indices: Vec<u32>,
}
impl Mesh for ProxyMesh {
    fn indices(&self) -> Option<IndexDataSlice> {
        Some(IndexDataSlice::U32(Slice::native(&self.indices)))
    }

    fn positions(&self) -> Option<Slice<[f32; 3]>> {
        Some(Slice::native(&self.positions))
    }
}

fn rgba8_image(image: image::Data) -> ImageData {
    let (components, _) = match image.format {
        image::Format::R8 => (1, 1),
        image::Format::R8G8 => (2, 1),
        image::Format::R8G8B8 => (3, 1),
        image::Format::R8G8B8A8 => (4, 1),
        image::Format::R16 => (1, 2),
        image::Format::R16G16 => (2, 2),
        image::Format::R16G16B16 => (3, 2),
        image::Format::R16G16B16A16 => (4, 2),
        image::Format::R32G32B32FLOAT => (3, 4),
        image::Format::R32G32B32A32FLOAT => (4, 4),
    };

    // @todo: 16bits will break.

    // Allocate a new buffer if the data isn't RGBA8.
    let pixels_count = image.width as usize * image.height as usize;
    let buffer = if components != 4 {
        let mut buffer = vec![0 as u8; pixels_count * 4];
        for i in 0..pixels_count {
            let dst_start = i * 4;
            let src_start = i * components;
            buffer[dst_start..(dst_start + components)]
                .copy_from_slice(&image.pixels[src_start..(src_start + components)]);
        }
        buffer
    } else {
        image.pixels
    };

    ImageData::new(buffer, image.width, image.height)
}

pub fn load_gltf(data: &[u8], opts: &GLTFLoaderOptions) -> Result<Scene, Error> {
    // @todo: This method is too slow, profile.
    let (doc, buffers, images) = match gltf::import_slice(data) {
        Ok(tuple) => tuple,
        Err(err) => {
            return match err {
                gltf::Error::Io(_) => Err(Error::FileNotFound("failed to load gltf".into())),
                _ => Err(Error::FileNotFound(String::new())),
            };
            // if let gltf::Error::Io(_) = err {
            //     error!("Hint: Are the .bin file(s) referenced by the .gltf file available?")
            // }
        }
    };
    let mut meshes: Vec<ProxyMesh> = Vec::new();
    let mut materials: Vec<uniforms::Material> = Vec::new();
    let mut instances: Vec<uniforms::Instance> = Vec::new();

    for mesh in doc.meshes() {
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut normals: Vec<[f32; 3]> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut uvs: Option<Vec<[f32; 2]>> = None;

        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
            positions.extend(reader.read_positions().unwrap());
            normals.extend(reader.read_normals().unwrap());
            if let Some(texcoord) = reader.read_tex_coords(0) {
                uvs.get_or_insert_with(|| Vec::new())
                    .extend(texcoord.into_f32());
            }
            indices.extend(
                reader
                    .read_indices()
                    .map(|read_indices| read_indices.into_u32())
                    .unwrap(),
            );
        }
        meshes.push(ProxyMesh {
            positions,
            normals,
            uvs,
            indices,
        });
    }

    for material in doc.materials() {
        let pbr = material.pbr_metallic_roughness();
        materials.push(uniforms::Material {
            color: pbr.base_color_factor().into(),
            roughness: pbr.roughness_factor(),
            reflectivity: pbr.metallic_factor(),
            albedo_texture: pbr
                .base_color_texture()
                .map(|c| c.texture().index() as u32)
                .unwrap_or(uniforms::INVALID_INDEX),
            mra_texture: pbr
                .metallic_roughness_texture()
                .map(|c| c.texture().index() as u32)
                .unwrap_or(uniforms::INVALID_INDEX),
            ..Default::default()
        });
    }

    let blas = BLAS::new(&meshes)?;
    for node in doc.nodes() {
        // @todo: handle scene graph.
        // User should have their own scene graph. However, for pure pathtracing
        // from format like glTF, a small footprint hierarchy handler should be
        // provided.
        if let Some(mesh) = node.mesh() {
            let index = mesh.index();
            let entry = blas.entries.get(index).unwrap();
            let model_to_world = glam::Mat4::from_cols_array_2d(&node.transform().matrix());
            for primitive in mesh.primitives() {
                let material_index = match primitive.material().index() {
                    Some(v) => v as u32,
                    None => u32::MAX,
                };
                instances.push(uniforms::Instance {
                    model_to_world,
                    world_to_model: model_to_world.inverse(),
                    material_index,
                    bvh_root_index: entry.node,
                    vertex_root_index: entry.vertex,
                    index_root_index: entry.index,
                });
            }
        }
    }

    let atlas = if images.len() > 0 {
        let mut atlas = texture::TextureAtlas::new(opts.atlas_max_size);
        for image in images.into_iter() {
            let i = rgba8_image(image);
            // @todo: package metal / roughness / ao in single texture.
            atlas.add(&texture::TextureSlice::new(i.data(), i.width()).unwrap());
        }
        Some(atlas)
    } else {
        None
    };

    Ok(Scene {
        meshes,
        instances,
        materials,
        blas,
        atlas,
        lights: vec![Default::default()],
    })
}
