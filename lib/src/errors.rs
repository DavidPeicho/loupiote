use image::ImageError;

#[derive(Debug)]
pub enum Error {
    FileNotFound(String),
    TextureToBufferReadFail,
    ImageError(ImageError),
    AccelBuild(String),
}

impl From<Error> for String {
    fn from(e: Error) -> Self {
        match e {
            Error::FileNotFound(filename) => {
                format!("file not found: {}", filename)
            }
            Error::ImageError(e) => {
                format!("file not found: {:?}", e)
            }
            Error::TextureToBufferReadFail => String::from("failed to read pixels from GPU to CPU"),
            Error::AccelBuild(reason) => {
                format!("failed to build acceleration structure: {:?}", reason)
            }
        }
    }
}

impl From<ImageError> for Error {
    fn from(e: ImageError) -> Self {
        Error::ImageError(e)
    }
}
