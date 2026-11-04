use std::ffi::OsString;
use std::marker::PhantomData;
use std::slice::from_raw_parts;
use windows::{core::*, Win32::Foundation::*};

/// "Any WTF-8 data must be converted to a Unicode encoding at the system’s boundary before being emitted."
/// -- https://wtf-8.codeberg.page/#intended-audience
///
/// But I can't read, so this struct allows us to transmit WTF-8 over RPC.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Wtf8Str<'a> {
    length: u32,
    data: *const u8,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> Wtf8Str<'a> {
    pub fn new(s: &[u8]) -> Wtf8Str<'a> {
        Wtf8Str {
            length: s.len().try_into().unwrap(),
            data: s.as_ptr(),
            _marker: PhantomData,
        }
    }

    pub fn as_slice(&self) -> Result<&'a [u8]> {
        if self.length == 0 {
            Ok(&[])
        } else {
            // SAFETY: `.data` is non-null and `.length` cannot be
            // truncated as we do not target <32 bit architectures.
            Ok(unsafe { from_raw_parts(self.data, self.length as usize) })
        }
    }

    pub fn to_os_string(self) -> Result<OsString> {
        self.as_slice().and_then(validate_wtf8)
    }
}

impl From<&str> for Wtf8Str<'_> {
    fn from(s: &str) -> Self {
        Wtf8Str::new(s.as_bytes())
    }
}

fn validate_wtf8(bytes: &[u8]) -> Result<OsString> {
    let mut remaining = bytes;
    let mut previous_high_surrogate = false;

    loop {
        match std::str::from_utf8(remaining) {
            Ok(_) => {
                // If we got here, it means we got a valid WTF-8 string,
                // as otherwise we would've returned with an Err() below.
                let bytes = bytes.to_vec();

                // Internally, `OsString` stores whether its input was valid UTF-8 or not.
                // Because of this, we have two paths: One for UTF-8 and one for WTF-8.
                let str = if remaining.len() == bytes.len() {
                    // SAFETY: If `remaining.len() == bytes.len()` then the entire input was valid UTF-8.
                    OsString::from(unsafe { String::from_utf8_unchecked(bytes) })
                } else {
                    // SAFETY: As per the above, we have validated `bytes`.
                    unsafe { OsString::from_encoded_bytes_unchecked(bytes) }
                };

                return Ok(str);
            }
            Err(err) => {
                remaining = &remaining[err.valid_up_to()..];

                // The only UTF-8 errors WTF-8 permits are complete surrogate encodings.
                let [0xed, second @ 0xa0..=0xbf, 0x80..=0xbf, rest @ ..] = remaining else {
                    return Err(ERROR_NO_UNICODE_TRANSLATION.into());
                };

                // Adjacent high/low surrogates must instead encode one scalar in four bytes.
                if err.valid_up_to() == 0 && previous_high_surrogate && *second >= 0xb0 {
                    return Err(ERROR_NO_UNICODE_TRANSLATION.into());
                }

                previous_high_surrogate = *second < 0xb0;
                remaining = rest;
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::validate_wtf8;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    #[test]
    fn os_string_uses_wtf8() {
        let cases: &[(&[u16], &[u8])] = &[
            (&[], b""),
            (&[0x0061, 0x00e9], b"a\xc3\xa9"),
            (&[0xd800], b"\xed\xa0\x80"),
            (&[0xdfff], b"\xed\xbf\xbf"),
            (&[0xd800, 0xdc00], b"\xf0\x90\x80\x80"),
            (&[0xd800, 0x0061, 0xdc00], b"\xed\xa0\x80a\xed\xb0\x80"),
        ];

        for &(wide, bytes) in cases {
            let expected = OsString::from_wide(wide);
            // Check the encoding before invoking the validator's unsafe conversion.
            assert_eq!(expected.as_encoded_bytes(), bytes, "{wide:04x?}");
            assert_eq!(validate_wtf8(bytes).unwrap(), expected, "{wide:04x?}");
        }
    }
}
