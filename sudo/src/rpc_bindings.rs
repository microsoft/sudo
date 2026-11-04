use std::cmp::min;
use std::marker::PhantomData;
use std::slice::from_raw_parts;
use std::str::from_utf8;
use windows::{core::*, Win32::Foundation::*};

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Utf8Str<'a> {
    length: u32,
    data: *const u8,
    _marker: PhantomData<&'a str>,
}

impl<'a> Utf8Str<'a> {
    pub fn new(s: &str) -> Utf8Str<'_> {
        Utf8Str {
            length: min(s.len(), u32::MAX as usize) as u32,
            data: s.as_ptr(),
            _marker: PhantomData,
        }
    }

    pub fn as_str(&self) -> Result<&str> {
        from_utf8(unsafe { from_raw_parts(self.data, self.length as usize) })
            .map_err(|_| ERROR_NO_UNICODE_TRANSLATION.to_hresult().into())
    }
}

impl<'a> From<&'a str> for Utf8Str<'a> {
    fn from(value: &'a str) -> Self {
        Utf8Str::new(value)
    }
}
