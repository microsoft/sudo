use crate::helpers::*;
use crate::{
    elevate_handler::handle_elevation_request, messages::ElevateRequest, rpc_bindings::Utf8Str,
};
use std::ffi::{c_void, CStr};
use std::mem::{size_of, take};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::{ERROR_BUSY, GENERIC_ALL, HANDLE, PSID};
use windows::{
    core::*, Win32::Security::Authorization::*, Win32::Security::*, Win32::System::Memory::*,
    Win32::System::Rpc::*, Win32::System::SystemServices::*, Win32::System::Threading::*,
};

extern "C" {
    static mut server_sudo_rpc_ServerIfHandle: *mut c_void;
}

static mut EXPECTED_CLIENT_PID: u32 = 0;

// Process-wide mutex to ensure that only one request is handled at a time. The
// bool inside the atomic is true if we've already started handling a request.
static RPC_SERVER_IN_USE: AtomicBool = AtomicBool::new(false);

// * Context: The callback function may pass this handle to
//   RpcImpersonateClient, RpcBindingServerFromClient,
//   RpcGetAuthorizationContextForClient, or any other server side function that
//   accepts a client binding handle to obtain information about the client.
//
// The callback function should return RPC_S_OK, if the client is allowed to
// call methods in this interface.
unsafe extern "system" fn rpc_server_callback(
    _interface_uuid: *const c_void,
    context: *const c_void,
) -> RPC_STATUS {
    let mut client_handle = OwnedHandle::default();
    let status = I_RpcOpenClientProcess(
        Some(context),
        PROCESS_QUERY_LIMITED_INFORMATION.0,
        &mut *client_handle as *mut _ as _,
    );
    if status != RPC_S_OK {
        return status;
    }

    // Check #1: We'll check that the client process is the one we expected,
    // when we were first started.
    let client_pid = GetProcessId(*client_handle); // if this fails, it returns 0
    if client_pid == 0 || client_pid != EXPECTED_CLIENT_PID {
        return RPC_S_ACCESS_DENIED;
    }

    // Check #2: Check that the client process is the same as the server process.
    if check_client(*client_handle).is_err() {
        return RPC_S_ACCESS_DENIED;
    }

    RPC_S_OK
}

// MSDN regarding SetSecurityDescriptorSacl:
// > The SACL is referenced by, not copied into, the security descriptor.
// --> To return a SD we need to hold onto everything we allocated. Yay.
#[derive(Default)]
struct OwnedSecurityDescriptor {
    pub sd: SECURITY_DESCRIPTOR,
    sacl: OwnedLocalAlloc<*mut ACL>,
    dacl: OwnedLocalAlloc<*mut ACL>,
}

// Creates a descriptor of form
//   D:(A;;GA;;;<pid's sid>)S:(ML;;NWNRNX;;;ME)
fn create_security_descriptor_for_process(pid: u32) -> Result<OwnedSecurityDescriptor> {
    unsafe {
        let mut s: OwnedSecurityDescriptor = Default::default();
        let psd = PSECURITY_DESCRIPTOR(&mut s.sd as *mut _ as _);
        InitializeSecurityDescriptor(psd, SECURITY_DESCRIPTOR_REVISION)?;

        // SACL
        {
            let user = {
                let process =
                    OwnedHandle::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?);
                get_sid_for_process(*process)?
            };

            let ea = [EXPLICIT_ACCESS_W {
                grfAccessPermissions: GENERIC_ALL.0,
                grfAccessMode: SET_ACCESS,
                grfInheritance: NO_INHERITANCE,
                Trustee: TRUSTEE_W {
                    pMultipleTrustee: null_mut(),
                    MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                    TrusteeForm: TRUSTEE_IS_SID,
                    TrusteeType: TRUSTEE_IS_USER,
                    ptstrName: PWSTR(&user.Sid as *const _ as _),
                },
            }];

            SetEntriesInAclW(Some(&ea), None, &mut *s.dacl).ok()?;
            SetSecurityDescriptorDacl(psd, true, Some(*s.dacl), false)?;
        }

        // DACL
        {
            // windows-rs doesn't have a definition for this macro.
            const SECURITY_MAX_SID_SIZE: usize = 88;

            let mut sid_buffer = [0u8; SECURITY_MAX_SID_SIZE];
            let mut sid_len = sid_buffer.len() as u32;
            let sid = PSID(&mut sid_buffer as *mut _ as _);
            CreateWellKnownSid(WinMediumLabelSid, None, sid, &mut sid_len)?;

            const SACL_BUFFER_PREFIX_LEN: usize =
                size_of::<ACL>() + size_of::<SYSTEM_MANDATORY_LABEL_ACE>();
            let sacl_len = SACL_BUFFER_PREFIX_LEN as u32 + sid_len;
            s.sacl.0 = LocalAlloc(LMEM_FIXED, sacl_len as usize)?.0 as _;

            InitializeAcl(*s.sacl, sacl_len, ACL_REVISION)?;
            AddMandatoryAce(
                *s.sacl,
                ACL_REVISION,
                ACE_FLAGS(0),
                SYSTEM_MANDATORY_LABEL_NO_READ_UP
                    | SYSTEM_MANDATORY_LABEL_NO_WRITE_UP
                    | SYSTEM_MANDATORY_LABEL_NO_EXECUTE_UP,
                sid,
            )?;

            SetSecurityDescriptorSacl(psd, true, Some(*s.sacl), false)?;
        }

        Ok(s)
    }
}

