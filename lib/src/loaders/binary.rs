use albedo_rtx::{blas, uniforms, BLASArray, Material, MeshDescriptor, Uniform, Vertex};
use std::{convert::TryInto, io::Read};

use crate::Scene;

pub fn load_binary_from_path<P: AsRef<std::path::Path>>(path: P, scene: &mut Scene) {
    let f = std::fs::File::open(path).unwrap();
    let mut reader = std::io::BufReader::new(f);

    let mut uint_buf = [0; 4];
    reader.read_exact(&mut uint_buf).unwrap();
    let primitive_count = u32::from_le_bytes(uint_buf);

    let vertex_count = primitive_count * 3;
    let mut vertices = Vec::with_capacity(vertex_count as usize);

    let mut vec4_buf = [0; 16];
    for i in 0..vertex_count {
        reader.read_exact(&mut vec4_buf).unwrap();
        vertices.push(Vertex {
            position: [
                f32::from_le_bytes(vec4_buf[0..4].try_into().unwrap()),
                f32::from_le_bytes(vec4_buf[4..8].try_into().unwrap()),
                f32::from_le_bytes(vec4_buf[8..12].try_into().unwrap()),
                f32::from_le_bytes(vec4_buf[12..16].try_into().unwrap()),
            ],
            normal: [0.0, 0.0, 0.0, 0.0],
        });
    }

    for i in (0..vertex_count).step_by(3) {
        let v_0 = &vertices[i as usize].position;
        let v_1 = &vertices[i as usize + 1].position;
        let v_2 = &vertices[i as usize + 2].position;

        let v_0 = glam::Vec3::new(v_0[0], v_0[1], v_0[2]);
        let v_1 = glam::Vec3::new(v_1[0], v_1[1], v_1[2]);
        let v_2 = glam::Vec3::new(v_2[0], v_2[1], v_2[2]);

        let e_0 = v_0 - v_1;
        let e_1 = v_0 - v_2;
        let normal = glam::Vec3::cross(e_0.normalize(), e_1.normalize());
        let normal = [normal.x, normal.y, normal.z, 0.0];
        vertices[i as usize].normal = normal;
        vertices[i as usize + 1].normal = normal;
        vertices[i as usize + 2].normal = normal;
    }

    let mesh = MeshDescriptor {
        positions: pas::Slice::new(&vertices, 0),
        normals: Some(pas::Slice::new(&vertices, 16)),
        texcoords0: None,
    };

    let blas_index = scene.blas.entries.len() as u32;
    let material_index = scene.materials.len() as u32;

    scene.blas.add_bvh(mesh);
    scene
        .blas
        .add_instance(blas_index, glam::Mat4::IDENTITY, material_index);

    scene.materials.push(Material {
        color: glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
        roughness: 1.0,
        reflectivity: 0.0,
        albedo_texture: uniforms::INVALID_INDEX,
        mra_texture: uniforms::INVALID_INDEX,
    });
}
