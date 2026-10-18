use crate::{error_response, process_json, MAX_REQUEST_BYTES};
use std::{cell::RefCell, collections::HashMap};

const MAX_ALLOCATIONS: usize = 16;
thread_local! {
    static INPUTS: RefCell<HashMap<u32,Box<[u8]>>> = RefCell::new(HashMap::new());
    static RESPONSE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn vc_abi_version() -> u32 {
    1
}

/// Allocate one zeroed input buffer. Zero signals rejection. At most 16 live buffers.
#[no_mangle]
pub extern "C" fn vc_alloc(len: u32) -> u32 {
    if len == 0 || len as usize > MAX_REQUEST_BYTES {
        return 0;
    }
    INPUTS.with(|inputs| {
        let mut inputs = inputs.borrow_mut();
        if inputs.len() >= MAX_ALLOCATIONS {
            return 0;
        }
        let mut bytes = vec![0u8; len as usize].into_boxed_slice();
        let ptr = bytes.as_mut_ptr() as u32;
        inputs.insert(ptr, bytes);
        ptr
    })
}

/// Only an exact live allocation pointer/length pair is freed; invalid pairs are ignored.
#[no_mangle]
pub extern "C" fn vc_free(ptr: u32, len: u32) {
    INPUTS.with(|inputs| {
        let mut inputs = inputs.borrow_mut();
        if inputs
            .get(&ptr)
            .is_some_and(|bytes| bytes.len() == len as usize)
        {
            inputs.remove(&ptr);
        }
    });
}

/// The returned response belongs to the module, and lasts until the next vc_process call.
#[no_mangle]
pub extern "C" fn vc_process(ptr: u32, len: u32) -> u32 {
    let response = INPUTS.with(|inputs| {
        let inputs = inputs.borrow();
        match inputs.get(&ptr) {
            Some(bytes) if len > 0 && len as usize <= bytes.len() => {
                process_json(&bytes[..len as usize])
            }
            _ => error_response(
                "BAD_INPUT",
                "Input pointer/length does not identify a live allocation",
            ),
        }
    });
    RESPONSE.with(|out| {
        let mut out = out.borrow_mut();
        *out = response.into_bytes();
        out.as_ptr() as u32
    })
}

#[no_mangle]
pub extern "C" fn vc_response_len() -> u32 {
    RESPONSE.with(|out| out.borrow().len() as u32)
}
