use crate::helpers::SudoMode;
use windows::{core::GUID, Win32::Foundation::HANDLE};

pub struct ElevateRequest {
    pub parent_pid: u32,
    pub handles: [HANDLE; 3], // in, out, err
    pub sudo_mode: SudoMode,
    pub application: String,
    pub args: Vec<String>,
    pub target_dir: String,
    pub env_vars: String,
    pub event_id: GUID,
}
