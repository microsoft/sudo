use sudo_events::SudoEvents;

// tl:{6ffdd42d-46d9-5efe-68a1-3b18cb73a607}
static SUDO_EVENTS: std::sync::OnceLock<SudoEvents> = std::sync::OnceLock::new();

const PDT_PRODUCT_AND_SERVICE_PERFORMANCE: u64 = 0x0000000001000000;
// const PDT_PRODUCT_AND_SERVICE_USAGE: u64 = 0x0000000002000000;

pub fn sudo_events() -> &'static SudoEvents {
    SUDO_EVENTS.get_or_init(SudoEvents::new)
}

use crate::messages::*;

pub fn enable_tracing() {
    sudo_events();
}

pub fn trace_log_message(message: &str) {
    sudo_events().message(None, message);
}

pub fn trace_command_not_found(exe_name: &str) {
    sudo_events().command_not_found(None, exe_name);
}

pub fn trace_cmd_builtin_found(exe_name: &str) {
    sudo_events().cmd_builtin_found(None, exe_name);
}

pub fn trace_run(req: &ElevateRequest, redirected_input: bool, redirected_output: bool) {
    sudo_events().run(
        None,
        &req.application,
        req.sudo_mode as u32,
        req.parent_pid,
        redirected_input,
        redirected_output,
    );
}

pub fn trace_modes(requested_mode: u32, allowed_mode: u32, policy_mode: u32) {
    // We manually set the privacy tag to PDT_PRODUCT_AND_SERVICE_PERFORMANCE so
    // that callers don't need to know that
    sudo_events().modes(
        None,
        requested_mode,
        allowed_mode,
        policy_mode,
        PDT_PRODUCT_AND_SERVICE_PERFORMANCE,
    );
}
