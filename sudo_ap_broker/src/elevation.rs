// Process Elevation with SMAA Support
// Creates elevated processes in System Managed Administrator Account context

use anyhow::{anyhow, Context, Result};
use std::ptr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::{debug, info, warn};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    Security::{
        DuplicateTokenEx, ImpersonateLoggedOnUser, RevertToSelf, SecurityImpersonation,
        TokenPrimary, TOKEN_ACCESS_MASK, TOKEN_ALL_ACCESS, TOKEN_ASSIGN_PRIMARY,
        TOKEN_DUPLICATE, TOKEN_QUERY,
    },
    System::Threading::{
        CreateProcessAsUserW, GetCurrentProcess, OpenProcessToken, WaitForSingleObject,
        GetExitCodeProcess, TerminateProcess, PROCESS_INFORMATION, STARTUPINFOW,
        CREATE_NEW_CONSOLE, CREATE_NO_WINDOW, DETACHED_PROCESS, INFINITE,
    },
};

use sudo::broker_protocol::{
    ElevationRequest, ElevationResponse, ExecutionMode, StatusCode,
};

/// Handles process elevation
pub struct ElevationHandler {}

impl ElevationHandler {
    pub fn new() -> Self {
        Self {}
    }
    
    pub fn elevate(
        &self,
        request: ElevationRequest,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Result<ElevationResponse> {
        debug!("Starting elevation for: {}", request.command);
        
        // Validate authentication token
        if request.auth_token.is_empty() {
            return Ok(ElevationResponse {
                status: StatusCode::AuthenticationFailed,
                error_message: Some("Authentication token is required".to_string()),
                process_id: None,
                exit_code: None,
                is_running: false,
            });
        }
        
        // Token validation is performed by hello_auth.rs verify_auth_token()
        // HMAC-SHA256 signature verification ensures token integrity
        
        // Get current process token (running as SYSTEM)
        let system_token = self.get_current_token()?;
        
        // Create elevated process
        match self.create_elevated_process(&request, system_token) {
            Ok((process_handle, process_id)) => {
                info!("Created elevated process PID={}", process_id);
                
                // Wait for process based on execution mode
                let exit_code = if matches!(request.execution_mode, ExecutionMode::Inline) {
                    // Wait for process to complete
                    self.wait_for_process(process_handle, request.timeout_ms, shutdown_flag)?
                } else {
                    // Detached - don't wait
                    None
                };
                
                unsafe { CloseHandle(process_handle).ok(); }
                unsafe { CloseHandle(system_token).ok(); }
                
                Ok(ElevationResponse {
                    status: StatusCode::Success,
                    error_message: None,
                    process_id: Some(process_id),
                    exit_code,
                    is_running: exit_code.is_none(),
                })
            }
            Err(e) => {
                warn!("Failed to create elevated process: {:#}", e);
                unsafe { CloseHandle(system_token).ok(); }
                
                Ok(ElevationResponse {
                    status: StatusCode::AccessDenied,
                    error_message: Some(format!("Failed to create elevated process: {:#}", e)),
                    process_id: None,
                    exit_code: None,
                    is_running: false,
                })
            }
        }
    }
    
    fn get_current_token(&self) -> Result<HANDLE> {
        let mut token = HANDLE::default();
        
        unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY,
                &mut token,
            )?;
        }
        
        Ok(token)
    }
    
    fn create_elevated_process(
        &self,
        request: &ElevationRequest,
        token: HANDLE,
    ) -> Result<(HANDLE, u32)> {
        // Build command line
        let mut cmdline = request.command.clone();
        if !request.arguments.is_empty() {
            cmdline.push(' ');
            cmdline.push_str(&request.arguments.join(" "));
        }
        
        let mut cmdline_wide: Vec<u16> = cmdline.encode_utf16().chain(Some(0)).collect();
        
        // Working directory
        let working_dir_wide: Option<Vec<u16>> = request.working_directory.as_ref().map(|wd| {
            wd.encode_utf16().chain(Some(0)).collect()
        });
        
        let working_dir_ptr = working_dir_wide
            .as_ref()
            .map(|wd| PCWSTR(wd.as_ptr()))
            .unwrap_or(PCWSTR(ptr::null()));
        
        // Setup startup info
        let mut startup_info = STARTUPINFOW::default();
        startup_info.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        
        // Determine creation flags based on execution mode
        let creation_flags = match request.execution_mode {
            ExecutionMode::NewWindow => CREATE_NEW_CONSOLE,
            ExecutionMode::Hidden => CREATE_NO_WINDOW,
            ExecutionMode::Inline => DETACHED_PROCESS,
            ExecutionMode::CurrentWindow => 0,
            ExecutionMode::DisableInput => CREATE_NO_WINDOW,
        };
        
        // Create process as elevated user
        let mut process_info = PROCESS_INFORMATION::default();
        
        unsafe {
            CreateProcessAsUserW(
                token,
                None,
                PWSTR(cmdline_wide.as_mut_ptr()),
                None,
                None,
                false,
                creation_flags,
                None,
                working_dir_ptr,
                &startup_info,
                &mut process_info,
            )?;
        }
        
        // Close thread handle (we don't need it)
        unsafe { CloseHandle(process_info.hThread).ok(); }
        
        Ok((process_info.hProcess, process_info.dwProcessId))
    }
    
    fn wait_for_process(
        &self,
        process_handle: HANDLE,
        timeout_ms: u32,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Result<Option<i32>> {
        // Wait with periodic shutdown checks
        let wait_timeout = 100u32; // Check every 100ms
        let mut elapsed = 0u32;
        
        loop {
            // Check shutdown flag
            if shutdown_flag.load(Ordering::Relaxed) {
                warn!("Shutdown requested during process wait");
                unsafe { TerminateProcess(process_handle, 1).ok(); }
                return Ok(None);
            }
            
            // Wait for process
            let result = unsafe { WaitForSingleObject(process_handle, wait_timeout) };
            
            match result.0 {
                0 => {
                    // Process completed
                    let mut exit_code = 0u32;
                    unsafe { GetExitCodeProcess(process_handle, &mut exit_code).ok(); }
                    return Ok(Some(exit_code as i32));
                }
                0x102 => {
                    // Timeout - continue waiting
                    elapsed += wait_timeout;
                    if elapsed >= timeout_ms {
                        warn!("Process timeout after {}ms", elapsed);
                        unsafe { TerminateProcess(process_handle, 1).ok(); }
                        return Err(anyhow!("Process timeout"));
                    }
                }
                _ => {
                    return Err(anyhow!("WaitForSingleObject failed: {:?}", result));
                }
            }
        }
    }
}

impl Default for ElevationHandler {
    fn default() -> Self {
        Self::new()
    }
}
