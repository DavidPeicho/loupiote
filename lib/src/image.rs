pub struct ImageView<'a> {
    pub data: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: u32,
}

impl<'a> ImageView<'a> {
    pub fn new(data: &'a [u8], width: u32, height: u32, bytes_per_pixel: u8) -> Self {
        Self {
            data,
            width,
            height,
            bytes_per_row: bytes_per_pixel as u32 * width,
        }
    }
}
