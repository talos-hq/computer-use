//! Small owning xkb API over runtime-loaded entry points. A missing library
//! refuses keyboard work before input; it cannot prevent Wayland capture/startup.
#![allow(non_snake_case)]
use anyhow::{anyhow, Result};
use std::{
    ffi::{CStr, CString},
    ptr::NonNull,
    sync::OnceLock,
};
pub(crate) use xkeysym::{KeyCode as Keycode, Keysym};
pub(crate) mod ffi {
    #![allow(non_camel_case_types)]
    pub enum xkb_context {}
    pub enum xkb_keymap {}
    pub enum xkb_state {}
}
use ffi::*;
use std::os::raw::{c_char, c_int};
pub(crate) const CONTEXT_NO_DEFAULT_INCLUDES: u32 = 1;
pub(crate) const CONTEXT_NO_ENVIRONMENT_NAMES: u32 = 2;
pub(crate) const KEYMAP_FORMAT_TEXT_V1: u32 = 1;
pub(crate) const KEYMAP_COMPILE_NO_FLAGS: u32 = 0;
pub(crate) const KEYSYM_NO_FLAGS: u32 = 0;
pub(crate) const KEYSYM_CASE_INSENSITIVE: u32 = 1;
pub(crate) const STATE_MODS_DEPRESSED: u32 = 1;
pub(crate) const STATE_MODS_LATCHED: u32 = 2;
pub(crate) const STATE_MODS_LOCKED: u32 = 4;
pub(crate) const STATE_MODS_EFFECTIVE: u32 = 8;
pub(crate) const STATE_LAYOUT_EFFECTIVE: u32 = 128;
pub(crate) const MOD_NAME_CTRL: &str = "Control";
pub(crate) const MOD_NAME_SHIFT: &str = "Shift";
pub(crate) const MOD_NAME_ALT: &str = "Mod1";
pub(crate) const MOD_NAME_LOGO: &str = "Mod4";
pub(crate) const MOD_NAME_CAPS: &str = "Lock";
pub(crate) const MOD_NAME_NUM: &str = "Mod2";
pub(crate) enum KeyDirection {
    Down = 1,
}

