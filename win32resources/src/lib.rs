//! Provides APIs for accessing Win32 resources, mainly string resources.
use std::borrow::Cow;
use std::ffi::c_void;
use std::ops::Deref;
use std::ptr::null_mut;
use std::slice::from_raw_parts;
use std::sync::OnceLock;

#[allow(clippy::upper_case_acronyms)]
type HINSTANCE = *const c_void;

extern "system" {
    fn LoadStringW(hInstance: HINSTANCE, uID: u32, lpBuffer: *mut u16, cchBufferMax: i32) -> i32;
}

extern "C" {
    static __ImageBase: [u8; 0];
}

fn module_base() -> *const c_void {
    unsafe { (&__ImageBase) as *const [u8; 0] as *const c_void }
}

/*
#[macro_export]
macro_rules! def_image_base {
    (
        $fn_name:ident
    ) => {
        fn $fn_name() -> &'static ModuleAddress {
            extern "C" {
                static _ImageBase: ();
            }
            (&_ImageBase) as *const () as *const core::ffi::c_void
        }
    }
}
 */

pub struct StaticStringResource {
    id: u32,
    value: OnceLock<Cow<'static, str>>,
    fallback: &'static str,
}

impl StaticStringResource {
    pub const fn new(id: u32, fallback: &'static str) -> Self {
        Self {
            id,
            value: OnceLock::new(),
            fallback,
        }
    }

    pub fn get(&self) -> &str {
        self.value.get_or_init(|| {
            let image_base = module_base();

            // If this returns 0, then the string was not found.
            let mut base: *const u16 = null_mut();
            let len = unsafe { LoadStringW(image_base, self.id, &mut base as *mut _ as *mut _, 0) };
            if len <= 0 {
                return Cow::Borrowed(self.fallback);
            }

            Cow::Owned(String::from_utf16_lossy(unsafe {
                from_raw_parts(base, len as usize)
            }))
        })
    }
}

impl Deref for StaticStringResource {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}
