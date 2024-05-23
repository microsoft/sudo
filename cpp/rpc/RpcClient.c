#define WIN32_LEAN_AND_MEAN
#include <Windows.h>

#include <stdbool.h>
#include <stdlib.h>

// Our generated header file
#include "sudo_rpc.h"

// Rust can't (easily) handle SEH exceptions, which the RPC however unfortunately uses.
// And so this wrapper C implementation exists.

// From <wil/rpc_helpers.h>:
// Some RPC exceptions are already HRESULTs. Others are in the regular Win32
// error space. If the incoming exception code isn't an HRESULT, wrap it.
inline HRESULT map_rpc_status(DWORD code)
{
    return IS_ERROR(code) ? code : HRESULT_FROM_WIN32(code);
}

HRESULT seh_wrapper_client_DoElevationRequest(
    RPC_IF_HANDLE binding,
    HANDLE parent_handle,
    const HANDLE* pipe_handles,
    const HANDLE* file_handles,
    DWORD sudo_mode,
    UTF8_STRING application,
    UTF8_STRING args,
    UTF8_STRING target_dir,
    UTF8_STRING env_vars,
    GUID eventId,
    HANDLE* child)
{
    RpcTryExcept
    {
        return client_DoElevationRequest(
            binding,
            parent_handle,
            pipe_handles,
            file_handles,
            sudo_mode,
            application,
            args,
            target_dir,
            env_vars,
            eventId,
            child);
    }
    RpcExcept(RpcExceptionFilter(RpcExceptionCode()))
    {
        return map_rpc_status(RpcExceptionCode());
    }
    RpcEndExcept;
}

HRESULT seh_wrapper_client_Shutdown(RPC_IF_HANDLE binding)
{
    RpcTryExcept
    {
        client_Shutdown(binding);
        return S_OK;
    }
    RpcExcept(RpcExceptionFilter(RpcExceptionCode()))
    {
        return map_rpc_status(RpcExceptionCode());
    }
    RpcEndExcept;
}

/******************************************************/
/*         MIDL allocate and free                     */
/******************************************************/

void __RPC_FAR* __RPC_USER midl_user_allocate(size_t len)
{
    return (malloc(len));
}

void __RPC_USER midl_user_free(void __RPC_FAR* ptr)
{
    free(ptr);
}
