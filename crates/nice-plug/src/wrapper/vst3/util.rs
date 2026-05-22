use std::cmp;
use std::ffi::CStr;
use std::mem;
use std::ops::Deref;
use vst3::{ComPtr, ComRef, Interface, Steinberg::*, Steinberg::FIDString, Steinberg::Vst::*};
use widestring::{U16CStr, U16CString};

/// Parse the channel (track) name from a VST3 channel context attribute list.
pub(crate) unsafe fn channel_name_from_attribute_list(
    list: *mut IAttributeList,
) -> Option<String> {
    if list.is_null() {
        return None;
    }

    let list = ComRef::from_raw(list)?;
    let mut name_buf: String128 = mem::zeroed();
    if list.getString(
        ChannelContext::kChannelNameKey,
        name_buf.as_mut_ptr(),
        mem::size_of::<String128>() as u32,
    ) != kResultTrue
    {
        return None;
    }

    let name = U16CStr::from_ptr_str(name_buf.as_ptr() as *const u16)
        .to_string()
        .ok()?;
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// When `Plugin::MIDI_INPUT` is set to `MidiConfig::MidiCCs` or higher then we'll register 130*16
/// additional parameters to handle MIDI CCs, channel pressure, and pitch bend, in that order.
pub const VST3_MIDI_CCS: u32 = 130;
pub const VST3_MIDI_CHANNELS: u32 = 16;
/// The number of parameters we'll need to register if the plugin accepts MIDI CCs.
pub const VST3_MIDI_NUM_PARAMS: u32 = VST3_MIDI_CCS * VST3_MIDI_CHANNELS;
/// The start of the MIDI CC parameter ranges. We'll print an assertion failure if any of the
/// plugin's parameters overlap with this range. The mapping to a parameter index is
/// `VST3_MIDI_PARAMS_START + (cc_idx + (channel * VST3_MIDI_CCS))`.
pub const VST3_MIDI_PARAMS_START: u32 = VST3_MIDI_PARAMS_END - VST3_MIDI_NUM_PARAMS;
/// The (exclusive) end of the MIDI CC parameter range. Anything above this is reserved by the host.
pub const VST3_MIDI_PARAMS_END: u32 = 1 << 31;

/// Early exit out of a VST3 function when one of the passed pointers is null
macro_rules! check_null_ptr {
    ($ptr:expr $(, $ptrs:expr)* $(, )?) => {
        check_null_ptr_msg!("Null pointer passed to function", $ptr $(, $ptrs)*)
    };
}

/// The same as [`check_null_ptr!`], but with a custom message.
macro_rules! check_null_ptr_msg {
    ($msg:expr, $ptr:expr $(, $ptrs:expr)* $(, )?) => {
        if $ptr.is_null() $(|| $ptrs.is_null())* {
            crate::nice_debug_assert_failure!($msg);
            return kInvalidArgument;
        }
    };
}

/// The same as [`strlcpy()`], but for VST3's fun UTF-16 strings instead.
pub fn u16strlcpy(dest: &mut [TChar], src: &str) {
    if dest.is_empty() {
        return;
    }

    let src_utf16 = match U16CString::from_str(src) {
        Ok(s) => s,
        Err(err) => {
            crate::nice_debug_assert_failure!("Invalid UTF-16 string: {}", err);
            return;
        }
    };
    let src_utf16_chars = src_utf16.as_slice();
    let src_utf16_chars_signed: &[TChar] =
        unsafe { &*(src_utf16_chars as *const [u16] as *const [TChar]) };

    // Make sure there's always room for a null terminator
    let copy_len = cmp::min(dest.len() - 1, src_utf16_chars_signed.len());
    dest[..copy_len].copy_from_slice(&src_utf16_chars_signed[..copy_len]);
    dest[copy_len] = 0;
}

/// Compare a host-provided [`FIDString`] to one of the SDK's string constants. These are plain C
/// string pointers, so pointer equality is not sufficient.
pub(crate) unsafe fn fid_matches(type_: FIDString, expected: FIDString) -> bool {
    unsafe { !type_.is_null() && CStr::from_ptr(type_) == CStr::from_ptr(expected) }
}

/// Send+Sync wrapper for these interface pointers.
#[repr(transparent)]
pub struct VstPtr<I: Interface> {
    ptr: ComPtr<I>,
}

impl<I: Interface> Deref for VstPtr<I> {
    type Target = ComPtr<I>;

    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

impl<I: Interface> From<ComPtr<I>> for VstPtr<I> {
    fn from(ptr: ComPtr<I>) -> Self {
        Self { ptr }
    }
}

/// SAFETY: Sharing these pointers across thread is safe as they have internal atomic reference
/// counting, so as long as a `VstPtr<T>` handle exists the object will stay alive.
unsafe impl<I: Interface + Send + Sync> Send for VstPtr<I> {}
unsafe impl<I: Interface + Send + Sync> Sync for VstPtr<I> {}

#[cfg(test)]
mod miri {
    use widestring::U16CStr;

    use super::*;

    #[test]
    fn u16strlcpy_normal() {
        let mut dest = [0; 256];
        u16strlcpy(&mut dest, "Hello, world!");

        assert_eq!(
            unsafe { U16CStr::from_ptr_str(dest.as_ptr() as *const u16) }
                .to_string()
                .unwrap(),
            "Hello, world!"
        );
    }

    #[test]
    fn u16strlcpy_overflow() {
        let mut dest = [0; 6];
        u16strlcpy(&mut dest, "Hello, world!");

        assert_eq!(
            unsafe { U16CStr::from_ptr_str(dest.as_ptr() as *const u16) }
                .to_string()
                .unwrap(),
            "Hello"
        );
    }
}
