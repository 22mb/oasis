//! Exports for the playground page. Text crosses the boundary through one
//! buffer in linear memory: the page writes the document's UTF-8 to the
//! memory `input` returns, then `generate` replaces the buffer with the
//! generated code, or with the error message when it returns false, and
//! `output` and `output_len` locate it.

use std::sync::Mutex;

static BUFFER: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Resizes the buffer to `len` bytes and returns where it starts.
#[unsafe(no_mangle)]
pub extern "C" fn input(len: usize) -> *mut u8 {
    let mut buffer = BUFFER.lock().unwrap();
    *buffer = vec![0; len];
    buffer.as_mut_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn generate() -> bool {
    let mut buffer = BUFFER.lock().unwrap();
    let result = match std::str::from_utf8(&buffer) {
        Ok(source) => {
            oasis_gen::generate(source, oasis_gen::Lang::TypeScript).map_err(|e| e.to_string())
        }
        Err(e) => Err(e.to_string()),
    };
    let ok = result.is_ok();
    *buffer = result.unwrap_or_else(|e| e).into_bytes();
    ok
}

#[unsafe(no_mangle)]
pub extern "C" fn output() -> *const u8 {
    BUFFER.lock().unwrap().as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn output_len() -> usize {
    BUFFER.lock().unwrap().len()
}
