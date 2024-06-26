use albedo_lib;
use image::ImageError;

#[derive(Debug)]
pub enum Error {
    FileNotFound(String),
    TextureToBufferReadFail,
    ImageError(ImageError),
    AccelBuild(String),
}

impl From<albedo_lib::Error> for Error {
    fn from(e: albedo_lib::Error) -> Self {
        match e {
            albedo_lib::Error::FileNotFound(f) => Error::FileNotFound(f),
            albedo_lib::Error::ImageError(e) => Error::ImageError(e),
            albedo_lib::Error::TextureToBufferReadFail => Error::TextureToBufferReadFail,
            albedo_lib::Error::AccelBuild(reason) => Error::AccelBuild(reason),
        }
    }
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
