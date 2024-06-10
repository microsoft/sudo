use crate::rpc_bindings::Utf8Str;
use crate::trace_log_message;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::mem::{size_of, MaybeUninit};
use std::ops::{Deref, DerefMut};
use std::os::windows::ffi::OsStringExt;
use std::os::windows::fs::FileExt;
use std::path::{Path, PathBuf};
use std::slice::{from_raw_parts, from_raw_parts_mut};
use windows::Win32::Storage::FileSystem::GetFullPathNameW;
use windows::Win32::System::Diagnostics::Debug::{IMAGE_NT_HEADERS32, IMAGE_SUBSYSTEM};
use windows::Win32::System::Environment::{FreeEnvironmentStringsW, GetEnvironmentStringsW};
use windows::Win32::System::Rpc::RPC_STATUS;
use windows::Win32::System::SystemServices::{
    IMAGE_DOS_HEADER, IMAGE_DOS_SIGNATURE, IMAGE_NT_SIGNATURE, SE_TOKEN_USER, SE_TOKEN_USER_1,
};
use windows::{
    core::*, Win32::Foundation::*, Win32::Security::Authorization::*, Win32::Security::*,
    Win32::System::Console::*, Win32::System::Threading::*,
};

// https://github.com/microsoft/win32metadata/issues/1857
pub const RPC_S_ACCESS_DENIED: RPC_STATUS = RPC_STATUS(ERROR_ACCESS_DENIED.0 as i32);

pub const E_FILENOTFOUND: HRESULT = ERROR_FILE_NOT_FOUND.to_hresult();
pub const E_CANCELLED: HRESULT = ERROR_CANCELLED.to_hresult();
pub const MSG_DIR_BAD_COMMAND_OR_FILE: WIN32_ERROR = WIN32_ERROR(9009);
pub const E_DIR_BAD_COMMAND_OR_FILE: HRESULT = MSG_DIR_BAD_COMMAND_OR_FILE.to_hresult();
pub const E_ACCESS_DISABLED_BY_POLICY: HRESULT = ERROR_ACCESS_DISABLED_BY_POLICY.to_hresult();

#[derive(PartialEq, Eq, Debug, Clone, Copy, PartialOrd, Ord)]
pub enum SudoMode {
    Disabled = 0,
    ForceNewWindow = 1,
    DisableInput = 2,
    Normal = 3,
}

impl TryFrom<u32> for SudoMode {
    type Error = Error;
    fn try_from(value: u32) -> Result<Self> {
        match value {
            0 => Ok(SudoMode::Disabled),
            1 => Ok(SudoMode::ForceNewWindow),
            2 => Ok(SudoMode::DisableInput),
            3 => Ok(SudoMode::Normal),
            _ => Err(ERROR_INVALID_PARAMETER.into()),
        }
    }
}

impl From<SudoMode> for u32 {
    fn from(value: SudoMode) -> Self {
        value as u32
    }
}

impl From<SudoMode> for i32 {
    fn from(val: SudoMode) -> Self {
        val as i32
    }
}

// There can be many different types that need to be LocalFree'd. PWSTR, PCWSTR, PSTR, PCSTR, PSECURITY_DESCRIPTOR
// are all distinct types, but they are compatible with the windows::core::IntoParam<HLOCAL> trait.
// There's also *mut ACL though which is also LocalAlloc'd and that's the problem (probably not the last of its kind).
// Writing a wrapper trait that is implemented for both IntoParam<HLOCAL> (or its friends) and *const/mut T
// doesn't work due to E0119. Implementing our own trait for each concrete type is highly annoying and verbose.
// So now this calls transmute_copy and zeroed. It's ugly and somewhat unsafe, but it's simple and short.
#[repr(transparent)]
pub struct OwnedLocalAlloc<T>(pub T);

