mod binary;
#[cfg(not(target_arch = "wasm32"))]
mod gltf;

pub use binary::*;
#[cfg(not(target_arch = "wasm32"))]
pub use gltf::*;
