use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tracing::{debug, error, info, warn};
use windows::core::PCWSTR;
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, ERROR_PIPE_CONNECTED, ERROR_IO_PENDING, LUID},
    Security::{
        CheckTokenMembership, RevertToSelf,
        AllocateAndInitializeSid, FreeSid, PSID, SID_IDENTIFIER_AUTHORITY,
        SECURITY_NT_AUTHORITY, DOMAIN_ALIAS_RID_ADMINS, SECURITY_BUILTIN_DOMAIN_RID,
    },
    Storage::FileSystem::{ReadFile, WriteFile},
    System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, ImpersonateNamedPipeClient,
        PIPE_ACCESS_DUPLEX, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    },
    System::IO::OVERLAPPED,
};

use crate::audit_logger::AuditLogger;
use crate::broker_protocol::MAX_MESSAGE_SIZE;
use crate::elevation::ElevationHandler;

// Listens for client connections and handles elevation requests

/// RAII guard to ensure RevertToSelf is called
struct RevertGuard;

impl Drop for RevertGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = RevertToSelf();
        }
    }
}

/// RAII guard to ensure SID is freed
struct SidGuard(PSID);

impl Drop for SidGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = FreeSid(self.0);
            }
        }
    }
}

/// Named pipe server for handling elevation requests
pub struct PipeServer {
    pipe_name: String,
    max_concurrent: usize,
    shutdown_flag: Arc<AtomicBool>,
}

impl PipeServer {
    pub fn new(
        pipe_name: &str,
        max_concurrent: usize,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Result<Self> {
        Ok(Self {
            pipe_name: pipe_name.to_string(),
            max_concurrent,
            shutdown_flag,
        })
    }
    
    pub fn run(&self) -> Result<()> {
        info!("Pipe server starting on {}", self.pipe_name);
        
        let mut handles = Vec::new();
        
        // Main accept loop
        while !self.shutdown_flag.load(Ordering::Relaxed) {
            // Create pipe instance
            let pipe_handle = self.create_pipe_instance()?;
            
            debug!("Waiting for client connection...");
            
            // Wait for client connection
            match self.wait_for_connection(pipe_handle) {
                Ok(connected_pipe) => {
                    info!("Client connected");
                    
                    // Spawn handler thread
                    let shutdown = self.shutdown_flag.clone();
                    let handle = thread::spawn(move || {
                        if let Err(e) = Self::handle_client(connected_pipe, shutdown) {
                            error!("Client handler error: {:#}", e);
                        }
                    });
                    
                    handles.push(handle);
                    
                    // Clean up finished threads
                    handles.retain(|h| !h.is_finished());
                    
                    // Check if we've hit max concurrent
                    if handles.len() >= self.max_concurrent {
                        warn!("Max concurrent connections reached ({}), throttling", self.max_concurrent);
                        thread::sleep(Duration::from_millis(100));
                    }
                }
                Err(e) => {
                    // Only log if not shutting down
                    if !self.shutdown_flag.load(Ordering::Relaxed) {
                        error!("Connection error: {:#}", e);
                    }
                    unsafe { CloseHandle(pipe_handle).ok(); }
                }
            }
        }
        
        info!("Pipe server shutting down - waiting for {} active connections", handles.len());
        
        // Wait for all handlers to finish
        for handle in handles {
            let _ = handle.join();
        }
        
        info!("Pipe server stopped");
        Ok(())
    }
    
    fn create_pipe_instance(&self) -> Result<HANDLE> {
        let pipe_name_wide: Vec<u16> = self.pipe_name.encode_utf16().chain(Some(0)).collect();
        
        // Create security descriptor that allows only SYSTEM and Administrators
        let security_attrs = Self::create_pipe_security_attributes()?;
        
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(pipe_name_wide.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                65536, // Out buffer size
                65536, // In buffer size
                0,     // Default timeout
                Some(&security_attrs as *const _ as *const _),
            )
        }?;
        
        Ok(handle)
    }
    
    /// Create security attributes for the named pipe
    /// 
    /// Creates a security descriptor that allows access only to:
    /// - SYSTEM (LocalSystem account - the broker service itself)
    /// - Administrators (Built-in Administrators group)
    /// 
    /// SDDL Format: D:(A;;GA;;;SY)(A;;GA;;;BA)
    /// - D: = DACL
    /// - (A;;GA;;;SY) = Allow Generic All to SYSTEM
    /// - (A;;GA;;;BA) = Allow Generic All to Built-in Administrators
    fn create_pipe_security_attributes() -> Result<windows::Win32::Security::SECURITY_ATTRIBUTES> {
        use windows::Win32::Security::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SECURITY_ATTRIBUTES, PSECURITY_DESCRIPTOR,
            SDDL_REVISION_1,
        };
        
