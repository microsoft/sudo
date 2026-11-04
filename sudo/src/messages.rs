use crate::helpers::SudoMode;
use std::ffi::OsString;
use windows::{core::GUID, Win32::Foundation::HANDLE};

pub struct ElevateRequest {
    pub parent_pid: u32,
    pub handles: [HANDLE; 3], // in, out, err
    pub sudo_mode: SudoMode,
    pub application: OsString, // TODO: With some effort, these could be &OsStr
    pub args: Vec<OsString>,
    pub target_dir: OsString,
    pub env_vars: OsString,
    pub event_id: GUID,
}
