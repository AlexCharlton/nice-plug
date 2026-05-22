#[macro_use]
mod util;

mod context;
mod factory;
mod inner;
mod note_expressions;
mod param_units;
pub mod subcategories;
mod view;
mod wrapper;

/// Re-export for the wrapper.
pub use factory::PluginInfo;
use nice_plug_core::plugin::Plugin;
pub use vst3;
pub use wrapper::Wrapper;

use crate::wrapper::vst3::subcategories::Vst3SubCategory;

/// Provides auxiliary metadata needed for a VST3 plugin.
pub trait Vst3Plugin: Plugin {
    /// The unique class ID that identifies this particular plugin. You can use the
    /// `*b"fooofooofooofooo"` syntax for this.
    ///
    /// This will be shuffled into a different byte order on Windows for project-compatibility.
    const VST3_CLASS_ID: [u8; 16];
    /// One or more subcategories. The host may use these to categorize the plugin. Internally this
    /// slice will be converted to a string where each character is separated by a pipe character
    /// (`|`). This string has a limit of 127 characters, and anything longer than that will be
    /// truncated.
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory];

    /// [`VST3_CLASS_ID`][Self::VST3_CLASS_ID`] in the correct order for the current platform so
    /// projects and presets can be shared between platforms. This should not be overridden.
    const PLATFORM_VST3_CLASS_ID: [u8; 16] = swap_vst3_uid_byte_order(Self::VST3_CLASS_ID);
}

#[cfg(not(target_os = "windows"))]
const fn swap_vst3_uid_byte_order(uid: [u8; 16]) -> [u8; 16] {
    uid
}

#[cfg(target_os = "windows")]
const fn swap_vst3_uid_byte_order(mut uid: [u8; 16]) -> [u8; 16] {
    // No mutable references in const functions, so we can't use `uid.swap()`
    let original_uid = uid;

    uid[0] = original_uid[3];
    uid[1] = original_uid[2];
    uid[2] = original_uid[1];
    uid[3] = original_uid[0];

    uid[4] = original_uid[5];
    uid[5] = original_uid[4];
    uid[6] = original_uid[7];
    uid[7] = original_uid[6];

    uid
}

