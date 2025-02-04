#[derive(Debug)]
pub enum Error {
    FileNotFound(String),
    TextureToBufferReadFail,
    AccelBuild(String),
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