pub fn rpc_server_setup(endpoint: &CStr, expected_client_pid: u32) -> Result<()> {
    let owned_sd = create_security_descriptor_for_process(expected_client_pid)?;

    unsafe {
        RpcServerUseProtseqEpA(
            /* Protseq            */ s!("ncalrpc"),
            /* MaxCalls           */ RPC_C_LISTEN_MAX_CALLS_DEFAULT,
            /* Endpoint           */ PCSTR(endpoint.as_ptr() as _),
            /* SecurityDescriptor */ Some(&owned_sd.sd as *const _ as _),
        )
        .ok()?;
        RpcServerRegisterIf3(
            /* IfSpec             */ server_sudo_rpc_ServerIfHandle,
            /* MgrTypeUuid        */ None,
            /* MgrEpv             */ None,
            /* Flags              */ RPC_IF_ALLOW_LOCAL_ONLY | RPC_IF_ALLOW_SECURE_ONLY,
            /* MaxCalls           */ RPC_C_LISTEN_MAX_CALLS_DEFAULT,
            /* MaxRpcSize         */ u32::MAX,
            /* IfCallback         */ Some(rpc_server_callback),
            /* SecurityDescriptor */ Some(&owned_sd.sd as *const _ as _),
        )
        .ok()?;

        EXPECTED_CLIENT_PID = expected_client_pid;

        let res = RpcServerListen(
            /* MinimumCallThreads */ 1,
            /* MaxCalls           */ RPC_C_LISTEN_MAX_CALLS_DEFAULT,
            /* DontWait           */ 0,
        );
        if res.is_err() {
            _ = RpcServerUnregisterIf(None, None, 0);
        }
        res.ok()
    }
}

// This is the RPC's sudo_rpc::Shutdown callback function.
#[no_mangle]
unsafe extern "C" fn server_Shutdown(_binding: *const c_void) {
    _ = TerminateProcess(GetCurrentProcess(), 0);
}

// This is the RPC's sudo_rpc::DoElevationRequest callback function.
#[no_mangle]
pub extern "C" fn server_DoElevationRequest(
    _binding: *const c_void,
    parent_handle: HANDLE,
    pipe_handles: *const [HANDLE; 3], // in, out, err
    file_handles: *const [HANDLE; 3], // in, out, err
    sudo_mode: u32,
    application: Utf8Str,
    args: Utf8Str,
    target_dir: Utf8Str,
    env_vars: Utf8Str,
    event_id: GUID,
    child: *mut HANDLE,
) -> HRESULT {
    // Only the first caller will get their request handled. Everyone else will
    // be forced to bail out.
    if RPC_SERVER_IN_USE.swap(true, Ordering::Relaxed) {
        // We're already in the middle of handling a request.
        return ERROR_BUSY.to_hresult();
    }

    // Here, we've locked the mutex and we're the only ones handling a request.
    //
    // And, we've set the atom to true, so if someone _does_ connect to us after
    // this function releases the lock, then they'll also bail out.

    // Immediately unregister ourself. This will prevent a future caller from
    // getting to us (but won't cancel the current request we're already in the
    // middle of replying to).
    unsafe {
        _ = RpcMgmtStopServerListening(None);
        _ = RpcServerUnregisterIf(None, None, 0);
    }

    let result = wrap_elevate_request(
        parent_handle,
        pipe_handles,
        file_handles,
        sudo_mode,
        application,
        args,
        target_dir,
        env_vars,
        event_id,
    )
    .and_then(|req| handle_elevation_request(&req));

    match result {
        Ok(mut handle) => {
            unsafe { child.write(take(&mut handle.0)) };
            HRESULT::default()
        }
        Err(err) => err.into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn wrap_elevate_request(
    parent_handle: HANDLE,
    pipe_handles: *const [HANDLE; 3], // in, out, err
    file_handles: *const [HANDLE; 3], // in, out, err
    sudo_mode: u32,
    application: Utf8Str,
    args: Utf8Str,
    target_dir: Utf8Str,
    env_vars: Utf8Str,
    event_id: GUID,
) -> Result<ElevateRequest> {
    let parent_pid = unsafe { GetProcessId(parent_handle) };
    let handles = unsafe {
        let pipes = &*pipe_handles;
        let files = &*file_handles;
        std::array::from_fn(|i| if pipes[i].0 != 0 { pipes[i] } else { files[i] })
    };

    Ok(ElevateRequest {
        parent_pid,
        handles,
        sudo_mode: sudo_mode.try_into()?,
        application: application.as_str()?.to_owned(),
        args: unpack_string_list_from_rpc(args)?,
        target_dir: target_dir.as_str()?.to_owned(),
        env_vars: env_vars.as_str()?.to_owned(),
        event_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_security_descriptor_for_process() {
        fn sd_to_string(sd: PSECURITY_DESCRIPTOR) -> Result<String> {
            unsafe {
                let mut buffer = PSTR::null();
                ConvertSecurityDescriptorToStringSecurityDescriptorA(
                    sd,
                    SDDL_REVISION,
                    DACL_SECURITY_INFORMATION
                        | LABEL_SECURITY_INFORMATION
                        | OWNER_SECURITY_INFORMATION,
                    &mut buffer,
                    None,
                )?;
                Ok(buffer.to_string()?)
            }
        }

        let s = create_security_descriptor_for_process(unsafe { GetCurrentProcessId() }).unwrap();
        let str = sd_to_string(PSECURITY_DESCRIPTOR(&s.sd as *const _ as _)).unwrap();
        assert!(str.starts_with("D:(A;;GA;;;"));
        assert!(str.ends_with(")S:(ML;;NWNRNX;;;ME)"));
    }
}
