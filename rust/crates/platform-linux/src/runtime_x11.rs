//! Xlib/XInput entry points, loaded only when an X11 input path is used.
#![allow(non_snake_case, unused_imports)]
use anyhow::{anyhow, Result};
use std::sync::OnceLock;

macro_rules! functions {
    ($soname:literal, { $(fn $name:ident($($arg:ident: $ty:ty),*) -> $ret:ty;)* }) => {
        struct Api { $($name: unsafe extern "C" fn($($ty),*) -> $ret,)* _library: libloading::Library }
        fn api() -> Result<&'static Api> {
            static API: OnceLock<Result<Api>> = OnceLock::new();
            API.get_or_init(|| {
                // SAFETY: Xlib/XInput have no load-time preconditions.
                let library = unsafe { crate::runtime_library::open($soname) }?;
                $(
                    const _: unsafe extern "C" fn($($ty),*) -> $ret = x11_binding::$name;
                    // SAFETY: ABI checked above; Api retains the library.
                    let $name = *unsafe { library.get::<unsafe extern "C" fn($($ty),*) -> $ret>(concat!(stringify!($name), "\0").as_bytes()) }?;
                )*
                Ok(Api { $($name,)* _library: library })
            }).as_ref().map_err(|error| anyhow!("{error:#}"))
        }
        pub(super) fn available() -> Result<()> { api().map(|_| ()) }
        $(
            /// # Safety
            /// Same pointer and lifetime requirements as the corresponding Xlib call.
            pub(crate) unsafe fn $name($($arg: $ty),*) -> $ret {
                // All display-taking callers are gated by open_named_display's
                // available() checks. Returning zero also makes XInitThreads /
                // XOpenDisplay fail safely if called before that gate.
                match api() {
                    Ok(api) => unsafe { (api.$name)($($arg),*) },
                    Err(_) => Default::default(),
                }
            }
        )*
    };
}

pub(crate) fn available() -> Result<()> {
    xlib::available()?;
    xinput2::available()
}

pub fn diagnostics() -> Vec<(&'static str, Result<()>)> {
    vec![
        ("libX11.so.6", xlib::available()),
        ("libXi.so.6", xinput2::available()),
    ]
}

pub(crate) mod xlib {
    use super::*;
    use std::os::raw::*;
    use x11::xlib as x11_binding;
    pub(crate) use x11::xlib::*;
    functions! { "libX11.so.6", {
        fn XChangeProperty(_8: *mut Display, _7: c_ulong, _6: c_ulong, _5: c_ulong, _4: c_int, _3: c_int, _2: *const c_uchar, _1: c_int) -> c_int;
        fn XCheckWindowEvent(_4: *mut Display, _3: c_ulong, _2: c_long, _1: *mut XEvent) -> c_int;
        fn XCloseDisplay(_1: *mut Display) -> c_int;
        fn XConnectionNumber(_1: *mut Display) -> c_int;
        fn XCreateSimpleWindow(_9: *mut Display, _8: c_ulong, _7: c_int, _6: c_int, _5: c_uint, _4: c_uint, _3: c_uint, _2: c_ulong, _1: c_ulong) -> c_ulong;
        fn XDefaultRootWindow(_1: *mut Display) -> c_ulong;
        fn XDefaultScreen(_1: *mut Display) -> c_int;
        fn XDestroyWindow(_2: *mut Display, _1: c_ulong) -> c_int;
        fn XDisplayHeight(_2: *mut Display, _1: c_int) -> c_int;
        fn XDisplayString(_1: *mut Display) -> *mut c_char;
        fn XDisplayWidth(_2: *mut Display, _1: c_int) -> c_int;
        fn XFlush(_1: *mut Display) -> c_int;
        fn XFree(_1: *mut c_void) -> c_int;
        fn XFreeEventData(_2: *mut Display, _1: *mut XGenericEventCookie) -> ();
        fn XGetEventData(_2: *mut Display, _1: *mut XGenericEventCookie) -> c_int;
        fn XGetGeometry(_9: *mut Display, _8: c_ulong, _7: *mut c_ulong, _6: *mut c_int, _5: *mut c_int, _4: *mut c_uint, _3: *mut c_uint, _2: *mut c_uint, _1: *mut c_uint) -> c_int;
        fn XGetInputFocus(_3: *mut Display, _2: *mut c_ulong, _1: *mut c_int) -> c_int;
        fn XGetWindowAttributes(_3: *mut Display, _2: c_ulong, _1: *mut XWindowAttributes) -> c_int;
        fn XGetWindowProperty(_12: *mut Display, _11: c_ulong, _10: c_ulong, _9: c_long, _8: c_long, _7: c_int, _6: c_ulong, _5: *mut c_ulong, _4: *mut c_int, _3: *mut c_ulong, _2: *mut c_ulong, _1: *mut *mut c_uchar) -> c_int;
        fn XGrabServer(_1: *mut Display) -> c_int;
        fn XInitThreads() -> c_int;
        fn XInternAtom(_3: *mut Display, _2: *const c_char, _1: c_int) -> c_ulong;
        fn XNextEvent(_2: *mut Display, _1: *mut XEvent) -> c_int;
        fn XOpenDisplay(_1: *const c_char) -> *mut Display;
        fn XPending(_1: *mut Display) -> c_int;
        fn XQueryExtension(_5: *mut Display, _4: *const c_char, _3: *mut c_int, _2: *mut c_int, _1: *mut c_int) -> c_int;
        fn XQueryTree(_6: *mut Display, _5: c_ulong, _4: *mut c_ulong, _3: *mut c_ulong, _2: *mut *mut c_ulong, _1: *mut c_uint) -> c_int;
        fn XSelectInput(_3: *mut Display, _2: c_ulong, _1: c_long) -> c_int;
        fn XSendEvent(_5: *mut Display, _4: c_ulong, _3: c_int, _2: c_long, _1: *mut XEvent) -> c_int;
        fn XServerVendor(_1: *mut Display) -> *mut c_char;
        fn XSetErrorHandler(_1: Option<unsafe extern "C" fn (*mut Display, *mut XErrorEvent) -> c_int>) -> Option<unsafe extern "C" fn (*mut Display, *mut XErrorEvent) -> c_int>;
        fn XSetInputFocus(_4: *mut Display, _3: c_ulong, _2: c_int, _1: c_ulong) -> c_int;
        fn XSync(_2: *mut Display, _1: c_int) -> c_int;
        fn XTranslateCoordinates(_8: *mut Display, _7: c_ulong, _6: c_ulong, _5: c_int, _4: c_int, _3: *mut c_int, _2: *mut c_int, _1: *mut c_ulong) -> c_int;
        fn XUngrabServer(_1: *mut Display) -> c_int;
        fn XkbAllocKeyboard() -> XkbDescPtr;
        fn XkbChangeEnabledControls(_4: *mut Display, _3: c_uint, _2: c_uint, _1: c_uint) -> c_int;
        fn XkbFreeKeyboard(_3: XkbDescPtr, _2: c_uint, _1: c_int) -> ();
        fn XkbGetControls(_3: *mut Display, _2: c_ulong, _1: XkbDescPtr) -> c_int;
        fn XkbLatchModifiers(_4: *mut Display, _3: c_uint, _2: c_uint, _1: c_uint) -> c_int;
        fn XkbLockModifiers(_4: *mut Display, _3: c_uint, _2: c_uint, _1: c_uint) -> c_int;
    }}
}

