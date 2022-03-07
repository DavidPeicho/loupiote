use std::io::Read;

pub fn load_file(filename: &str) -> Vec<u8> {
    let mut f = std::fs::File::open(&filename).expect("no file found");
    let metadata = std::fs::metadata(&filename).expect("unable to read metadata");
    let mut buffer: Vec<u8> = vec![0; metadata.len() as usize];
    f.read(&mut buffer).expect("buffer overflow");
    buffer
}

pub fn load_file_u32(filename: &str) -> Option<Vec<u32>> {
    let mut f = std::fs::File::open(&filename).expect("no file found");
    let metadata = std::fs::metadata(&filename).expect("unable to read metadata");
    let mut buffer: Vec<u8> = vec![0; metadata.len() as usize];
    f.read(&mut buffer).expect("buffer overflow");

    let nb_bytes = metadata.len() as usize;

    match nb_bytes % std::mem::size_of::<u32>() {
        0 => {
            let v_from_raw = unsafe {
                // Ensure the original vector is not dropped.
                let mut v_clone = std::mem::ManuallyDrop::new(buffer);
                Vec::from_raw_parts(
                    v_clone.as_mut_ptr() as *mut u32,
                    v_clone.len(),
                    v_clone.capacity(),
                )
            };
            Some(v_from_raw)
        }
        _ => None,
    }
}
