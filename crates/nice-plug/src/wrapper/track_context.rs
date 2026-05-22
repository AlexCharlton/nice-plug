//! Shared storage for track/channel context information provided by the host.

use clap_sys::ext::track_info::{CLAP_TRACK_INFO_HAS_TRACK_NAME, clap_track_info};
use clap_sys::string_sizes::CLAP_NAME_SIZE;
use parking_lot::RwLock;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::Arc;

/// Thread-safe storage for the current track name, when the host provides one.
#[derive(Debug, Default)]
pub(crate) struct SharedTrackContext {
    name: RwLock<Option<String>>,
}

impl SharedTrackContext {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set_name(&self, name: Option<String>) {
        *self.name.write() = name;
    }

    pub fn name(&self) -> Option<String> {
        self.name.read().clone()
    }
}

/// Parse a track name from a [`clap_track_info`] struct.
pub(crate) fn name_from_clap_track_info(info: &clap_track_info) -> Option<String> {
    if info.flags & CLAP_TRACK_INFO_HAS_TRACK_NAME == 0 {
        return None;
    }

    c_char_buffer_to_string(&info.name)
}

fn c_char_buffer_to_string(buffer: &[c_char]) -> Option<String> {
    // SAFETY: `buffer` is NUL-terminated per the CLAP ABI.
    let cstr = unsafe { CStr::from_ptr(buffer.as_ptr()) };
    let name = cstr.to_str().ok()?.to_owned();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

const _: () = assert!(CLAP_NAME_SIZE > 0);
