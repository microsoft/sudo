// Audit Logging to Windows Event Log
// Logs elevation requests, successes, and failures for security audit trail

use anyhow::{Context, Result};
use std::ptr;
use tracing::{debug, warn};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::{
    Foundation::HANDLE,
    System::EventLog::{
        DeregisterEventSource, RegisterEventSourceW, ReportEventW, EVENTLOG_INFORMATION_TYPE,
        EVENTLOG_ERROR_TYPE, EVENTLOG_WARNING_TYPE,
    },
};

use sudo::broker_protocol::{ElevationRequest, ElevationResponse};

/// Event IDs for audit logging
pub const EVENT_ELEVATION_REQUEST: u32 = 1001;
pub const EVENT_ELEVATION_SUCCESS: u32 = 1002;
pub const EVENT_ELEVATION_DENIED: u32 = 1003;
pub const EVENT_AUTHENTICATION_FAILED: u32 = 1004;
pub const EVENT_INTERNAL_ERROR: u32 = 1005;

/// Audit logger for security events
pub struct AuditLogger;

impl AuditLogger {
    /// Get event source handle
    fn get_event_source() -> Result<HANDLE> {
        let source_name: Vec<u16> = "SudoElevationBroker"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        
        let handle = unsafe { RegisterEventSourceW(None, PCWSTR(source_name.as_ptr()))? };
        
        Ok(handle)
    }
    
    /// Write event to Windows Event Log
    fn write_event(event_id: u32, event_type: u16, message: &str) -> Result<()> {
        let source = Self::get_event_source()?;
        
        let message_wide: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let messages = [PCWSTR(message_wide.as_ptr())];
        
        unsafe {
            ReportEventW(
                source,
                event_type,
                0,           // Category
                event_id,    // Event ID
                None,        // User SID
                &messages,   // Messages
                None,        // Raw data
            )?;
            
            DeregisterEventSource(source).ok();
        }
        
        Ok(())
    }
    
    /// Log elevation request
    pub fn log_elevation_request(request: &ElevationRequest) -> Result<()> {
        let message = format!(
            "Elevation request received\n\
             Command: {}\n\
             Arguments: {}\n\
             Execution Mode: {:?}\n\
             Working Directory: {}\n\
             Auth Token Length: {} bytes",
            request.command,
            request.arguments.join(" "),
            request.execution_mode,
            request.working_directory.as_deref().unwrap_or("(current)"),
            request.auth_token.len()
        );
        
        debug!("Audit: {}", message);
        
        if let Err(e) = Self::write_event(
            EVENT_ELEVATION_REQUEST,
            EVENTLOG_INFORMATION_TYPE.0 as u16,
            &message,
        ) {
            warn!("Failed to write audit log: {:#}", e);
        }
        
        Ok(())
    }
    
    /// Log successful elevation
    pub fn log_elevation_success(
        request: &ElevationRequest,
        response: &ElevationResponse,
    ) -> Result<()> {
        let message = format!(
            "Elevation succeeded\n\
             Command: {}\n\
             Process ID: {}\n\
             Exit Code: {}",
            request.command,
            response.process_id.unwrap_or(0),
            response.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "N/A".to_string())
        );
        
        debug!("Audit: {}", message);
        
        if let Err(e) = Self::write_event(
            EVENT_ELEVATION_SUCCESS,
            EVENTLOG_INFORMATION_TYPE.0 as u16,
            &message,
        ) {
            warn!("Failed to write audit log: {:#}", e);
        }
        
        Ok(())
    }
    
    /// Log elevation failure
    pub fn log_elevation_failure(
        request: &ElevationRequest,
        error: &anyhow::Error,
    ) -> Result<()> {
        let message = format!(
            "Elevation failed\n\
             Command: {}\n\
             Error: {:#}",
            request.command,
            error
        );
        
        debug!("Audit: {}", message);
        
        if let Err(e) = Self::write_event(
            EVENT_INTERNAL_ERROR,
            EVENTLOG_ERROR_TYPE.0 as u16,
            &message,
        ) {
            warn!("Failed to write audit log: {:#}", e);
        }
        
        Ok(())
    }
    
    /// Log authentication failure
    pub fn log_authentication_failure(request: &ElevationRequest, reason: &str) -> Result<()> {
        let message = format!(
            "Authentication failed\n\
             Command: {}\n\
             Reason: {}",
            request.command,
            reason
        );
        
        debug!("Audit: {}", message);
        
        if let Err(e) = Self::write_event(
            EVENT_AUTHENTICATION_FAILED,
            EVENTLOG_WARNING_TYPE.0 as u16,
            &message,
        ) {
            warn!("Failed to write audit log: {:#}", e);
        }
        
        Ok(())
    }
    
    /// Log access denied
    pub fn log_access_denied(request: &ElevationRequest, reason: &str) -> Result<()> {
        let message = format!(
            "Elevation denied\n\
             Command: {}\n\
             Reason: {}",
            request.command,
            reason
        );
        
        debug!("Audit: {}", message);
        
        if let Err(e) = Self::write_event(
            EVENT_ELEVATION_DENIED,
            EVENTLOG_WARNING_TYPE.0 as u16,
            &message,
        ) {
            warn!("Failed to write audit log: {:#}", e);
        }
        
        Ok(())
    }
}
