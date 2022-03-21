#[derive(Debug)]
pub enum Error {
    FileNotFound(String),
}

impl From<Error> for String {
    fn from(e: Error) -> Self {
        match e {
            Error::FileNotFound(filename) => {
                format!("file not found: {}", filename)
            }
        }
    }
}
