use crate::helpers::join_args;
use crate::messages::ElevateRequest;
use std::env;
use std::ffi::CString;
use std::mem::size_of_val;
use std::ops::{Deref, DerefMut};
use std::ptr::addr_of;
use windows::core::*;
use windows::Win32::System::Diagnostics::Etw::*;

// These come from cpp/logging/EventViewerLogging.c
extern "C" {
    static PROVIDER_GUID: GUID;
    static SudoRequestRunEvent: EVENT_DESCRIPTOR;
    static SudoRecieveRunRequestEvent: EVENT_DESCRIPTOR;
}

#[repr(transparent)]
#[derive(Default)]
struct OwnedReghandle(pub u64);

impl Drop for OwnedReghandle {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe {
                EventUnregister(self.0);
            }
        }
    }
}

impl Deref for OwnedReghandle {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for OwnedReghandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

fn str_to_cstr_vec<T: Into<Vec<u8>>>(s: T) -> Vec<u8> {
    CString::new(s)
        .expect("strings should not have nulls")
        .into_bytes_with_nul()
}

fn create_descriptor<T, U>(ptr: *const T, len: U) -> EVENT_DATA_DESCRIPTOR
where
    U: TryInto<u32>,
    <U as TryInto<u32>>::Error: std::fmt::Debug,
{
    EVENT_DATA_DESCRIPTOR {
        Ptr: ptr as _,
        Size: len.try_into().unwrap(),
        Anonymous: Default::default(),
    }
}

/// Writes this request to the Windows Event Log. We do this for admins to be
/// able to audit who's calling what with sudo.
/// We log our messages to "Applications and Services Logs" -> "Microsoft" ->
/// "Windows" -> "Sudo" -> "Admin".
///
/// Alternatively, you can view this log with
/// `wevtutil qe Microsoft-Windows-Sudo/Admin /c:3 /rd:true /f:text`
pub fn event_log_request(is_client: bool, req: &ElevateRequest) {
    let mut registration_handle = OwnedReghandle::default();
    // The error code returned by EventRegister is primarily intended for use in debugging and diagnostic scenarios.
    // Most production code should continue to run even if an ETW provider failed to register,
    // so release builds should usually ignore the error code returned by EventRegister.
    unsafe { EventRegister(&PROVIDER_GUID, None, None, &mut *registration_handle) };

    let application = str_to_cstr_vec(req.application.as_str());
    let args_len = req.args.len() as u32;
    let args: Vec<_> = req
        .args
        .iter()
        .map(|arg| str_to_cstr_vec(arg.as_str()))
        .collect();
    let cwd = str_to_cstr_vec(req.target_dir.as_str());
    let mode = req.sudo_mode as u32;
    let inherit_env = !req.env_vars.is_empty();
    let redirected = req.handles.iter().any(|h| !h.is_invalid());
    let commandline = str_to_cstr_vec(format!(
        "{} {} {}",
        env::current_exe().unwrap().display(),
        req.application,
        join_args(&req.args)
    ));
    let request_id = req.event_id;

    let mut descriptors = Vec::with_capacity(9 + args.len());
    // <data name="Application" inType="win:AnsiString" outType="win:Utf8" />
    descriptors.push(create_descriptor(application.as_ptr(), application.len()));
    // <data name="ArgsCount" inType="win:UInt32" />
    descriptors.push(create_descriptor(
        addr_of!(args_len),
        size_of_val(&args_len),
    ));
    // <data name="Argument" inType="win:AnsiString" outType="win:Utf8" count="ArgsCount" />
    for arg in &args {
        descriptors.push(EVENT_DATA_DESCRIPTOR {
            Ptr: arg.as_ptr() as _,
            Size: arg.len() as u32,
            Anonymous: Default::default(),
        });
    }
    // <data name="CurrentWorkingDirectory" inType="win:AnsiString" outType="win:Utf8" />
    descriptors.push(create_descriptor(cwd.as_ptr(), cwd.len()));
    // <data name="Mode" inType="win:UInt32" />
    descriptors.push(create_descriptor(addr_of!(mode), size_of_val(&mode)));
    // <data name="InheritEnvironment" inType="win:UInt8" outType="win:Boolean" />
    descriptors.push(create_descriptor(
        addr_of!(inherit_env),
        size_of_val(&inherit_env),
    ));
    // <data name="Redirected" inType="win:UInt8" outType="win:Boolean" />
    descriptors.push(create_descriptor(
        addr_of!(redirected),
        size_of_val(&redirected),
    ));
    // <data name="FullCommandline" inType="win:AnsiString" outType="win:Utf8" />
    descriptors.push(create_descriptor(commandline.as_ptr(), commandline.len()));
    // <data name="RequestID" inType="win:GUID"/>
    descriptors.push(create_descriptor(
        addr_of!(request_id),
        size_of_val(&request_id),
    ));

    unsafe {
        // We're literally using the same data template for both requests and
        // responses. The only difference is the event ID's have different keywords
        // (to ID who sent the event).
        let event_id = if is_client {
            &SudoRequestRunEvent
        } else {
            &SudoRecieveRunRequestEvent
        };
        EventWrite(*registration_handle, event_id, Some(&descriptors));
    }
}