pub(crate) mod xinput2 {
    use super::*;
    use std::os::raw::*;
    use x11::xinput2 as x11_binding;
    pub(crate) use x11::xinput2::*;
    use x11::xlib::*;
    functions! { "libXi.so.6", {
        fn XIAllowEvents(_4: *mut Display, _3: c_int, _2: c_int, _1: c_ulong) -> c_int;
        fn XIChangeHierarchy(_3: *mut Display, _2: *mut XIAnyHierarchyChangeInfo, _1: c_int) -> c_int;
        fn XIChangeProperty(_8: *mut Display, _7: c_int, _6: c_ulong, _5: c_ulong, _4: c_int, _3: c_int, _2: *mut c_uchar, _1: c_int) -> ();
        fn XIFreeDeviceInfo(_1: *mut XIDeviceInfo) -> ();
        fn XIGetFocus(_3: *mut Display, _2: c_int, _1: *mut c_ulong) -> c_int;
        fn XIGetProperty(_12: *mut Display, _11: c_int, _10: c_ulong, _9: c_long, _8: c_long, _7: c_int, _6: c_ulong, _5: *mut c_ulong, _4: *mut c_int, _3: *mut c_ulong, _2: *mut c_ulong, _1: *mut *mut c_uchar) -> c_int;
        fn XIQueryDevice(_3: *mut Display, _2: c_int, _1: *mut c_int) -> *mut XIDeviceInfo;
        fn XIQueryPointer(_12: *mut Display, _11: c_int, _10: c_ulong, _9: *mut c_ulong, _8: *mut c_ulong, _7: *mut c_double, _6: *mut c_double, _5: *mut c_double, _4: *mut c_double, _3: *mut XIButtonState, _2: *mut XIModifierState, _1: *mut XIModifierState) -> c_int;
        fn XIQueryVersion(_3: *mut Display, _2: *mut c_int, _1: *mut c_int) -> c_int;
        fn XISelectEvents(_4: *mut Display, _3: c_ulong, _2: *mut XIEventMask, _1: c_int) -> c_int;
        fn XISetFocus(_4: *mut Display, _3: c_int, _2: c_ulong, _1: c_ulong) -> c_int;
        fn XIWarpPointer(_10: *mut Display, _9: c_int, _8: c_ulong, _7: c_ulong, _6: c_double, _5: c_double, _4: c_uint, _3: c_uint, _2: c_double, _1: c_double) -> c_int;
    }}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a disposable X11 display (Xvfb)"]
    fn runtime_tables_open_a_display_and_query_xinput() {
        available().unwrap();
        // SAFETY: initialize Xlib before opening the test's only display.
        assert_ne!(unsafe { xlib::XInitThreads() }, 0);
        // SAFETY: null selects DISPLAY; the returned handle is owned below.
        let display = unsafe { xlib::XOpenDisplay(std::ptr::null()) };
        assert!(!display.is_null());
        struct Display(*mut xlib::Display);
        impl Drop for Display {
            fn drop(&mut self) {
                // SAFETY: owns the live display, no other references escape.
                unsafe { xlib::XCloseDisplay(self.0) };
            }
        }
        let display = Display(display);
        let (mut major, mut minor) = (2, 0);
        // SAFETY: live display and writable version numbers.
        assert_eq!(
            unsafe { xinput2::XIQueryVersion(display.0, &mut major, &mut minor) },
            0
        );
        let mut count = 0;
        // SAFETY: live display and writable count; XInput owns the returned array.
        let devices =
            unsafe { xinput2::XIQueryDevice(display.0, xinput2::XIAllDevices, &mut count) };
        assert!(!devices.is_null());
        assert!(count > 0);
        // SAFETY: frees exactly the array returned by XIQueryDevice.
        unsafe { xinput2::XIFreeDeviceInfo(devices) };
    }
}
