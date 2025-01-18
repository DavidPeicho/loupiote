use std::path::Path;

use albedo_rtx::texture;
use albedo_rtx::uniforms;

use albedo_rtx::BLASArray;
use albedo_rtx::IndexedMeshDescriptor;
use albedo_rtx::MeshDescriptor;
use gltf::{self, image};

use crate::errors::Error;
use crate::scene::{ImageData, Scene};

pub struct GLTFLoaderOptions {
    pub atlas_max_size: u32,
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

pub fn load_gltf(data: &[u8], opts: &GLTFLoaderOptions, scene: &mut Scene) -> Result<(), Error> {
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

    let bvh_offset = scene.blas.entries.len() as u32;
    for mesh in doc.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
            let Some(in_positions) = reader.read_positions() else {
                continue;
            };

            match primitive.mode() {
                gltf::mesh::Mode::Triangles
                | gltf::mesh::Mode::TriangleFan
                | gltf::mesh::Mode::TriangleStrip => (),
                _ => continue,
            };

            // TODO: glTF can be sparsed, which means a copy is required in this particular case.
            // Ideally, the glTF crate would give a fast way to nth the iterator.
            let positions: Vec<[f32; 4]> = in_positions.map(|v| [v[0], v[1], v[2], 0.0]).collect();
            let normals: Option<Vec<[f32; 3]>> = if let Some(normals) = reader.read_normals() {
                Some(normals.collect())
            } else {
                None
            };
            let texcoords: Option<Vec<[f32; 2]>> =
                if let Some(texcoords) = reader.read_tex_coords(0) {
                    let texcoords = texcoords.into_f32();
                    Some(texcoords.collect())
                } else {
                    None
                };

            let mesh = MeshDescriptor {
                positions: pas::Slice::native(&positions),
                normals: normals.as_ref().map(|v| pas::Slice::native(v)),
                texcoords0: texcoords.as_ref().map(|v| pas::Slice::native(&v)),
            };

            if let Some(indices) = reader.read_indices() {
                let indices: Vec<u32> = indices.into_u32().collect();
                scene.blas.add_bvh_indexed(IndexedMeshDescriptor {
                    mesh,
                    indices: &indices,
                });
            } else {
                scene.blas.add_bvh(mesh);
            }
        }
    }

    let mat_offset = scene.materials.len() as u32;
    let texture_offset = if let Some(atlas) = scene.atlas.as_ref() {
        atlas.textures().len() as u32
    } else {
        0
    };
    for material in doc.materials() {
        let pbr = material.pbr_metallic_roughness();
        scene.materials.push(uniforms::Material {
            color: pbr.base_color_factor().into(),
            roughness: pbr.roughness_factor(),
            reflectivity: pbr.metallic_factor(),
            albedo_texture: pbr
                .base_color_texture()
                .map(|c| texture_offset + c.texture().index() as u32)
                .unwrap_or(uniforms::INVALID_INDEX),
            mra_texture: pbr
                .metallic_roughness_texture()
                .map(|c| texture_offset + c.texture().index() as u32)
                .unwrap_or(uniforms::INVALID_INDEX),
            ..Default::default()
        });
    }

    for node in doc.nodes() {
        // @todo: handle scene graph.
        // User should have their own scene graph. However, a simple code path
        // should directly be provided for users.
        if let Some(mesh) = node.mesh() {
            let index = mesh.index() as u32;
            let model_to_world = glam::Mat4::from_cols_array_2d(&node.transform().matrix());
            for primitive in mesh.primitives() {
                let material_index = match primitive.material().index() {
                    Some(v) => v as u32,
                    None => u32::MAX,
                };
                scene.blas.add_instance(
                    bvh_offset + index,
                    model_to_world,
                    mat_offset + material_index,
                );
            }
        }
    }

    if images.len() > 0 {
        if scene.atlas.is_none() {
            scene.atlas = Some(texture::TextureAtlas::new(opts.atlas_max_size));
        }
        for image in images.into_iter() {
            let i = rgba8_image(image);
            // @todo: package metal / roughness / ao in single texture.
            scene
                .atlas
                .as_mut()
                .unwrap()
                .add(&texture::TextureSlice::new(i.data(), i.width()).unwrap());
        }
    }

    Ok(())
}

pub fn load_gltf_path<P: AsRef<Path>>(
    path: P,
    opts: &GLTFLoaderOptions,
    scene: &mut Scene,
) -> Result<(), Error> {
    let bytes: Vec<u8> = std::fs::read(path).unwrap();
    load_gltf(&bytes[..], opts, scene)
}