        // SDDL string: Allow SYSTEM and Administrators full access
        let sddl = "D:(A;;GA;;;SY)(A;;GA;;;BA)";
        let sddl_wide: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        
        let mut security_descriptor: PSECURITY_DESCRIPTOR = PSECURITY_DESCRIPTOR::default();
        let mut _size = 0u32;
        
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                windows::core::PCWSTR(sddl_wide.as_ptr()),
                SDDL_REVISION_1,
                &mut security_descriptor as *mut _,
                Some(&mut _size as *mut _),
            ).context("Failed to convert SDDL to security descriptor")?;
        }
        
        let security_attrs = windows::Win32::Security::SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<windows::Win32::Security::SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security_descriptor.0,
            bInheritHandle: windows::Win32::Foundation::BOOL::from(false),
        };
        
        debug!("Created pipe security descriptor: SYSTEM and Administrators only");
        Ok(security_attrs)
    }
    
    fn wait_for_connection(&self, pipe_handle: HANDLE) -> Result<HANDLE> {
        unsafe {
            let result = ConnectNamedPipe(pipe_handle, None);
            
            if result.is_ok() || result.unwrap_err().code() == ERROR_PIPE_CONNECTED.into() {
                Ok(pipe_handle)
            } else {
                CloseHandle(pipe_handle).ok();
                Err(anyhow!("Failed to connect pipe: {:?}", result.unwrap_err()))
            }
        }
    }
    
    fn handle_client(pipe_handle: HANDLE, shutdown_flag: Arc<AtomicBool>) -> Result<()> {
        debug!("Handling client connection");
        
        // CRITICAL SECURITY: Verify client identity and permissions
        // This is the primary security mechanism for the broker service
        Self::verify_client_identity(pipe_handle)?;
        
        // Receive elevation request
        let request_bytes = Self::receive_message(pipe_handle)?;
        
        // Deserialize request
        let request: sudo::broker_protocol::ElevationRequest = 
            bincode::deserialize(&request_bytes)
                .context("Failed to deserialize elevation request")?;
        
        debug!("Received elevation request for command: {}", request.command);
        
        // Validate protocol version
        if request.version != sudo::broker_protocol::PROTOCOL_VERSION {
            let response = sudo::broker_protocol::ElevationResponse {
                status: sudo::broker_protocol::StatusCode::ProtocolVersionMismatch,
                error_message: Some(format!(
                    "Protocol version mismatch: client={}, server={}",
                    request.version,
                    sudo::broker_protocol::PROTOCOL_VERSION
                )),
                process_id: None,
                exit_code: None,
                is_running: false,
            };
            
            let response_bytes = bincode::serialize(&response)?;
            Self::send_message(pipe_handle, &response_bytes)?;
            
            unsafe { DisconnectNamedPipe(pipe_handle).ok(); }
            unsafe { CloseHandle(pipe_handle).ok(); }
            
            return Ok(());
        }
        
        // Audit log the request
        AuditLogger::log_elevation_request(&request)?;
        
        // Handle elevation
        let elevation_handler = ElevationHandler::new();
        let response = match elevation_handler.elevate(request.clone(), shutdown_flag) {
            Ok(resp) => {
                AuditLogger::log_elevation_success(&request, &resp)?;
                resp
            }
            Err(e) => {
                error!("Elevation failed: {:#}", e);
                let response = sudo::broker_protocol::ElevationResponse {
                    status: sudo::broker_protocol::StatusCode::InternalError,
                    error_message: Some(format!("{:#}", e)),
                    process_id: None,
                    exit_code: None,
                    is_running: false,
                };
                AuditLogger::log_elevation_failure(&request, &e)?;
                response
            }
        };
        
        // Send response
        let response_bytes = bincode::serialize(&response)?;
        Self::send_message(pipe_handle, &response_bytes)?;
        
        // Disconnect and close
        unsafe {
            DisconnectNamedPipe(pipe_handle).ok();
            CloseHandle(pipe_handle).ok();
        }
        
        debug!("Client handler completed");
        Ok(())
    }
    
    fn receive_message(pipe_handle: HANDLE) -> Result<Vec<u8>> {
        // Read 4-byte length prefix
        let mut length_buf = [0u8; 4];
        let mut bytes_read = 0u32;
        
        unsafe {
            ReadFile(
                pipe_handle,
                Some(&mut length_buf),
                Some(&mut bytes_read),
                None,
            )?;
        }
        
        if bytes_read != 4 {
            return Err(anyhow!("Failed to read message length (got {} bytes)", bytes_read));
        }
        
        let length = u32::from_le_bytes(length_buf) as usize;

        // Validate message size
        if length > MAX_MESSAGE_SIZE {
            return Err(anyhow!("Message too large: {} bytes", length));
        }
        
        // Read message data
        let mut data = vec![0u8; length];
        let mut total_read = 0;
        
        while total_read < length {
            let mut bytes_read = 0u32;
            unsafe {
                ReadFile(
                    pipe_handle,
                    Some(&mut data[total_read..]),
                    Some(&mut bytes_read),
                    None,
                )?;
            }
            total_read += bytes_read as usize;
        }
        
        Ok(data)
    }
    
    /// Verify client identity and permissions
    /// 
    /// This is the PRIMARY SECURITY MECHANISM for the broker service.
    /// 
    /// Security model:
    /// 1. Named pipe security (DACL) allows only SYSTEM and Administrators to connect
    /// 2. Service impersonates the connected client to get their identity
    /// 3. Service verifies client is a member of the Administrators group
    /// 4. If verification passes, service reverts to LocalSystem and processes request
    /// 
    /// The Windows Hello authentication in the client is for USER CONSENT only.
    /// The broker service performs the actual authorization check here.
    fn verify_client_identity(pipe_handle: HANDLE) -> Result<()> {
        debug!("Verifying client identity and permissions");
        
        unsafe {
            // Step 1: Impersonate the client
            // This causes the current thread to take on the security context of the client
            ImpersonateNamedPipeClient(pipe_handle)
                .context("Failed to impersonate named pipe client")?;
            
            // Ensure we revert even if verification fails
            let revert_guard = RevertGuard;
            
            // Step 2: Create SID for BUILTIN\Administrators group
            let mut admin_sid: PSID = PSID::default();
            let mut nt_authority = SID_IDENTIFIER_AUTHORITY {
                Value: [0, 0, 0, 0, 0, 5], // SECURITY_NT_AUTHORITY
            };
            
            let result = AllocateAndInitializeSid(
                &mut nt_authority as *mut _,
                2, // 2 sub-authorities
                SECURITY_BUILTIN_DOMAIN_RID as u32,
                DOMAIN_ALIAS_RID_ADMINS as u32,
                0, 0, 0, 0, 0, 0,
                &mut admin_sid as *mut _,
            );
            
            if result.is_err() {
                error!("Failed to allocate Administrators SID");
                return Err(anyhow!("Failed to allocate Administrators SID: {:?}", result.unwrap_err()));
            }
            
            // Ensure SID is freed
            let sid_guard = SidGuard(admin_sid);
            
            // Step 3: Check if the impersonated client is a member of Administrators group
            let mut is_member = windows::Win32::Foundation::BOOL::from(false);
            let check_result = CheckTokenMembership(
                HANDLE::default(), // Use current thread token (we're impersonating)
                admin_sid,
                &mut is_member as *mut _,
            );
            
            if check_result.is_err() {
                error!("Failed to check token membership");
                return Err(anyhow!("Failed to check token membership: {:?}", check_result.unwrap_err()));
            }
            
            // Step 4: Revert to LocalSystem (automatic via revert_guard drop)
            drop(revert_guard);
            drop(sid_guard);
            
            // Step 5: Verify the client is an administrator
            if !is_member.as_bool() {
                error!("Client is not a member of Administrators group - access denied");
                return Err(anyhow!(
                    "Access denied: Client must be a member of the Administrators group"
                ));
            }
            
            info!("Client identity verified: member of Administrators group");
            Ok(())
        }
    }
    
    fn send_message(pipe_handle: HANDLE, data: &[u8]) -> Result<()> {
        // Send 4-byte length prefix
        let length = data.len() as u32;
        let length_bytes = length.to_le_bytes();
        
        let mut bytes_written = 0u32;
        unsafe {
            WriteFile(
                pipe_handle,
                Some(&length_bytes),
                Some(&mut bytes_written),
                None,
            )?;
        }
        
        if bytes_written != 4 {
            return Err(anyhow!("Failed to write message length"));
        }
        
        // Send message data
        let mut total_written = 0;
        
        while total_written < data.len() {
            let mut bytes_written = 0u32;
            unsafe {
                WriteFile(
                    pipe_handle,
                    Some(&data[total_written..]),
                    Some(&mut bytes_written),
                    None,
                )?;
            }
            total_written += bytes_written as usize;
        }
        
        Ok(())
    }
}