impl<T> Default for OwnedLocalAlloc<T> {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

impl<T> Drop for OwnedLocalAlloc<T> {
    fn drop(&mut self) {
        unsafe {
            let ptr: HLOCAL = std::mem::transmute_copy(self);
            if !ptr.0.is_null() {
                LocalFree(ptr);
            }
        }
    }
}

impl<T> Deref for OwnedLocalAlloc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for OwnedLocalAlloc<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub unsafe extern "system" fn ignore_ctrl_c(ctrl_type: u32) -> BOOL {
    match ctrl_type {
        CTRL_C_EVENT | CTRL_BREAK_EVENT => TRUE,
        _ => FALSE,
    }
}

pub fn generate_rpc_endpoint_name(pid: u32) -> String {
    format!(r"sudo_elevate_{pid}")
}

pub fn is_running_elevated() -> Result<bool> {
    // TODO!
    // Do the thing Terminal does to see if UAC is entirely disabled:
    // Which is basically (from Utils::CanUwpDragDrop)
    //     const auto elevationType = wil::get_token_information<TOKEN_ELEVATION_TYPE>(processToken.get());
    //     const auto elevationState = wil::get_token_information<TOKEN_ELEVATION>(processToken.get());
    //     if (elevationType == TokenElevationTypeDefault && elevationState.TokenIsElevated)
    //

    let current_token = current_process_token()?;
    let elevation: TOKEN_ELEVATION = get_token_info(*current_token)?;
    Ok(elevation.TokenIsElevated == 1)
}

fn current_process_token() -> Result<Owned<HANDLE>> {
    let mut token = Owned::default();
    unsafe {
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut *token)?;
    }
    Ok(token)
}
fn get_process_token(process: HANDLE) -> Result<Owned<HANDLE>> {
    let mut token = Owned::default();
    unsafe {
        OpenProcessToken(process, TOKEN_QUERY, &mut *token)?;
    }
    Ok(token)
}

// helper trait to get the TOKEN_INFORMATION_CLASS for a given type
trait TokenInfo {
    fn info_class() -> TOKEN_INFORMATION_CLASS;
}
impl TokenInfo for TOKEN_ELEVATION_TYPE {
    fn info_class() -> TOKEN_INFORMATION_CLASS {
        TokenElevationType
    }
}
impl TokenInfo for TOKEN_ELEVATION {
    fn info_class() -> TOKEN_INFORMATION_CLASS {
        TokenElevation
    }
}
impl TokenInfo for SE_TOKEN_USER {
    fn info_class() -> TOKEN_INFORMATION_CLASS {
        TokenUser
    }
}

fn get_token_info<T: TokenInfo>(token: HANDLE) -> Result<T> {
    unsafe {
        let mut info: T = std::mem::zeroed();
        let size = std::mem::size_of::<T>() as u32;
        let mut ret_size = size;
        GetTokenInformation(
            token,
            T::info_class(),
            Some(&mut info as *mut _ as _),
            size,
            &mut ret_size,
        )?;
        Ok(info)
    }
}

pub fn can_current_user_elevate() -> Result<bool> {
    let current_token = current_process_token()?;
    let elevation_type: TOKEN_ELEVATION_TYPE = get_token_info(*current_token)?;
    Ok(elevation_type == TokenElevationTypeFull || elevation_type == TokenElevationTypeLimited)
}

pub fn get_sid_for_process(process: HANDLE) -> Result<SE_TOKEN_USER_1> {
    let process_token = get_process_token(process)?;
    let token_user: SE_TOKEN_USER = get_token_info(*process_token)?;
    Ok(token_user.Anonymous2)
}

pub fn get_current_user() -> Result<HSTRING> {
    unsafe {
        let user = get_sid_for_process(GetCurrentProcess())?;

        let mut str_sid = OwnedLocalAlloc::default();
        ConvertSidToStringSidW(PSID(&user.Sid as *const _ as _), &mut *str_sid)?;

        str_sid.to_hstring()
    }
}

