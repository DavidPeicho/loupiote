# Albedo: Pathtracer

Interactive Pathtracer written in Rust and based on [WebGPU](https://github.com/gfx-rs/wgpu) 🦀

This repository is based on the [Albedo](https://github.com/albedo-engine/albedo) and more precisely the [albedo_rtx](https://github.com/albedo-engine/albedo/tree/main/crates/albedo_rtx) crate.

![Initial Result with Albedo](screenshots/damaged-helmet.gif)

## Features

* BVH built using SAH
* glTF loader
* GUI composed of:
    * Load glTF using dialog
    * Save current render

## Build

🚧 Albedo Pathtracer is a work-in-progress and might be unstable 🚧

* Download locally the [Albedo library](https://github.com/albedo-engine/albedo)
* Update the `Cargo.toml` file with the path to your local Albedo library:

```toml
[dependencies]
albedo_backend = { path = "[PATH_TO_ALBEDO]/crates/albedo_backend", version = "0.0.1" }
albedo_rtx = { path = "[PATH_TO_ALBEDO]/crates/albedo_rtx", version = "0.0.1" }
```

## Usage

### Camera

* WASD to fly around
* Left clikc + mouse move to rotate around

## Coming Next

* [x] Texture Mapping
* [x] GUI
* [ ] [SVGF](https://cg.ivd.kit.edu/publications/2017/svgf/svgf_preprint.pdf)

## Gallery

![Initial Result with Albedo](screenshots/damaged-helmet.jpg)

* **Title**: *Battle Damaged Sci-fi Helmet - PBR*
* **Author**: [theblueturtle_](https://sketchfab.com/theblueturtle_)
* **License**: Creative Commons Attribution-NonCommercial

## References

* [Physically Based Rendering: From Theory To Implementation](https://pbr-book.org/)
* [Physically Based Lighting at Pixar](https://blog.selfshadow.com/publications/s2013-shading-course/pixar/s2013_pbs_pixar_notes.pdf)
* [Real Shading in Unreal Engine 4](https://blog.selfshadow.com/publications/s2013-shading-course/karis/s2013_pbs_epic_notes_v2.pdf)
* [OpenGL-PathTracer](https://github.com/RobertBeckebans/OpenGL-PathTracer)
* [three-gpu-pathtracer](https://github.com/gkjohnson/three-gpu-pathtracer)
