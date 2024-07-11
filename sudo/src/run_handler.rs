use crate::elevate_handler::spawn_target_for_request;
use crate::helpers::*;
use crate::logging_bindings::event_log_request;
use crate::messages::ElevateRequest;
use crate::rpc_bindings::Utf8Str;
use crate::rpc_bindings_client::{
    rpc_client_cleanup, rpc_client_do_elevation_request, rpc_client_setup,
};
use crate::{r, tracing};
use std::env;
use std::ffi::{CString, OsStr};
use std::path::Path;
use windows::Wdk::Foundation::{NtQueryObject, ObjectBasicInformation};
use windows::Win32::System::WindowsProgramming::PUBLIC_OBJECT_BASIC_INFORMATION;
use windows::{
    core::*, Wdk::System::Threading::*, Win32::Foundation::*, Win32::Storage::FileSystem::*,
    Win32::System::Console::*, Win32::System::Diagnostics::Debug::*, Win32::System::Rpc::*,
    Win32::System::SystemInformation::*, Win32::System::Threading::*, Win32::UI::Shell::*,
    Win32::UI::WindowsAndMessaging::*,
};

fn current_elevation_matches_request(is_admin: bool, _req: &ElevateRequest) -> bool {
    // FUTURE TODO: actually support running as another user.
    is_admin
}

/// helper to find the process creation time for a given process handle
/// process_handle: handle to the process to get the creation time for. This is a non-owning handle.
fn get_process_creation_time(process_handle: HANDLE) -> Result<FILETIME> {
    unsafe {
        // You actually have to pass in valid pointers to these, even if we don't need them.
        let mut creation_time = FILETIME::default();
        let mut exit_time = FILETIME::default();
        let mut kernel_time = FILETIME::default();
        let mut user_time = FILETIME::default();
        GetProcessTimes(
            process_handle,
            &mut creation_time,
            &mut exit_time,
            &mut kernel_time,
            &mut user_time,
        )?;
        Ok(creation_time)
    }
}

fn is_in_windows_dir(path: &Path) -> bool {
    let path = HSTRING::from(path);
    if path.len() >= MAX_PATH as usize {
        return false;
    }
    let mut win_dir = [0u16; MAX_PATH as usize];
    let len = unsafe { GetWindowsDirectoryW(Some(&mut win_dir)) };
    if len == 0 || len >= MAX_PATH {
        return false;
    }
    unsafe { PathIsPrefixW(PCWSTR(win_dir.as_ptr()), PCWSTR(path.as_ptr())).as_bool() }
}

/// Attempts to modify this request to run the command in CMD, if the
/// "application" that was passed to us was really just a CMD intrinsic. This is
/// used to support things like `sudo dir.
///
/// We don't do any modification if the parent process was some variety of
/// PowerShell. There's impossible to resolve issues repackaging the args back
/// into a PowerShell command, so we're hoping that the sudo.ps1 script will
/// handle that case instead.
///
/// * Returns an error if we failed to get the parent pid, or otherwise lookup
///   info we needed.
/// * Returns true if the application was a CMD intrinsic AND we were spawned
///   from CMD, and we adjusted the args accordingly.
/// * Returns false if the application was not an intrinsic or cmdlet
fn adjust_args_for_intrinsics_and_cmdlets(req: &mut ElevateRequest) -> Result<bool> {
    // First things first: Get our parent process PID, with NtQueryInformationProcess
    let parent_pid = unsafe {
        let mut process_info = PROCESS_BASIC_INFORMATION::default();
        let mut return_len = 0u32;

        let get_parent_pid = NtQueryInformationProcess(
            GetCurrentProcess(),
            ProcessBasicInformation,
            &mut process_info as *mut _ as _,
            std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            &mut return_len,
        )
        .ok();
        if let Err(err) = get_parent_pid {
            tracing::trace_log_message(&format!("Error getting parent pid: {:?}", err.code().0));
            return Err(err);
        }
        process_info.InheritedFromUniqueProcessId
    };
    tracing::trace_log_message(&format!("parent_pid: {parent_pid:?}"));
    // Now, open that process so we can query some more information about it.
    // (stick it in an `Owned<T>` so it gets closed when we're done with it)
    let parent_process_handle = unsafe {
        Owned::new(OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            parent_pid.try_into().unwrap(),
        )?)
    };

    // Sanity check time!
    // Was the parent process started _before us_?

    // Compare the two. If the parent process was created _after_ us, then we want to bail (with Ok(false))
    unsafe {
        let parent_process_creation_time = get_process_creation_time(*parent_process_handle)?;
        let our_creation_time = get_process_creation_time(GetCurrentProcess())?;
        if CompareFileTime(&parent_process_creation_time, &our_creation_time) == 1 {
            // Parent process was created after us. Bail.
            return Ok(false);
        }
    }

    // Now, get the full path to the parent process
    let parent_process_path =
        get_process_path_from_handle(*parent_process_handle).unwrap_or_default();

    tracing::trace_log_message(&format!("parent_process_str: {:?}", parent_process_path));

    if !parent_process_path.ends_with("cmd.exe") {
        // It's not. Bail.
        return Ok(false);
    }

    // We're using the Windows dir here, because we might be a x64 sudo
    // that's being run from a x86 cmd.exe (which is _actually_ in syswow64).
    if !is_in_windows_dir(&parent_process_path) {
        // It's not. Bail.
        return Ok(false);
    }

    // Here, our parent is in fact cmd.exe (any arch).

    if is_cmd_intrinsic(&req.application) {
        tracing::trace_cmd_builtin_found(&req.application);

        req.args
            .splice(0..0, ["/c".to_string(), req.application.clone()]);

        // Toss this back at _exactly our parent process_. This makes sure we
        // don't try to invoke the x64 cmd.exe from a x86 cmd.exe
        req.application = parent_process_path.to_string_lossy().to_string();
        return Ok(true);
    }

    Ok(false)
}