pub fn is_cmd_intrinsic(application: &str) -> bool {
    // List from https://ss64.com/nt/syntax-internal.html
    //
    // The following are internal commands to cmd.exe
    // ASSOC, BREAK, CALL ,CD/CHDIR, CLS, COLOR, COPY, DATE, DEL, DIR, DPATH,
    // ECHO, ENDLOCAL, ERASE, EXIT, FOR, FTYPE, GOTO, IF, KEYS, MD/MKDIR,
    // MKLINK (vista and above), MOVE, PATH, PAUSE, POPD, PROMPT, PUSHD, REM,
    // REN/RENAME, RD/RMDIR, SET, SETLOCAL, SHIFT, START, TIME, TITLE, TYPE,
    // VER, VERIFY, VOL

    // if the application is one of these, we need to do something special
    // to make sure it works.
    //
    // We also want to makke sure it's case insensitive
    matches!(
        application.to_uppercase().as_str(),
        "ASSOC"
            | "BREAK"
            | "CALL"
            | "CD"
            | "CHDIR"
            | "CLS"
            | "COLOR"
            | "COPY"
            | "DATE"
            | "DEL"
            | "DIR"
            | "DPATH"
            | "ECHO"
            | "ENDLOCAL"
            | "ERASE"
            | "EXIT"
            | "FOR"
            | "FTYPE"
            | "GOTO"
            | "IF"
            | "KEYS"
            | "MD"
            | "MKDIR"
            | "MKLINK"
            | "MOVE"
            | "PATH"
            | "PAUSE"
            | "POPD"
            | "PROMPT"
            | "PUSHD"
            | "REM"
            | "REN"
            | "RENAME"
            | "RD"
            | "RMDIR"
            | "SET"
            | "SETLOCAL"
            | "SHIFT"
            | "START"
            | "TIME"
            | "TITLE"
            | "TYPE"
            | "VER"
            | "VERIFY"
            | "VOL"
    )
}

/// Returns the current environment as a null-delimited string.
pub fn env_as_string() -> String {
    unsafe {
        let beg = GetEnvironmentStringsW().0 as *const _;
        let mut end = beg;

        // Try to figure out the end of the double-null terminated env block.
        loop {
            extern "C" {
                fn wcslen(s: *const u16) -> usize;
            }

            let len = wcslen(end);
            if len == 0 {
                break;
            }
            end = end.add(len + 1);
        }

        // The string we want to return should not be double-null terminated.
        // The last iteration above however added `len + 1` and so we need to undo that now.
        // We use `saturating_sub` because at least theoretically `beg` may be an empty string.
        let len = usize::try_from(end.offset_from(beg))
            .unwrap()
            .saturating_sub(1);
        let str = String::from_utf16_lossy(from_raw_parts(beg, len));
        let _ = FreeEnvironmentStringsW(PCWSTR(beg));
        str
    }
}

/// Splits a null-delimited environment string into key/value pairs.
pub fn env_from_raw_bytes(env_string: &str) -> impl Iterator<Item = (&OsStr, &OsStr)> {
    env_string.split('\0').filter_map(|s| {
        // In the early days the cmd.exe devs added env variables that start with "=".
        // They look like "=C:=C:\foo\bar" and are used to track per-drive CWDs across cmd child-processes.
        // See here for more information: https://devblogs.microsoft.com/oldnewthing/20100506-00/?p=14133
        // The get() call simultaneously takes care of rejecting empty strings, which is neat.
        let idx = s.get(1..)?.find('=')?;
        // The `.get(1..)` call will slice off 1 character from the start of the string and thus from the `idx` value.
        // This means that when we want to split the string into two parts `[0,idx)` and `(idx,length)`
        // (= `[idx+1,length)` = without the "=" character) then we need to add +1 to both sides now.
        Some((OsStr::new(&s[..idx + 1]), OsStr::new(&s[idx + 2..])))
    })
}

