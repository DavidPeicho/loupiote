# Loupiote

Interactive Pathtracer written in Rust and based on [WebGPU](https://github.com/gfx-rs/wgpu) 🦀

This repository is based on the [Albedo](https://github.com/albedo-engine/albedo) and more precisely the [albedo_rtx](https://github.com/albedo-engine/albedo/tree/main/crates/albedo_rtx) crate.

![Initial Result with Albedo](screenshots/damaged-helmet.gif)

## Features

* BVH built using SAH
* glTF loader
* GUI composed of:
    * Load glTF using dialog
    * Save current render

## Stability

🚧 Loupiote is a work-in-progress and might be unstable 🚧

This package will serve the purpose of:
* Providing a high-level, easy-to-use Pathracer
* Helping stabilize the [Albedo](https://github.com/albedo-engine/albedo) core rendering library

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
