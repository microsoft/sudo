use crate::messages::*;
use std::ffi::OsStr;
use sudo_events::SudoEvents;

// tl:{6ffdd42d-46d9-5efe-68a1-3b18cb73a607}
static SUDO_EVENTS: std::sync::OnceLock<SudoEvents> = std::sync::OnceLock::new();

const PDT_PRODUCT_AND_SERVICE_PERFORMANCE: u64 = 0x0000000001000000;
// const PDT_PRODUCT_AND_SERVICE_USAGE: u64 = 0x0000000002000000;

pub fn sudo_events() -> &'static SudoEvents {
    SUDO_EVENTS.get_or_init(SudoEvents::new)
}

pub fn trace_log_message(message: &str) {
    let events = sudo_events();
    events.message(None, message);
}

pub fn trace_command_not_found(exe_name: &OsStr) {
    let events = sudo_events();
    if events.command_not_found_is_enabled(None) {
        events.command_not_found(None, &exe_name.to_string_lossy());
    }
}

pub fn trace_cmd_builtin_found(exe_name: &OsStr) {
    let events = sudo_events();
    if events.cmd_builtin_found_is_enabled(None) {
        events.cmd_builtin_found(None, &exe_name.to_string_lossy());
    }
}

pub fn trace_run(req: &ElevateRequest, redirected_input: bool, redirected_output: bool) {
    let events = sudo_events();
    if events.run_is_enabled(None) {
        events.run(
            None,
            &req.application.to_string_lossy(),
            req.sudo_mode as u32,
            req.parent_pid,
            redirected_input,
            redirected_output,
        );
    }
}

pub fn trace_modes(requested_mode: u32, allowed_mode: u32, policy_mode: u32) {
    let events = sudo_events();
    // We manually set the privacy tag to PDT_PRODUCT_AND_SERVICE_PERFORMANCE so
    // that callers don't need to know that
    events.modes(
        None,
        requested_mode,
        allowed_mode,
        policy_mode,
        PDT_PRODUCT_AND_SERVICE_PERFORMANCE,
    );
}