/// Windows does not actually support distinct command line parameters. They're all just given as a single string.
/// We can't just use `.join(" ")` either, because this breaks arguments with whitespaces. This function handles these details.
pub fn join_args<T: AsRef<str>>(args: &[T]) -> String {
    // We estimate 3*args.len() overhead per arg: 2 quotes and 1 whitespace.
    let expected_len = args
        .len()
        .checked_mul(3)
        .and_then(|n| {
            args.iter()
                .map(|s| s.as_ref().len())
                .try_fold(n, usize::checked_add)
        })
        .unwrap();

    let mut accumulator = Vec::with_capacity(expected_len);

    // Fun fact: At the time of writing, Windows Terminal has a function called `QuoteAndEscapeCommandlineArg`
    // and Rust's `std::sys::windows::args` crate has a `append_arg` function. Both functions are pretty much
    // 1:1 identical to the code below, but both were written independently. I guess there aren't too many
    // ways to express this concept, but I'm still somewhat surprised there aren't multiple ways to do it.
    for (idx, arg) in args.iter().enumerate() {
        if idx != 0 {
            accumulator.push(b' ');
        }

        let str = arg.as_ref();
        let quote = str.is_empty() || str.contains(' ') || str.contains('\t');
        if quote {
            accumulator.push(b'"');
        }

        let mut backslashes: usize = 0;
        for &x in str.as_bytes() {
            if x == b'\\' {
                backslashes += 1;
            } else {
                if x == b'"' {
                    accumulator.extend((0..=backslashes).map(|_| b'\\'));
                }
                backslashes = 0;
            }
            accumulator.push(x);
        }

        if quote {
            accumulator.extend((0..backslashes).map(|_| b'\\'));
            accumulator.push(b'"');
        }
    }

    // Assuming that our `args` slice was UTF8 the accumulator can't suddenly contain non-UTF8.
    unsafe { String::from_utf8_unchecked(accumulator) }
}

/// Joins a list of strings into a single string, each of which is null-terminated (including the final one).
pub fn pack_string_list_for_rpc<T: AsRef<str>>(args: &[T]) -> String {
    let expected_len = args
        .iter()
        .map(|s| s.as_ref().len())
        // We extend each arg in args with 1 character: \0.
        // This results in an added overhead of args.len() characters, which we
        // implicitly add by setting it as the initial value for the fold().
        .try_fold(args.len(), usize::checked_add)
        .unwrap();

    let mut accumulator = String::with_capacity(expected_len);
    for arg in args {
        accumulator.push_str(arg.as_ref());
        accumulator.push('\0');
    }
    accumulator
}

/// Splits a string generated by `pack_args` up again.
pub fn unpack_string_list_from_rpc(args: Utf8Str) -> Result<Vec<String>> {
    Ok(args
        .as_str()?
        .split_terminator('\0')
        .map(String::from)
        .collect())
}

pub trait ConfigProvider {
    fn get_setting_mode(&self) -> Result<u32>;
    fn get_policy_mode(&self) -> Result<u32>;
}

#[derive(Default)]
pub struct RegistryConfigProvider;
impl ConfigProvider for RegistryConfigProvider {
    fn get_setting_mode(&self) -> Result<u32> {
        windows_registry::LOCAL_MACHINE
            .open("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Sudo")
            .and_then(|key| key.get_u32("Enabled"))
    }
    fn get_policy_mode(&self) -> Result<u32> {
        windows_registry::LOCAL_MACHINE
            .open("SOFTWARE\\Policies\\Microsoft\\Windows\\Sudo")
            .and_then(|key| key.get_u32("Enabled"))
    }
}

/// Get the current mode allowed by policy.
/// * If the policy isn't set (we fail to read the reg key), we'll return Ok(3)
/// (to indicate that all modes up to inline are allowed).
/// * If the policy is set, we'll return the value from the policy.
/// * If we fail to read the policy for any other reason, we'll return an error
/// (which should be treated as "disabled by policy")
pub fn get_allowed_mode_from_policy(config: &impl ConfigProvider) -> Result<SudoMode> {
    match config.get_policy_mode() {
        Ok(v) => v.min(3).try_into(),
        // This is okay! No policy state really means that it's _not_ disabled by policy.
        Err(e) if e.code() == E_FILENOTFOUND => Ok(SudoMode::Normal),
        Err(e) => Err(e),
    }
}

