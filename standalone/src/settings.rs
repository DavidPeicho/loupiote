use albedo_lib::BlitMode;

pub struct Settings {
    pub accumulate: bool,
    pub use_blue_noise: bool,
    pub blit_mode: BlitMode,
}

impl Settings {
    pub fn new() -> Self {
        Self { accumulate: false, use_blue_noise: false, blit_mode: BlitMode::Pahtrace }
    }
}