fn adjust_args_for_gui_exes(req: &mut ElevateRequest) {
    // We did find the command. We're now gonna try to find out if the file
    // is:
    // - An command line exe
    // - A GUI exe
    // - Just a plain old file (not an exe)
    //
    // Depending on what it is, we'll need to modify our request to run it.
    // A Windows GUI exe can just be shell executed directly.
    // TODO: We may want to do other work in the future for plain, non-executable files.

    let (is_exe, is_gui) = match get_exe_subsystem(&req.application) {
        Ok(subsystem) => (true, subsystem == IMAGE_SUBSYSTEM_WINDOWS_GUI),
        Err(..) => (false, false),
    };

    tracing::trace_log_message(&format!("is_exe: {is_exe}"));
    tracing::trace_log_message(&format!("is_gui: {is_gui}"));

    // TODO: figure out how to handle non-exe files. ShellExecute(runas,
    // ...) doesn't do anything for them, and I'm not sure we can trivially
    // have the service find out what the right verb is for an arbitrary
    // extension. (this is the kind of comment I'm sure to be proven wrong
    // about)
    if is_gui {
        tracing::trace_log_message("not cli exe. Force new window");
        req.sudo_mode = SudoMode::ForceNewWindow;
    }
}

pub fn run_target(
    copy_env: bool,
    args: &[&String],
    sudo_mode: SudoMode,
    requested_dir: Option<String>,
) -> Result<i32> {
    let manually_requested_dir = requested_dir.is_some();
    let req = prepare_request(copy_env, args, sudo_mode, requested_dir)?;
    do_request(req, copy_env, manually_requested_dir)
}