/// Get the current setting mode from the registry. If the setting isn't there,
/// we're disabled. If we fail to read the setting, we're disabled. Errors
/// should be treated as disabled.
pub fn get_setting_mode(config: &impl ConfigProvider) -> Result<SudoMode> {
    match config.get_setting_mode() {
        Ok(v) => v.min(3).try_into(),
        Err(e) if e.code() == E_FILENOTFOUND => Ok(SudoMode::Disabled),
        Err(e) => Err(e),
    }
}

pub fn get_allowed_mode(config: &impl ConfigProvider) -> Result<SudoMode> {
    let allowed_mode_from_policy = get_allowed_mode_from_policy(config)?;
    if allowed_mode_from_policy == SudoMode::Disabled {
        return Err(E_ACCESS_DISABLED_BY_POLICY.into());
    }
    let setting_mode = get_setting_mode(config).unwrap_or(SudoMode::Disabled);

    SudoMode::try_from(std::cmp::min::<u32>(
        allowed_mode_from_policy.into(),
        setting_mode.into(),
    ))
}

pub fn get_process_path_from_handle(process: HANDLE) -> Result<PathBuf> {
    let mut buffer = vec![0u16; MAX_PATH as usize];

    // Call QueryFullProcessImageNameW in a loop to make sure we actually get the
    // full path. We have to do it in a loop, because QueryFullProcessImageNameW
    // doesn't actually tell us how big the buffer needs to be on error.
    loop {
        let mut len = buffer.len() as u32;
        match unsafe {
            QueryFullProcessImageNameW(
                process,
                Default::default(),
                PWSTR(buffer.as_mut_ptr()),
                &mut len,
            )
        } {
            Ok(()) => return Ok(PathBuf::from(OsString::from_wide(&buffer[..len as usize]))),
            Err(err) if err.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() => return Err(err),
            Err(_) => buffer.resize(buffer.len() * 2, 0),
        };
    }
}

/// Check that the client process is the same as the server process.
pub fn check_client(client_handle: HANDLE) -> Result<()> {
    // Open a handle to the provided process
    let process_path = get_process_path_from_handle(client_handle)?;
    let our_path = std::env::current_exe().unwrap();
    trace_log_message(&format!(
        "{process_path:?} connected to server {our_path:?}"
    ));

    // Now, is this path the same as us? (ignoring case)
    if !process_path
        .as_os_str()
        .eq_ignore_ascii_case(our_path.as_os_str())
    {
        return Err(E_ACCESSDENIED.into());
    }

    let mut client_sid = get_sid_for_process(client_handle)?;
    let mut our_sid = unsafe { get_sid_for_process(GetCurrentProcess())? };
    // Are these SIDs the same? This check prevents over-the-shoulder elevation
    // when the RPC server is in use.
    unsafe {
        // If the SID structures are equal, the return value is nonzero (TRUE)
        // Then the windows-rs projection will take the true and convert that to Ok(()), or FALSE to Err(GetLastError())
        // EqualSid(PSID{ 0: &mut client_sid.Sid as *mut _ } , &mut our_sid.Sid as *mut _ as PSID)?;
        let client_psid: PSID = PSID(&mut client_sid.Buffer as *mut _ as _);
        let our_psid: PSID = PSID(&mut our_sid.Buffer as *mut _ as _);
        EqualSid(client_psid, our_psid)?;
    };

    Ok(())
}

/// Make a Windows path absolute, using GetFullPathNameW to resolve the file on disk.
/// Largely lifted from the rust stdlib, because it's _currently_ a nightly-only function.
/// We don't have all the same internal stdlib helpers they do, but it's effectively the same..
pub fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().as_encoded_bytes().starts_with(br"\\?\") {
        return Ok(path.into());
    }
    let lpfilename = HSTRING::from(path.as_os_str());
    let mut buffer = vec![0u16; MAX_PATH as usize];
    loop {
        // GetFullPathNameW will return the required buffer size if the buffer is too small.
        let res = unsafe { GetFullPathNameW(&lpfilename, Some(buffer.as_mut_slice()), None) };
        match res as usize {
            0 => return Err(Error::from_win32()), // returns GLE
            len if len <= buffer.len() => {
                return Ok(PathBuf::from(OsString::from_wide(&buffer[..len])))
            }
            new_len => buffer.resize(new_len, 0),
        }
    }
}