// FFI signatures follow xkbcommon 0.9 (MIT), with scalar aliases expanded.
struct Api {
    xkb_context_new: unsafe extern "C" fn(u32) -> *mut xkb_context,
    xkb_context_unref: unsafe extern "C" fn(*mut xkb_context),
    xkb_keymap_new_from_string:
        unsafe extern "C" fn(*mut xkb_context, *const c_char, u32, u32) -> *mut xkb_keymap,
    xkb_keymap_unref: unsafe extern "C" fn(*mut xkb_keymap),
    xkb_keymap_min_keycode: unsafe extern "C" fn(*mut xkb_keymap) -> u32,
    xkb_keymap_max_keycode: unsafe extern "C" fn(*mut xkb_keymap) -> u32,
    xkb_keymap_num_mods: unsafe extern "C" fn(*mut xkb_keymap) -> u32,
    xkb_keymap_mod_get_name: unsafe extern "C" fn(*mut xkb_keymap, u32) -> *const c_char,
    xkb_keymap_mod_get_index: unsafe extern "C" fn(*mut xkb_keymap, *const c_char) -> u32,
    xkb_keymap_num_layouts: unsafe extern "C" fn(*mut xkb_keymap) -> u32,
    xkb_keymap_num_layouts_for_key: unsafe extern "C" fn(*mut xkb_keymap, u32) -> u32,
    xkb_keymap_num_levels_for_key: unsafe extern "C" fn(*mut xkb_keymap, u32, u32) -> u32,
    xkb_keymap_key_get_syms_by_level:
        unsafe extern "C" fn(*mut xkb_keymap, u32, u32, u32, *mut *const u32) -> c_int,
    xkb_keymap_key_get_name: unsafe extern "C" fn(*mut xkb_keymap, u32) -> *const c_char,
    xkb_state_new: unsafe extern "C" fn(*mut xkb_keymap) -> *mut xkb_state,
    xkb_state_unref: unsafe extern "C" fn(*mut xkb_state),
    xkb_state_update_key: unsafe extern "C" fn(*mut xkb_state, u32, u32) -> u32,
    xkb_state_update_mask:
        unsafe extern "C" fn(*mut xkb_state, u32, u32, u32, u32, u32, u32) -> u32,
    xkb_state_key_get_one_sym: unsafe extern "C" fn(*mut xkb_state, u32) -> u32,
    xkb_state_serialize_mods: unsafe extern "C" fn(*mut xkb_state, u32) -> u32,
    xkb_state_serialize_layout: unsafe extern "C" fn(*mut xkb_state, u32) -> u32,
    xkb_keysym_from_name: unsafe extern "C" fn(*const c_char, u32) -> u32,
    xkb_keysym_get_name: unsafe extern "C" fn(u32, *mut c_char, usize) -> c_int,
    _library: libloading::Library,
}
fn api() -> Result<&'static Api> {
    static API: OnceLock<Result<Api>> = OnceLock::new();
    API.get_or_init(|| {
        // SAFETY: xkbcommon has no load-time preconditions.
        let library = unsafe { crate::runtime_library::open("libxkbcommon.so.0") }?;
        Ok(Api {
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_context_new: *unsafe { library.get(b"xkb_context_new\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_context_unref: *unsafe { library.get(b"xkb_context_unref\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keymap_new_from_string: *unsafe { library.get(b"xkb_keymap_new_from_string\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keymap_unref: *unsafe { library.get(b"xkb_keymap_unref\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keymap_min_keycode: *unsafe { library.get(b"xkb_keymap_min_keycode\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keymap_max_keycode: *unsafe { library.get(b"xkb_keymap_max_keycode\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keymap_num_mods: *unsafe { library.get(b"xkb_keymap_num_mods\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keymap_mod_get_name: *unsafe { library.get(b"xkb_keymap_mod_get_name\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keymap_mod_get_index: *unsafe { library.get(b"xkb_keymap_mod_get_index\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keymap_num_layouts: *unsafe { library.get(b"xkb_keymap_num_layouts\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keymap_num_layouts_for_key: *unsafe {
                library.get(b"xkb_keymap_num_layouts_for_key\0")
            }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keymap_num_levels_for_key: *unsafe {
                library.get(b"xkb_keymap_num_levels_for_key\0")
            }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keymap_key_get_syms_by_level: *unsafe {
                library.get(b"xkb_keymap_key_get_syms_by_level\0")
            }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keymap_key_get_name: *unsafe { library.get(b"xkb_keymap_key_get_name\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_state_new: *unsafe { library.get(b"xkb_state_new\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_state_unref: *unsafe { library.get(b"xkb_state_unref\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_state_update_key: *unsafe { library.get(b"xkb_state_update_key\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_state_update_mask: *unsafe { library.get(b"xkb_state_update_mask\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_state_key_get_one_sym: *unsafe { library.get(b"xkb_state_key_get_one_sym\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_state_serialize_mods: *unsafe { library.get(b"xkb_state_serialize_mods\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_state_serialize_layout: *unsafe { library.get(b"xkb_state_serialize_layout\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keysym_from_name: *unsafe { library.get(b"xkb_keysym_from_name\0") }?,
            // SAFETY: the fixed C ABI above matches the symbol; Api owns its library.
            xkb_keysym_get_name: *unsafe { library.get(b"xkb_keysym_get_name\0") }?,
            _library: library,
        })
    })
    .as_ref()
    .map_err(|error| anyhow!("{error:#}"))
}
pub(crate) fn available() -> Result<()> {
    api().map(|_| ())
}

pub(crate) struct Context {
    ptr: NonNull<xkb_context>,
    api: &'static Api,
}
impl Context {
    pub(crate) fn new(flags: u32) -> Result<Self> {
        let api = api()?;
        // SAFETY: flags are xkb context flags, no borrowed data.
        let ptr = NonNull::new(unsafe { (api.xkb_context_new)(flags) })
            .ok_or_else(|| anyhow!("xkb_context_new failed"))?;
        Ok(Self { ptr, api })
    }
}
impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns one context reference.
        unsafe { (self.api.xkb_context_unref)(self.ptr.as_ptr()) }
    }
}

pub(crate) struct Keymap {
    ptr: NonNull<xkb_keymap>,
    api: &'static Api,
}
impl Keymap {
    pub(crate) fn new_from_string(
        context: &Context,
        map: String,
        format: u32,
        flags: u32,
    ) -> Option<Self> {
        let map = CString::new(map).ok()?;
        // SAFETY: live context and terminated keymap, no pointer retained into map.
        NonNull::new(unsafe {
            (context.api.xkb_keymap_new_from_string)(
                context.ptr.as_ptr(),
                map.as_ptr(),
                format,
                flags,
            )
        })
        .map(|ptr| Self {
            ptr,
            api: context.api,
        })
    }
    pub(crate) fn get_raw_ptr(&self) -> *mut xkb_keymap {
        self.ptr.as_ptr()
    }
    pub(crate) fn mod_get_index(&self, name: &str) -> u32 {
        let Ok(name) = CString::new(name) else {
            return u32::MAX;
        };
        // SAFETY: live keymap and terminated name.
        unsafe { (self.api.xkb_keymap_mod_get_index)(self.ptr.as_ptr(), name.as_ptr()) }
    }
    pub(crate) fn mod_get_name(&self, index: u32) -> &str {
        // SAFETY: live keymap; returned string belongs to it.
        let ptr = unsafe { (self.api.xkb_keymap_mod_get_name)(self.ptr.as_ptr(), index) };
        if ptr.is_null() {
            return "";
        }
        unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("")
    }
    pub(crate) fn key_get_name(&self, code: Keycode) -> Option<&str> {
        // SAFETY: live keymap; returned string belongs to it.
        let ptr = unsafe { (self.api.xkb_keymap_key_get_name)(self.ptr.as_ptr(), code.raw()) };
        if ptr.is_null() {
            return None;
        }
        unsafe { CStr::from_ptr(ptr) }.to_str().ok()
    }
    pub(crate) fn key_get_syms_by_level(
        &self,
        code: Keycode,
        layout: u32,
        level: u32,
    ) -> Vec<Keysym> {
        let mut ptr = std::ptr::null();
        // SAFETY: live keymap and writable out-pointer; returned slice belongs to map.
        let count = unsafe {
            (self.api.xkb_keymap_key_get_syms_by_level)(
                self.ptr.as_ptr(),
                code.raw(),
                layout,
                level,
                &mut ptr,
            )
        };
        if count <= 0 || ptr.is_null() {
            return Vec::new();
        }
        unsafe { std::slice::from_raw_parts(ptr, count as usize) }
            .iter()
            .copied()
            .map(Keysym::new)
            .collect()
    }
    pub(crate) fn min_keycode(&self) -> Keycode {
        // SAFETY: live keymap, scalar indices are checked by xkbcommon.
        Keycode::new(unsafe { (self.api.xkb_keymap_min_keycode)(self.ptr.as_ptr()) })
    }
    pub(crate) fn max_keycode(&self) -> Keycode {
        // SAFETY: live keymap, scalar indices are checked by xkbcommon.
        Keycode::new(unsafe { (self.api.xkb_keymap_max_keycode)(self.ptr.as_ptr()) })
    }
    pub(crate) fn num_mods(&self) -> u32 {
        // SAFETY: live keymap, scalar indices are checked by xkbcommon.
        unsafe { (self.api.xkb_keymap_num_mods)(self.ptr.as_ptr()) }
    }
    pub(crate) fn num_layouts(&self) -> u32 {
        // SAFETY: live keymap, scalar indices are checked by xkbcommon.
        unsafe { (self.api.xkb_keymap_num_layouts)(self.ptr.as_ptr()) }
    }
    pub(crate) fn num_layouts_for_key(&self, code: Keycode) -> u32 {
        // SAFETY: live keymap, scalar indices are checked by xkbcommon.
        unsafe { (self.api.xkb_keymap_num_layouts_for_key)(self.ptr.as_ptr(), code.raw()) }
    }
    pub(crate) fn num_levels_for_key(&self, code: Keycode, layout: u32) -> u32 {
        // SAFETY: live keymap, scalar indices are checked by xkbcommon.
        unsafe { (self.api.xkb_keymap_num_levels_for_key)(self.ptr.as_ptr(), code.raw(), layout) }
    }
}
impl Drop for Keymap {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns one keymap reference.
        unsafe { (self.api.xkb_keymap_unref)(self.ptr.as_ptr()) }
    }
}
pub(crate) struct State {
    ptr: NonNull<xkb_state>,
    api: &'static Api,
}
impl State {
    pub(crate) fn new(map: &Keymap) -> Option<Self> {
        // SAFETY: live keymap; state takes its own map reference.
        NonNull::new(unsafe { (map.api.xkb_state_new)(map.ptr.as_ptr()) })
            .map(|ptr| Self { ptr, api: map.api })
    }
    pub(crate) fn update_key(&mut self, code: Keycode, direction: KeyDirection) {
        // SAFETY: exclusively borrowed state, scalar key and direction.
        unsafe {
            (self.api.xkb_state_update_key)(self.ptr.as_ptr(), code.raw(), direction as u32);
        }
    }
    pub(crate) fn update_mask(
        &mut self,
        depressed: u32,
        latched: u32,
        locked: u32,
        depressed_layout: u32,
        latched_layout: u32,
        locked_layout: u32,
    ) {
        // SAFETY: exclusively borrowed state and scalar masks.
        unsafe {
            (self.api.xkb_state_update_mask)(
                self.ptr.as_ptr(),
                depressed,
                latched,
                locked,
                depressed_layout,
                latched_layout,
                locked_layout,
            );
        }
    }
    pub(crate) fn key_get_one_sym(&self, code: Keycode) -> Keysym {
        // SAFETY: live state and scalar keycode.
        Keysym::new(unsafe { (self.api.xkb_state_key_get_one_sym)(self.ptr.as_ptr(), code.raw()) })
    }
    pub(crate) fn serialize_mods(&self, components: u32) -> u32 {
        // SAFETY: live state and scalar component mask.
        unsafe { (self.api.xkb_state_serialize_mods)(self.ptr.as_ptr(), components) }
    }
    pub(crate) fn serialize_layout(&self, components: u32) -> u32 {
        // SAFETY: live state and scalar component mask.
        unsafe { (self.api.xkb_state_serialize_layout)(self.ptr.as_ptr(), components) }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns one state reference.
        unsafe { (self.api.xkb_state_unref)(self.ptr.as_ptr()) }
    }
}
pub(crate) fn keysym_from_name(name: &str, flags: u32) -> Keysym {
    let (Ok(api), Ok(name)) = (api(), CString::new(name)) else {
        return Keysym::new(0);
    };
    // SAFETY: terminated name and valid flags.
    Keysym::new(unsafe { (api.xkb_keysym_from_name)(name.as_ptr(), flags) })
}
pub(crate) fn keysym_get_name(sym: Keysym) -> String {
    let Ok(api) = api() else { return String::new() };
    let mut bytes = [0u8; 128];
    // SAFETY: writable buffer with stated length.
    let len =
        unsafe { (api.xkb_keysym_get_name)(sym.raw(), bytes.as_mut_ptr().cast(), bytes.len()) };
    if len <= 0 || len as usize >= bytes.len() {
        return String::new();
    }
    String::from_utf8_lossy(&bytes[..len as usize]).into_owned()
}
