use loupiote_core;

#[derive(Debug)]
pub enum Error {
    FileNotFound(String),
    TextureToBufferReadFail,
    AccelBuild(String),
}

impl From<loupiote_core::Error> for Error {
    fn from(e: loupiote_core::Error) -> Self {
        match e {
            loupiote_core::Error::FileNotFound(f) => Error::FileNotFound(f),
            loupiote_core::Error::TextureToBufferReadFail => Error::TextureToBufferReadFail,
            loupiote_core::Error::AccelBuild(reason) => Error::AccelBuild(reason),
        }
    }
}

impl From<Error> for String {
    fn from(e: Error) -> Self {
        match e {
            Error::FileNotFound(filename) => {
                format!("file not found: {}", filename)
            }
            Error::TextureToBufferReadFail => String::from("failed to read pixels from GPU to CPU"),
            Error::AccelBuild(reason) => {
                format!("failed to build acceleration structure: {:?}", reason)
            }
        }
    }
}