/// Export one or more VST3 plugins from this library using the provided plugin types. The first
/// plugin's vendor information is used for the factory's information.
#[macro_export]
macro_rules! nice_export_vst3 {
    ($($plugin_ty:ty),+) => {
        // Earlier versions used a simple generic struct for this, but because we don't have
        // variadic generics (yet) we can't generate the struct for multiple plugin types without
        // macros. So instead we'll generate the implementation ad-hoc inside of this macro.
        #[doc(hidden)]
        mod vst3_factory {
            use ::std::collections::HashSet;
            use ::std::ffi::c_void;

            use $crate::wrapper::vst3::{PluginInfo, Wrapper};
            use $crate::wrapper::vst3::vst3::{Class, ComWrapper, Steinberg::*};

            // Because the `$plugin_ty`s are likely defined in the enclosing scope. This works even
            // if the types are not public because this is a child module.
            use super::*;

            // Sneaky way to get the number of expanded elements
            const PLUGIN_COUNT: usize = [$(stringify!($plugin_ty)),+].len();

            #[doc(hidden)]
            pub struct Factory {
                // This is a type erased version of the information stored on the plugin types
                plugin_infos: [PluginInfo; PLUGIN_COUNT],
            }

            impl Class for Factory {
                type Interfaces = (IPluginFactory, IPluginFactory2, IPluginFactory3);
            }

            impl Factory {
                pub fn new() -> Factory {
                    let plugin_infos = [$(PluginInfo::for_plugin::<$plugin_ty>()),+];

                    if cfg!(debug_assertions) {
                        let unique_cids: HashSet<[u8; 16]> =
                            plugin_infos.iter().map(|d| *d.cid).collect();
                        $crate::nice_debug_assert_eq!(
                            unique_cids.len(),
                            plugin_infos.len(),
                            "Duplicate VST3 class IDs found in `nice_export_vst3!()` call"
                        );
                    }

                    Factory { plugin_infos }
                }
            }

            impl IPluginFactoryTrait for Factory {
                unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
                    if info.is_null() {
                        return kInvalidArgument;
                    }

                    // We'll use the first plugin's info for this
                    unsafe { *info = self.plugin_infos[0].create_factory_info(); }

                    kResultOk
                }

                unsafe fn countClasses(&self) -> i32 {
                    self.plugin_infos.len() as i32
                }

                unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
                    if index < 0 || index >= self.plugin_infos.len() as i32 {
                        return kInvalidArgument;
                    }

                    unsafe { *info = self.plugin_infos[index as usize].create_class_info(); }

                    kResultOk
                }

                unsafe fn createInstance(
                    &self,
                    cid: FIDString,
                    iid: FIDString,
                    obj: *mut *mut c_void,
                ) -> tresult {
                    if cid.is_null() || obj.is_null() {
                        return kInvalidArgument;
                    }

                    let cid = *(cid as *const TUID);
                    let cid_bytes: [u8; 16] = cid.map(|b| b as u8);

                    // This is a poor man's way of treating `$plugin_ty` like an indexable array.
                    // Assuming `self.plugin_infos` is in the same order, we can simply check all of
                    // the registered plugin CIDs for matches using an unrolled loop.
                    let mut plugin_idx = 0;
                    $({
                        let plugin_info = &self.plugin_infos[plugin_idx];
                        if cid_bytes == *plugin_info.cid {
                            let instance = ComWrapper::new(Wrapper::<$plugin_ty>::new())
                                .to_com_ptr::<FUnknown>()
                                .unwrap();
                            let ptr = instance.as_ptr();
                            // 99.999% of the times `iid` will be that of `IComponent`, but the
                            // caller is technically allowed to create an object for any support
                            // interface. We don't have a way to check whether our plugin supports
                            // the interface without creating it, but since the odds that a caller
                            // will create an object with an interface we don't support are
                            // basically zero this is not a problem.
                            return unsafe {
                                ((*(*ptr).vtbl).queryInterface)(ptr, iid as *mut TUID, obj)
                            };
                        }

                        plugin_idx += 1;
                    })+

                    kInvalidArgument
                }
            }

            impl IPluginFactory2Trait for Factory {
                unsafe fn getClassInfo2(&self, index: i32, info: *mut PClassInfo2) -> tresult {
                    if index < 0 || index >= self.plugin_infos.len() as i32 {
                        return kInvalidArgument;
                    }

                    unsafe { *info = self.plugin_infos[index as usize].create_class_info_2(); }

                    kResultOk
                }
            }

            impl IPluginFactory3Trait for Factory {
                unsafe fn getClassInfoUnicode(
                    &self,
                    index: i32,
                    info: *mut PClassInfoW,
                ) -> tresult {
                    if index < 0 || index >= self.plugin_infos.len() as i32 {
                        return kInvalidArgument;
                    }

                    unsafe { *info = self.plugin_infos[index as usize].create_class_info_unicode(); }

                    kResultOk
                }

                unsafe fn setHostContext(&self, _context: *mut FUnknown) -> tresult {
                    // We don't need to do anything with this
                    kResultOk
                }
            }
        }

        /// The VST3 plugin factory entry point.
        #[unsafe(no_mangle)]
        pub extern "system" fn GetPluginFactory(
        ) -> *mut $crate::wrapper::vst3::vst3::Steinberg::IPluginFactory {
            use $crate::wrapper::vst3::vst3::{Class, ComWrapper, Steinberg::IPluginFactory};

            ComWrapper::new(self::vst3_factory::Factory::new())
                .to_com_ptr::<IPluginFactory>()
                .unwrap()
                .into_raw()
        }

        // These two entry points are used on Linux, and they would theoretically also be used on
        // the BSDs:
        // https://github.com/steinbergmedia/vst3_public_sdk/blob/c3948deb407bdbff89de8fb6ab8500ea4df9d6d9/source/main/linuxmain.cpp#L47-L52
        #[allow(missing_docs)]
        #[unsafe(no_mangle)]
        #[cfg(all(target_family = "unix", not(target_os = "macos")))]
        pub extern "C" fn ModuleEntry(_lib_handle: *mut ::std::ffi::c_void) -> bool {
            $crate::wrapper::setup_logger();
            true
        }

        #[allow(missing_docs)]
        #[unsafe(no_mangle)]
        #[cfg(all(target_family = "unix", not(target_os = "macos")))]
        pub extern "C" fn ModuleExit() -> bool {
            true
        }

        // These two entry points are used on macOS:
        // https://github.com/steinbergmedia/vst3_public_sdk/blob/bc459feee68803346737901471441fd4829ec3f9/source/main/macmain.cpp#L60-L61
        #[allow(missing_docs)]
        #[unsafe(no_mangle)]
        #[cfg(target_os = "macos")]
        pub extern "C" fn bundleEntry(_lib_handle: *mut ::std::ffi::c_void) -> bool {
            $crate::wrapper::setup_logger();
            true
        }

        #[allow(missing_docs)]
        #[unsafe(no_mangle)]
        #[cfg(target_os = "macos")]
        pub extern "C" fn bundleExit() -> bool {
            true
        }

        // And these two entry points are used on Windows:
        // https://github.com/steinbergmedia/vst3_public_sdk/blob/bc459feee68803346737901471441fd4829ec3f9/source/main/dllmain.cpp#L59-L60
        #[allow(missing_docs)]
        #[unsafe(no_mangle)]
        #[cfg(target_os = "windows")]
        pub extern "system" fn InitDll() -> bool {
            $crate::wrapper::setup_logger();
            true
        }

        #[allow(missing_docs)]
        #[unsafe(no_mangle)]
        #[cfg(target_os = "windows")]
        pub extern "system" fn ExitDll() -> bool {
            true
        }
    };
}