/// Constructs an ElevateRequest from the given arguments. We'll package up
/// handles, we'll separate out the application and args, and we'll do some
/// other work to make sure the request is ready to go.
///
/// This also includes getting the absolute path to the requested application
/// (which involves hitting up the file system). If the target app is a GUI app,
/// we'll convert the request to run in a new window.
///
/// If the app isn't actually an app, and it's instead a CMD intrinsic, we'll
/// convert the request to run in CMD (if we were _ourselves_ ran from CMD).
fn prepare_request(
    copy_env: bool,
    args: &[&String],
    sudo_mode: SudoMode,
    requested_dir: Option<String>,
) -> Result<ElevateRequest> {
    let handle_indices = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];

    // Get our stdin and stdout handles
    let handles = handle_indices.map(|idx| unsafe { GetStdHandle(idx).ok().unwrap_or_default() });

    // Is stdin or stdout a console?
    let is_console = handles.map(|h| unsafe {
        let mut mode = CONSOLE_MODE::default();
        GetConsoleMode(h, &mut mode).is_ok()
    });

    // Pass invalid handles if the handle is a console handle. If you don't,
    // then RPC will explode trying to duplicate the console handle to the
    // elevated process (because a console isn't a "pipe", but a file is)
    let mut filtered_handles: [HANDLE; 3] = Default::default();
    for i in 0..3 {
        if !is_console[i] {
            filtered_handles[i] = handles[i];
        }
    }

    // If they passed a directory, use that. Otherwise, "use the current dir"
    // (with the known caveats about new window mode)
    let actual_dir = match requested_dir {
        Some(dir) => {
            // If they passed a directory, we need to canonicalize it. This is
            // because the elevated sudo will start in system32 (because we are
            // ourselves, an exe that's in the Windows directory). This will
            // make sure the elevated sudo gets the real path they asked for.
            //
            // DON'T use std::canonicalize though. That'll give us a UNC path
            // and just about nothing actaully accepts those (CMD.EXE included)
            absolute_path(Path::new(&dir))?
        }
        None => env::current_dir()?,
    }
    .to_string_lossy()
    .into_owned();

    // Build our request
    let mut req = ElevateRequest {
        parent_pid: std::process::id(),
        handles: filtered_handles,
        sudo_mode,
        application: args[0].clone(),
        args: args.iter().skip(1).map(|arg| arg.to_string()).collect(),
        target_dir: actual_dir,
        env_vars: copy_env.then(env_as_string).unwrap_or_default(),
        event_id: GUID::new().unwrap(),
    };

    tracing::trace_run(&req, !is_console[0], !is_console[1]);
    event_log_request(true, &req);

    // Does the application exist somewhere on the path?
    let where_result = which::which(&req.application);

    if let Ok(path) = where_result {
        // It's a real file that exists on the PATH.

        // Replace the request's application with the full path to the exe. This
        // ensures that the elevated sudo will execute the same thing that was
        // found here in the unelevated context.

        req.application = absolute_path(&path)?.to_string_lossy().to_string();
        adjust_args_for_gui_exes(&mut req);
    } else {
        tracing::trace_command_not_found(&req.application);

        // Maybe, it's a CMD intrinsic. If it is, we'll need to adjust the args
        // to make a new CMD to run the command
        //
        // This will return
        // * an error if we couldn't get at our parent PID (very unexpected) or
        //   open the parent handle
        // * Ok(false):
        //   - if the parent was created after us, so we can't tell if it's CMD or not
        //   - if the parent wasn't CMD or it wasn't an intrinsic
        // * Ok(true): The parent was CMD, it was an intrinsic, and the args
        //   were adjusted to account for this.
        if !adjust_args_for_intrinsics_and_cmdlets(&mut req)? {
            return Err(E_DIR_BAD_COMMAND_OR_FILE.into());
        }
    }
    Ok(req)
}

fn do_request(req: ElevateRequest, copy_env: bool, manually_requested_dir: bool) -> Result<i32> {
    // Are we already running as admin? If we are, we don't need to do a whole
    // bunch of ShellExecute. We can just spawn the target exe.]
    let is_admin = is_running_elevated()?;

    if current_elevation_matches_request(is_admin, &req) {
        // println!("We're already running as admin. Just run the command.");
        let mut child = spawn_target_for_request(&req)?;
        match child.wait() {
            Ok(status) => Ok(status.code().unwrap_or_default()),
            Err(err) => Err(err.into()),
        }
    } else {
        // We're not running elevated here. We need to start the
        // elevated sudo and send it our request to handle.

        // In ForceNewWindow mode, we want to use ShellExecuteEx to create the
        // target process, whenever possible. This has the benefit of having the
        // UAC display the target app directly, and also avoiding any RPC calls
        // at all.
        //
        // However, there are caveats which prevent us from using ShellExecuteEx
        // in all cases:
        // * We can't use ShellExecuteEx if we need to copy the environment,
        //   because ShellExecuteEx doesn't allow us to set the environment of
        //   the target process. So if they want environment variables copied,
        //   we need to use RPC.
        // * ShellExecuteEx will always set the CWD to system32, if the target
        //   exe is in the Windows dir. It does this _deep_ in the OS and
        //   there's nothing we can do to avoid it. So, if the user has
        //   requested a CWD, we need to use RPC.
        //    - Theoretically, we only need to use RPC if the target app is in
        //      the Windows dir, but we'd need to recreate the internal logic of
        //      CreateProcess to resolve the commandline we've been given here
        //      to determine that.
        let should_use_runas =
            req.sudo_mode == SudoMode::ForceNewWindow && !copy_env && !manually_requested_dir;

        if should_use_runas {
            tracing::trace_log_message("Direct ShellExecute");
            runas_admin(&req.application, &join_args(&req.args), SW_NORMAL)?;
            Ok(0)
        } else {
            tracing::trace_log_message("starting RPC handoff");
            handoff_to_elevated(&req)
        }
    }
}

