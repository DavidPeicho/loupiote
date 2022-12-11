use std::path;

pub enum LoadEvent {
    GLTF(Vec<u8>),
    Env(Vec<u8>),
}

pub enum Event {
    SaveScreenshot(path::PathBuf),
    Load(LoadEvent),
}
