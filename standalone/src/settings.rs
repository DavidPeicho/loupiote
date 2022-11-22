pub struct Settings {
    pub accumulate: bool,
}

impl Settings {
    pub fn new() -> Self {
        Self { accumulate: false }
    }
}