/// Generates a random nonce to include in the RPC endpoint name. We're using
/// `RtlGenRandom` to generate the number. This is how the core language does it:
/// https://github.com/rust-lang/rust/pull/45370
fn random_nonce() -> u32 {
    #[link(name = "advapi32")]
    extern "system" {
        // This function's real name is `RtlGenRandom`.
        fn SystemFunction036(RandomBuffer: *mut u8, RandomBufferLength: u32) -> BOOLEAN;
    }

    let mut nonce = 0u32;
    unsafe {
        SystemFunction036(
            (&mut nonce as *mut u32) as *mut u8,
            std::mem::size_of::<u32>() as _,
        );
    }
    nonce
}

fn handoff_to_elevated(req: &ElevateRequest) -> Result<i32> {
    // Build a single string from the request's application and args
    let parent_pid = std::process::id();

    // generate a pseudorandom nonce to include
    let nonce = random_nonce();

    tracing::trace_log_message(&format!(
        "running as user: '{}'",
        get_current_user().as_ref().unwrap_or(h!("unknown"))
    ));

    let path = env::current_exe().unwrap();
    let target_args = format!(
        "elevate -p {parent_pid} -n {nonce} {} {}",
        req.application,
        join_args(&req.args)
    );
    tracing::trace_log_message(&format!("elevate request: '{target_args:?}'"));
    runas_admin(&path, &target_args, SW_HIDE)?;

    // Subtle: Add our own CtrlC handler, so that we can ignore it.
    // Otherwise, the console gets into a weird state, where we return
    // control to the parent shell, but the child process is still
    // running. Two clients in the same console is always weird.
    unsafe {
        _ = SetConsoleCtrlHandler(Some(ignore_ctrl_c), true);
    }

    send_request_via_rpc(req, nonce)
}