unsafe fn read_struct_at<T>(f: &mut File, offset: u64) -> Result<T> {
    let mut data = MaybeUninit::<T>::uninit();
    let bytes = from_raw_parts_mut(data.as_mut_ptr() as *mut u8, size_of::<T>());
    let read = f.seek_read(bytes, offset)?;
    if read != bytes.len() {
        return Err(ERROR_HANDLE_EOF.into());
    }
    Ok(data.assume_init())
}

pub fn get_exe_subsystem<P: AsRef<Path>>(path: P) -> Result<IMAGE_SUBSYSTEM> {
    let mut f = File::open(path)?;

    let dos: IMAGE_DOS_HEADER = unsafe { read_struct_at(&mut f, 0)? };
    if dos.e_magic != IMAGE_DOS_SIGNATURE {
        return Err(ERROR_BAD_EXE_FORMAT.into());
    }

    // IMAGE_NT_HEADERS32 and IMAGE_NT_HEADERS64 have different sizes,
    // but the offset of the .OptionalHeader.Subsystem member is identical.
    let nt: IMAGE_NT_HEADERS32 = unsafe { read_struct_at(&mut f, dos.e_lfanew as u64)? };
    if nt.Signature != IMAGE_NT_SIGNATURE {
        return Err(ERROR_BAD_EXE_FORMAT.into());
    }

    Ok(nt.OptionalHeader.Subsystem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Diagnostics::Debug::{
        IMAGE_SUBSYSTEM_WINDOWS_CUI, IMAGE_SUBSYSTEM_WINDOWS_GUI,
    };

    #[test]
    fn test_env_from_raw_string() {
        let raw_string = "foo=bar\0baz=qux\0\0";
        let env_map: Vec<_> = env_from_raw_bytes(raw_string).collect();
        assert_eq!(env_map.len(), 2);
        assert_eq!(env_map[0], (OsStr::new("foo"), OsStr::new("bar")));
        assert_eq!(env_map[1], (OsStr::new("baz"), OsStr::new("qux")));
    }

    #[test]
    fn test_env_with_drive_vars() {
        let raw_string = "foo=bar\0=D:=D:\\qux\0\0";
        let env_map: Vec<_> = env_from_raw_bytes(raw_string).collect();
        assert_eq!(env_map.len(), 2);
        assert_eq!(env_map[0], (OsStr::new("foo"), OsStr::new("bar")));
        assert_eq!(env_map[1], (OsStr::new("=D:"), OsStr::new("D:\\qux")));
    }

    #[test]
    fn test_join_args() {
        assert_eq!(join_args(&[""; 0]), "");
        assert_eq!(join_args(&["foo", "bar"]), "foo bar");
        assert_eq!(join_args(&["f \too", " bar\t"]), "\"f \too\" \" bar\t\"");
        assert_eq!(join_args(&["f\\\"oo", "\"bar\""]), r#"f\\\"oo \"bar\""#);
    }

    #[test]
    fn test_pack_args() {
        assert_eq!(pack_string_list_for_rpc(&[""; 0]), "");
        assert_eq!(pack_string_list_for_rpc(&["foo"]), "foo\0");
        assert_eq!(pack_string_list_for_rpc(&["foo", "bar"]), "foo\0bar\0");
    }

    #[test]
    fn test_unpack_args() {
        assert_eq!(unpack_string_list_from_rpc("".into()).unwrap(), [""; 0]);
        assert_eq!(unpack_string_list_from_rpc("foo".into()).unwrap(), ["foo"]);
        assert_eq!(
            unpack_string_list_from_rpc("foo\0".into()).unwrap(),
            ["foo"]
        );
        assert_eq!(
            unpack_string_list_from_rpc("foo\0bar".into()).unwrap(),
            ["foo", "bar"]
        );
        assert_eq!(
            unpack_string_list_from_rpc("foo\0bar\0".into()).unwrap(),
            ["foo", "bar"]
        );
        assert_eq!(
            unpack_string_list_from_rpc("\0\0".into()).unwrap(),
            ["", ""]
        );
        assert_eq!(
            unpack_string_list_from_rpc("foo\0\0bar\0\0".into()).unwrap(),
            ["foo", "", "bar", ""]
        );
    }

    #[test]
    fn test_get_exe_subsystem() {
        assert_eq!(
            Ok(IMAGE_SUBSYSTEM_WINDOWS_CUI),
            get_exe_subsystem(r"C:\Windows\System32\nslookup.exe")
        );
        assert_eq!(
            Ok(IMAGE_SUBSYSTEM_WINDOWS_GUI),
            get_exe_subsystem(r"C:\Windows\notepad.exe")
        );
    }

    /// config tests
    struct TestConfigProvider {
        setting_mode: Result<u32>,
        policy_mode: Result<u32>,
    }

    impl ConfigProvider for TestConfigProvider {
        fn get_setting_mode(&self) -> Result<u32> {
            self.setting_mode.clone()
        }
        fn get_policy_mode(&self) -> Result<u32> {
            self.policy_mode.clone()
        }
    }

    #[test]
    fn test_get_allowed_mode_from_policy() {
        // no setting at all
        let config = TestConfigProvider {
            setting_mode: Err(E_FILENOTFOUND.into()),
            policy_mode: Err(E_FILENOTFOUND.into()),
        };
        assert_eq!(get_setting_mode(&config).unwrap(), SudoMode::Disabled);
        assert_eq!(
            get_allowed_mode_from_policy(&config).unwrap(),
            SudoMode::Normal
        );
        assert_eq!(get_allowed_mode(&config).unwrap(), SudoMode::Disabled);

        // Setting set to 3 (normal), but policy only allows 2 (disable input)
        let config = TestConfigProvider {
            setting_mode: Ok(3),
            policy_mode: Ok(2),
        };
        assert_eq!(get_setting_mode(&config).unwrap(), SudoMode::Normal);
        assert_eq!(
            get_allowed_mode_from_policy(&config).unwrap(),
            SudoMode::DisableInput
        );
        assert_eq!(get_allowed_mode(&config).unwrap(), SudoMode::DisableInput);

        // policy is out of range
        let config = TestConfigProvider {
            setting_mode: Ok(3),
            policy_mode: Ok(4),
        };
        assert_eq!(get_setting_mode(&config).unwrap(), SudoMode::Normal);
        assert_eq!(
            get_allowed_mode_from_policy(&config).unwrap(),
            SudoMode::Normal
        );
        assert_eq!(get_allowed_mode(&config).unwrap(), SudoMode::Normal);

        // entirely disabled by policy
        let config = TestConfigProvider {
            setting_mode: Ok(3),
            policy_mode: Ok(0),
        };
        assert_eq!(get_setting_mode(&config).unwrap(), SudoMode::Normal);
        assert_eq!(
            get_allowed_mode_from_policy(&config).unwrap(),
            SudoMode::Disabled
        );
        assert_eq!(
            get_allowed_mode(&config),
            Err(E_ACCESS_DISABLED_BY_POLICY.into())
        );

        // No policy config found
        let config = TestConfigProvider {
            setting_mode: Ok(3),
            policy_mode: Err(E_FILENOTFOUND.into()),
        };
        assert_eq!(get_setting_mode(&config).unwrap(), SudoMode::Normal);
        assert_eq!(
            get_allowed_mode_from_policy(&config).unwrap(),
            SudoMode::Normal
        );
        assert_eq!(get_allowed_mode(&config).unwrap(), SudoMode::Normal);

        // not set, but disabled by policy
        let config = TestConfigProvider {
            setting_mode: Err(E_FILENOTFOUND.into()),
            policy_mode: Ok(0),
        };
        assert_eq!(get_setting_mode(&config).unwrap(), SudoMode::Disabled);
        assert_eq!(
            get_allowed_mode_from_policy(&config).unwrap(),
            SudoMode::Disabled
        );
        assert_eq!(
            get_allowed_mode(&config),
            Err(E_ACCESS_DISABLED_BY_POLICY.into())
        );
    }
}
