use crate::helpers::*;
use crate::logging_bindings::event_log_request;
use crate::messages::ElevateRequest;
use crate::rpc_bindings_server::rpc_server_setup;
use crate::tracing;
use std::ffi::CString;
use std::os::windows::io::{FromRawHandle, IntoRawHandle};
use std::os::windows::process::CommandExt;
use std::process::Stdio;
use windows::{
    core::*, Win32::Foundation::*, Win32::System::Console::*, Win32::System::Threading::*,
};

fn handle_to_stdio(h: HANDLE) -> Stdio {
    if h.is_invalid() {
        return Stdio::inherit();
    }

    unsafe {
        let p = GetCurrentProcess();
        let mut clone = Default::default();
        match DuplicateHandle(p, h, p, &mut clone, 0, true, DUPLICATE_SAME_ACCESS) {
            Ok(_) => Stdio::from_raw_handle(clone.0 as _),
            Err(_) => Stdio::null(),
        }
    }
}

/// Prepare the target process, spawn it, and hand back the Child process. This will take care of setting up the handles for redirected input/output, and setting the environment variables.
pub fn spawn_target_for_request(request: &ElevateRequest) -> Result<std::process::Child> {
    tracing::trace_log_message(&format!("Spawning: {}...", &request.application));

    let mut command_args = std::process::Command::new(request.application.clone());

    command_args.current_dir(request.target_dir.clone());
    command_args.args(request.args.clone());

    tracing::trace_log_message(&format!("args: {:?}", &request.args));

    if !request.env_vars.is_empty() {
        command_args.env_clear();
        command_args.envs(env_from_raw_bytes(&request.env_vars));
    }

    // If we're in ForceNewWindow mode, we want the target process to use a new
    // console window instead of inheriting the one from the parent process.
    if request.sudo_mode == SudoMode::ForceNewWindow {
        command_args.creation_flags(CREATE_NEW_CONSOLE.0);
    }

    // Set the stdin/stdout/stderr of the child process In disabled input
    // mode, set stdin to null. We don't want the target application to be
    // able to read anything from stdin.
    command_args.stdin(if request.sudo_mode != SudoMode::DisableInput {
        handle_to_stdio(request.handles[0])
    } else {
        Stdio::null()
    });
    command_args.stdout(handle_to_stdio(request.handles[1]));
    command_args.stderr(handle_to_stdio(request.handles[2]));

    command_args.spawn().map_err(|err| {
        match err.kind() {
            std::io::ErrorKind::NotFound => {
                // This error code is MSG_DIR_BAD_COMMAND_OR_FILE. That's
                // what CMD uses to indicate a command not found.
                E_DIR_BAD_COMMAND_OR_FILE.into()
            }
            _ => err.into(),
        }
    })
}

/// Execute the elevation request.
/// * Conditionally attach to the parent process's console (if requested)
/// * Spawn the target process (with redirected input/output if requested, and with the environment variables passed in if needed)
/// Called by rust_handle_elevation_request
pub fn handle_elevation_request(request: &ElevateRequest) -> Result<Owned<HANDLE>> {
    // Log the request we received to the event log. This should create a pair
    // of events, one for the request, and one for the response, each with the
    // same RequestID.
    event_log_request(false, request);

    // Check if the requested sudo mode is allowed
    let config: RegistryConfigProvider = Default::default();
    let allowed_mode = get_allowed_mode(&config)?;
    if request.sudo_mode > allowed_mode {
        tracing::trace_log_message(&format!(
            "Requested sudo mode is not allowed: {:?} ({:?})",
            request.sudo_mode, allowed_mode
        ));
        return Err(E_ACCESSDENIED.into());
    }

    // If we're in ForceNewWindow mode, we _don't_ want to detach from our
    // current console and reattach to the parent process's console. Instead,
    // we'll just create the target process with CREATE_NEW_CONSOLE.
    //
    // This scenario only happens when we're running `sudo -E --newWindow`, to
    // copy env vars but also use a new console window. If we don't pass -E,
    // then we'll have instead just directly ShellExecute'd the target
    // application (and never hit this codepath)
    //
    // Almost all the time, we'll actually hit the body of this conditional.
    if request.sudo_mode != SudoMode::ForceNewWindow {
        // It would seem that we always need to detach from the current console,
        // even in redirected i/o mode. In the case that we aren't fully redirected
        // (like, if stdin is redirected but stdout isn't), we'll still need to
        // attach to the parent console for the other std handles.

        unsafe {
            // Detach from the current console
            _ = FreeConsole();

            // Attach to the parent process's console
            if { AttachConsole(request.parent_pid) }.is_ok() {
                // Add our own CtrlC handler, so that we can ignore it.
                _ = SetConsoleCtrlHandler(Some(ignore_ctrl_c), true);
                // TODO! add some error handling here you goober
            }
        }
    }

    // We're attached to the right console, Run the command.
    let process_launch = spawn_target_for_request(request);
    unsafe {
        _ = SetConsoleCtrlHandler(Some(ignore_ctrl_c), false);
        _ = FreeConsole();
    }

    let child = process_launch?;

    // Limit the things the caller can do with the process handle, because the one we just created is PROCESS_ALL_ACCESS.
    // I tried to use [out, system_handle(sh_process, PROCESS_QUERY_LIMITED_INFORMATION)]
    // in the COM API to have it limit the handle permissions but that didn't work at all.
    // So now we do it manually here.
    unsafe {
        let mut child_handle = Owned::default();
        let current_process = GetCurrentProcess();
        DuplicateHandle(
            current_process,
            HANDLE(child.into_raw_handle() as _),
            current_process,
            &mut *child_handle,
            (PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_DUP_HANDLE | PROCESS_SYNCHRONIZE).0,
            false,
            DUPLICATE_CLOSE_SOURCE,
        )?;
        Ok(child_handle)
    }
}

/// Starts the RPC server and blocks until Shutdown() is called.
pub fn start_rpc_server(
    parent_pid: u32,
    nonce: u32,
    _caller_sid: Option<&String>,
    _args: &[&String],
) -> Result<i32> {
    // TODO:48520593 In rust_handle_elevation_request, validate that the parent
    // process handle is the same one that we opened here.

    let endpoint = generate_rpc_endpoint_name(parent_pid, nonce);
    let endpoint = CString::new(endpoint).unwrap();
    rpc_server_setup(&endpoint, parent_pid)?;

    Ok(0)
}