/// Connects to the elevated sudo instance via RPC, then makes a couple RPC
/// calls to send the request to the elevated sudo.
///
/// In the case of success, this might not return until our target process
/// actually exits.
///
/// This will return an error if we can't connect to the RPC server. However,
/// we'll return Ok regardless if the RPC call itself succeeded or not. The Ok()
/// value will be:
/// - 0 if the _target_ process exited successfully
/// - Anything else to indicate either an error in the RPC call, or the target
///   process exited with an error.
///   - Specifically be on the lookout for 1764 here, which is
///     RPC_S_CANNOT_SUPPORT
fn send_request_via_rpc(req: &ElevateRequest, nonce: u32) -> Result<i32> {
    let endpoint = generate_rpc_endpoint_name(unsafe { GetCurrentProcessId() }, nonce);
    let endpoint = CString::new(endpoint).unwrap();

    // Attempt to connect to our RPC server, with a backoff. This will try 10
    // times, backing off by 100ms each time (a total of 5 seconds)

    let mut tries = 0;
    loop {
        if tries > 10 {
            return Err(ERROR_TIMEOUT.into());
        }
        // Casting this name to a *const u8 is a little unsafe, but our
        // endpoint names aren't gonna be running abreast of weird encoding
        // edge cases, and RpcStringBindingCompose ultimately wants an
        // unsigned char string.
        let connect_result = rpc_client_setup(&endpoint);

        match connect_result {
            RPC_STATUS(0) => break,
            RPC_S_NOT_LISTENING => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(100 * tries))
            }
            _ => std::process::exit(connect_result.0),
        }
    }

    // The GetCurrentProcess() is not a "real" handle and unsuitable to be used with COM.
    // -> We need to clone it first.
    let h_real = unsafe {
        let mut process = Owned::default();
        let current_process = GetCurrentProcess();
        DuplicateHandle(
            current_process,
            current_process,
            current_process,
            &mut *process,
            0,
            true,
            DUPLICATE_SAME_ACCESS,
        )?;
        process
    };

    tracing::trace_log_message(&format!("sending i/o/e handles: {:?}", req.handles));

    let mut child_handle = Owned::default();
    let rpc_elevate = rpc_client_do_elevation_request(
        *h_real,
        &req.handles,
        req.sudo_mode,
        Utf8Str::new(&req.application),
        Utf8Str::new(&pack_string_list_for_rpc(&req.args)),
        Utf8Str::new(&req.target_dir),
        Utf8Str::new(&req.env_vars),
        req.event_id,
        &mut *child_handle,
    );

    tracing::trace_log_message(&format!("RequestElevation result {rpc_elevate:?}"));
    // Clean up (terminate) the RPC server we made.
    rpc_client_cleanup();

    rpc_elevate.ok()?;

    // Assert that handle_elevation_request() properly limited the handle access rights to just the bits that we needed.
    if cfg!(debug_assertions) {
        unsafe {
            let mut info: PUBLIC_OBJECT_BASIC_INFORMATION = Default::default();
            NtQueryObject(
                *child_handle,
                ObjectBasicInformation,
                Some(&mut info as *mut _ as _),
                std::mem::size_of_val(&info) as _,
                None,
            )
            .ok()?;

            let expected =
                PROCESS_DUP_HANDLE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE;
            debug_assert!(info.GrantedAccess == expected.0);
        }
    }

    // If we were in new window mode, and we're here, then we're
    // ShellExecuting sudo.exe, and then using the elevated sudo to create a
    // new console window. In that case, we want to print an error message
    // here - the elevated sudo will have exited as soon as the child is
    // launched.
    if req.sudo_mode == SudoMode::ForceNewWindow {
        let translated_msg = r::IDS_LAUNCHEDNEWWINDOW.get();
        let replaced = translated_msg.replace("{0}", &req.application);
        println!("{}", replaced);
        Ok(0)
    } else {
        unsafe {
            let mut status = 0u32;
            _ = WaitForSingleObject(*child_handle, INFINITE);
            GetExitCodeProcess(*child_handle, &mut status)?;
            Ok(status as _)
        }
    }
}

fn runas_admin<Exe, Args>(exe: &Exe, args: &Args, show: SHOW_WINDOW_CMD) -> Result<()>
where
    Exe: AsRef<OsStr> + ?Sized,
    Args: AsRef<OsStr> + ?Sized,
{
    runas_admin_impl(exe.as_ref(), args.as_ref(), show)
}

fn runas_admin_impl(exe: &OsStr, args: &OsStr, show: SHOW_WINDOW_CMD) -> Result<()> {
    let cwd = env::current_dir()?;
    let h_exe = HSTRING::from(exe);
    let h_commandline = HSTRING::from(args);
    let h_cwd = HSTRING::from(cwd.as_os_str());
    let mut sei = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(h_exe.as_ptr()),
        lpParameters: PCWSTR(h_commandline.as_ptr()),
        lpDirectory: PCWSTR(h_cwd.as_ptr()),
        nShow: show.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut sei) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmd_is_cui() {
        let app_name = "cmd".to_string();
        let req = prepare_request(false, &[&app_name], SudoMode::Normal, None).unwrap();
        assert_eq!(req.sudo_mode, SudoMode::Normal);
    }
    #[test]
    fn test_notepad_is_gui() {
        let req =
            prepare_request(false, &[&("notepad".to_string())], SudoMode::Normal, None).unwrap();
        // If we did in fact find notepad, then we should have set the mode to
        // ForceNewWindow, since it's a GUI app.
        assert_eq!(req.sudo_mode, SudoMode::ForceNewWindow);

        // I found in the past that `notepad.exe` worked, while `notepad`
        // didn't. Just make sure they both do, for sanity's sake.
        let req_exe = prepare_request(
            false,
            &[&("notepad.exe".to_string())],
            SudoMode::Normal,
            None,
        )
        .unwrap();
        assert_eq!(req_exe.sudo_mode, SudoMode::ForceNewWindow);
    }
}
