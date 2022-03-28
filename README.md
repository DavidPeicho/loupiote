# Albedo: Pathtracer

Interactive Pathtracer written in Rust and based on [WebGPU](https://github.com/gfx-rs/wgpu) 🦀

This repository is based on the [Albedo](https://github.com/albedo-engine/albedo) and more precisely the [albedo_rtx](https://github.com/albedo-engine/albedo/tree/main/crates/albedo_rtx) crate.

![Initial Result with Albedo](screenshots/initial_result.gif)

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

* [ ] Texture Mapping
* [ ] GUI
* [ ] [SVGF](https://cg.ivd.kit.edu/publications/2017/svgf/svgf_preprint.pdf)
