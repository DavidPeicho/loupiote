use std::path;

pub enum Event {
    SaveScreenshot(path::PathBuf),
}
