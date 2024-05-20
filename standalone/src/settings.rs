pub struct Settings {
    pub accumulate: bool,
    pub use_blue_noise: bool,
}

impl Settings {
    pub fn new() -> Self {
        Self { accumulate: false, use_blue_noise: true }
    }
}
