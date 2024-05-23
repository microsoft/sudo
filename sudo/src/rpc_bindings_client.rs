use crate::helpers::SudoMode;
use crate::rpc_bindings::Utf8Str;
use std::ffi::{c_void, CStr};
use windows::core::{s, GUID, HRESULT, PCSTR, PSTR};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{GetFileType, FILE_TYPE_DISK, FILE_TYPE_PIPE};
use windows::Win32::System::Rpc::{
    RpcBindingFree, RpcBindingFromStringBindingA, RpcMgmtIsServerListening,
    RpcStringBindingComposeA, RpcStringFreeA, RPC_STATUS, RPC_S_OK,
};

extern "C" {
    static mut client_sudo_rpc_ClientIfHandle: *mut c_void;

    fn seh_wrapper_client_DoElevationRequest(
        binding: *mut c_void,
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
    ) -> HRESULT;

    fn seh_wrapper_client_Shutdown(binding: *mut c_void) -> HRESULT;
}

pub fn rpc_client_setup(endpoint: &CStr) -> RPC_STATUS {
    unsafe {
        let mut string_binding = PSTR::null();
        let status = RpcStringBindingComposeA(
            /* ObjUuid       */ None,
            /* ProtSeq       */ s!("ncalrpc"),
            /* NetworkAddr   */ None,
            /* Endpoint      */ PCSTR(endpoint.as_ptr() as _),
            /* Options       */ None,
            /* StringBinding */ Some(&mut string_binding),
        );
        if status != RPC_S_OK {
            return status;
        }

        let status = RpcBindingFromStringBindingA(
            PCSTR(string_binding.0),
            std::ptr::addr_of_mut!(client_sudo_rpc_ClientIfHandle),
        );
        // Don't forget to free the previously allocated string before potentially returning. :)
        RpcStringFreeA(&mut string_binding);
        if status != RPC_S_OK {
            return status;
        }

        RpcMgmtIsServerListening(Some(client_sudo_rpc_ClientIfHandle))
    }
}

/// Cleans up the RPC server. This is implemented on the server-side in
/// server_Shutdown. It will TerminateProcess the RPC server, really really
/// making sure no one can use it anymore.
pub fn rpc_client_cleanup() {
    unsafe {
        _ = seh_wrapper_client_Shutdown(client_sudo_rpc_ClientIfHandle);
        _ = RpcBindingFree(std::ptr::addr_of_mut!(client_sudo_rpc_ClientIfHandle));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn rpc_client_do_elevation_request(
    parent_handle: HANDLE,
    handles: &[HANDLE; 3], // in, out, err
    sudo_mode: SudoMode,
    application: Utf8Str,
    args: Utf8Str,
    target_dir: Utf8Str,
    env_vars: Utf8Str,
    event_id: GUID,
    child: *mut HANDLE,
) -> HRESULT {
    let mut pipe_handles = [HANDLE::default(); 3];
    let mut file_handles = [HANDLE::default(); 3];

    for i in 0..3 {
        match unsafe { GetFileType(handles[i]) } {
            FILE_TYPE_PIPE => pipe_handles[i] = handles[i],
            FILE_TYPE_DISK => file_handles[i] = handles[i],
            _ => {}
        }
    }

    unsafe {
        seh_wrapper_client_DoElevationRequest(
            client_sudo_rpc_ClientIfHandle,
            parent_handle,
            &pipe_handles,
            &file_handles,
            sudo_mode.into(),
            application,
            args,
            target_dir,
            env_vars,
            event_id,
            child,
        )
    }
}
