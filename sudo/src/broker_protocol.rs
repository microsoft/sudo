// Elevation Broker Protocol Definition
//
// This module defines the communication protocol between sudo.exe (client)
// and the SudoElevationBroker service (server) via named pipes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Protocol version for compatibility checking
pub const PROTOCOL_VERSION: u32 = 1;

/// Named pipe path for broker communication
pub const BROKER_PIPE_NAME: &str = r"\\.\pipe\SudoElevationBroker";

/// Maximum message size (16MB)
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Timeout for broker operations (30 seconds)
pub const DEFAULT_TIMEOUT_MS: u32 = 30_000;

// ============================================================================
// Request/Response Messages
// ============================================================================

/// Request to elevate and execute a process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevationRequest {
    /// Protocol version
    pub version: u32,
    
    /// Authentication token (from Windows Hello or other auth mechanism)
    #[serde(with = "serde_bytes")]
    pub auth_token: Vec<u8>,
    
    /// Executable path to run
    pub command: String,
    
    /// Command-line arguments
    pub arguments: Vec<String>,
    
    /// Working directory (None = inherit from client)
    pub working_directory: Option<String>,
    
    /// Environment variables (only overrides/additions)
    pub environment: HashMap<String, String>,
    
    /// Execution mode
    pub execution_mode: ExecutionMode,
    
    /// Timeout in milliseconds (0 = no timeout)
    pub timeout_ms: u32,
    
    /// Whether to capture stdout
    pub capture_stdout: bool,
    
    /// Whether to capture stderr
    pub capture_stderr: bool,
}

/// Response to an elevation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevationResponse {
    /// Request status
    pub status: StatusCode,
    
    /// Error message if status is not Success
    pub error_message: Option<String>,
    
    /// Process ID of the elevated process (if successful)
    pub process_id: Option<u32>,
    
    /// Exit code of the process (if completed)
    pub exit_code: Option<i32>,
    
    /// Whether the process is still running
    pub is_running: bool,
}

/// Streaming output chunk from the elevated process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputChunk {
    /// Which output stream this data is from
    pub stream: OutputStreamType,
    
    /// Output data
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
    
    /// Whether this is the final chunk for this stream
    pub is_final: bool,
    
    /// Sequence number for ordering
    pub sequence: u64,
}

// ============================================================================
// Enums
// ============================================================================

/// Execution modes for the elevated process
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Run inline - capture output and return when complete
    Inline,
    
    /// Run in new window - create new console window
    NewWindow,
    
    /// Run in current window - attach to current console
    CurrentWindow,
    
    /// Disable input - prevent interactive input
    DisableInput,
    
    /// Hidden - run without visible window
    Hidden,
}

impl ExecutionMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "inline" | "i" => Some(Self::Inline),
            "new" | "newwindow" | "n" => Some(Self::NewWindow),
            "current" | "currentwindow" | "c" => Some(Self::CurrentWindow),
            "disableinput" | "d" => Some(Self::DisableInput),
            "hidden" | "h" => Some(Self::Hidden),
            _ => None,
        }
    }
}

/// Status codes for elevation requests
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum StatusCode {
    /// Operation completed successfully
    Success = 0,
    
    /// Authentication failed
    AuthenticationFailed = 1,
    
    /// User denied the elevation request
    UserDenied = 2,
    
    /// The requested command was not found
    CommandNotFound = 3,
    
    /// Access denied (insufficient privileges)
    AccessDenied = 4,
    
    /// Timeout waiting for process completion
    Timeout = 5,
    
    /// Invalid request parameters
    InvalidRequest = 6,
    
    /// Broker service internal error
    InternalError = 7,
    
    /// Unsupported protocol version
    UnsupportedVersion = 8,
    
    /// Windows Hello not available
    HelloNotAvailable = 9,
    
    /// Administrator Protection not enabled
    APNotEnabled = 10,
    
    /// General error
    /// 999 is used as a catch-all error code, distinct from specific errors above.
    /// This ensures it does not overlap with any future specific error codes.
    Error = 999,
}

impl StatusCode {
    pub fn to_error_message(&self) -> &'static str {
        match self {
            Self::Success => "Success",
            Self::AuthenticationFailed => "Authentication failed",
            Self::UserDenied => "User denied the elevation request",
            Self::CommandNotFound => "Command not found",
            Self::AccessDenied => "Access denied",
            Self::Timeout => "Operation timed out",
            Self::InvalidRequest => "Invalid request parameters",
            Self::InternalError => "Internal broker service error",
            Self::UnsupportedVersion => "Unsupported protocol version",
            Self::HelloNotAvailable => "Windows Hello is not available",
            Self::APNotEnabled => "Administrator Protection is not enabled",
            Self::Error => "An error occurred",
        }
    }
    
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }
}

/// Output stream type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputStreamType {
    /// Standard output
    Stdout,
    
    /// Standard error
    Stderr,
}

// ============================================================================
// Helper Functions
// ============================================================================

impl ElevationRequest {
    /// Create a new elevation request with defaults
    pub fn new(command: String, arguments: Vec<String>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            auth_token: Vec::new(),
            command,
            arguments,
            working_directory: None,
            environment: HashMap::new(),
            execution_mode: ExecutionMode::Inline,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            capture_stdout: true,
            capture_stderr: true,
        }
    }
    
    /// Validate the request
    pub fn validate(&self) -> Result<(), String> {
        if self.version != PROTOCOL_VERSION {
            return Err(format!(
                "Unsupported protocol version: {} (expected {})",
                self.version, PROTOCOL_VERSION
            ));
        }
        
        if self.command.is_empty() {
            return Err("Command cannot be empty".to_string());
        }
        
        if self.timeout_ms > 0 && self.timeout_ms < 1000 {
            return Err("Timeout must be at least 1000ms or 0 for no timeout".to_string());
        }
        
        Ok(())
    }
    
    /// Serialize to bytes for transmission
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        bincode::serialize(self)
            .map_err(|e| format!("Failed to serialize request: {}", e))
    }
    
    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() > MAX_MESSAGE_SIZE {
            return Err(format!(
                "Message too large: {} bytes (max {})",
                data.len(),
                MAX_MESSAGE_SIZE
            ));
        }
        
        bincode::deserialize(data)
            .map_err(|e| format!("Failed to deserialize request: {}", e))
    }
}

impl ElevationResponse {
    /// Create a success response
    pub fn success(process_id: u32) -> Self {
        Self {
            status: StatusCode::Success,
            error_message: None,
            process_id: Some(process_id),
            exit_code: None,
            is_running: true,
        }
    }
    
    /// Create an error response
    pub fn error(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            error_message: Some(message.into()),
            process_id: None,
            exit_code: None,
            is_running: false,
        }
    }
    
    /// Create a completed response
    pub fn completed(process_id: u32, exit_code: i32) -> Self {
        Self {
            status: StatusCode::Success,
            error_message: None,
            process_id: Some(process_id),
            exit_code: Some(exit_code),
            is_running: false,
        }
    }
    
    /// Serialize to bytes for transmission
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        bincode::serialize(self)
            .map_err(|e| format!("Failed to serialize response: {}", e))
    }
    
    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() > MAX_MESSAGE_SIZE {
            return Err(format!(
                "Message too large: {} bytes (max {})",
                data.len(),
                MAX_MESSAGE_SIZE
            ));
        }
        
        bincode::deserialize(data)
            .map_err(|e| format!("Failed to deserialize response: {}", e))
    }
}

impl OutputChunk {
    /// Create a new output chunk
    pub fn new(stream: OutputStreamType, data: Vec<u8>, sequence: u64, is_final: bool) -> Self {
        Self {
            stream,
            data,
            is_final,
            sequence,
        }
    }
    
    /// Serialize to bytes for transmission
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        bincode::serialize(self)
            .map_err(|e| format!("Failed to serialize output chunk: {}", e))
    }
    
    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() > MAX_MESSAGE_SIZE {
            return Err(format!(
                "Message too large: {} bytes (max {})",
                data.len(),
                MAX_MESSAGE_SIZE
            ));
        }
        
        bincode::deserialize(data)
            .map_err(|e| format!("Failed to deserialize output chunk: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_elevation_request_serialization() {
        let mut request = ElevationRequest::new(
            "notepad.exe".to_string(),
            vec!["test.txt".to_string()],
        );
        request.working_directory = Some("C:\\Users\\Test".to_string());
        request.environment.insert("TEST_VAR".to_string(), "value".to_string());
        
        let bytes = request.to_bytes().unwrap();
        let decoded = ElevationRequest::from_bytes(&bytes).unwrap();
        
        assert_eq!(request.command, decoded.command);
        assert_eq!(request.arguments, decoded.arguments);
        assert_eq!(request.working_directory, decoded.working_directory);
    }
    
    #[test]
    fn test_execution_mode_parsing() {
        assert_eq!(ExecutionMode::from_str("inline"), Some(ExecutionMode::Inline));
        assert_eq!(ExecutionMode::from_str("NEW"), Some(ExecutionMode::NewWindow));
        assert_eq!(ExecutionMode::from_str("hidden"), Some(ExecutionMode::Hidden));
        assert_eq!(ExecutionMode::from_str("invalid"), None);
    }
    
    #[test]
    fn test_status_code_messages() {
        assert_eq!(StatusCode::Success.to_error_message(), "Success");
        assert!(StatusCode::Success.is_success());
        assert!(!StatusCode::AccessDenied.is_success());
    }
    
    #[test]
    fn test_protocol_version_validation() {
        let mut request = ElevationRequest::new(
            "cmd.exe".to_string(),
            vec![],
        );
        
        // Valid version
        request.version = PROTOCOL_VERSION;
        assert!(request.validate().is_ok());
        
        // Invalid version
        request.version = 999;
        let result = request.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unsupported protocol version"));
    }
    
    #[test]
    fn test_empty_command_validation() {
        let mut request = ElevationRequest::new(
            "".to_string(),
            vec![],
        );
        
        let result = request.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Command cannot be empty"));
    }
    
    #[test]
    fn test_timeout_validation() {
        let mut request = ElevationRequest::new(
            "cmd.exe".to_string(),
            vec![],
        );
        
        // Valid timeouts
        request.timeout_ms = 0; // No timeout
        assert!(request.validate().is_ok());
        
        request.timeout_ms = 1000; // Minimum valid
        assert!(request.validate().is_ok());
        
        request.timeout_ms = 60_000; // Normal timeout
        assert!(request.validate().is_ok());
        
        // Invalid timeout (too small)
        request.timeout_ms = 500;
        let result = request.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least 1000ms"));
    }
    
    #[test]
    fn test_message_size_limit() {
        // Create a large message that exceeds MAX_MESSAGE_SIZE
        let oversized_data = vec![0u8; MAX_MESSAGE_SIZE + 1];
        
        let result = ElevationRequest::from_bytes(&oversized_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Message too large"));
    }
    
    #[test]
    fn test_invalid_serialization_data() {
        // Invalid bincode data
        let invalid_data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        
        let result = ElevationRequest::from_bytes(&invalid_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to deserialize"));
    }
    
    #[test]
    fn test_elevation_response_success() {
        let response = ElevationResponse::success(1234);
        
        assert_eq!(response.status, StatusCode::Success);
        assert!(response.error_message.is_none());
        assert_eq!(response.process_id, Some(1234));
        assert!(response.is_running);
    }
    
    #[test]
    fn test_elevation_response_error() {
        let response = ElevationResponse::error(
            StatusCode::AccessDenied,
            "Permission denied"
        );
        
        assert_eq!(response.status, StatusCode::AccessDenied);
        assert_eq!(response.error_message, Some("Permission denied".to_string()));
        assert!(response.process_id.is_none());
        assert!(!response.is_running);
    }
    
    #[test]
    fn test_output_chunk_serialization() {
        let chunk = OutputChunk {
            stream: OutputStreamType::Stdout,
            data: b"test output".to_vec(),
            is_final: false,
            sequence: 42,
        };
        
        let bytes = bincode::serialize(&chunk).unwrap();
        let decoded: OutputChunk = bincode::deserialize(&bytes).unwrap();
        
        assert_eq!(chunk.data, decoded.data);
        assert_eq!(chunk.sequence, decoded.sequence);
        assert_eq!(chunk.is_final, decoded.is_final);
    }
    
    #[test]
    fn test_large_environment_variables() {
        let mut request = ElevationRequest::new(
            "cmd.exe".to_string(),
            vec![],
        );
        
        // Add many environment variables
        for i in 0..100 {
            request.environment.insert(
                format!("VAR_{}", i),
                format!("VALUE_{}", i)
            );
        }
        
        let bytes = request.to_bytes().unwrap();
        assert!(bytes.len() < MAX_MESSAGE_SIZE);
        
        let decoded = ElevationRequest::from_bytes(&bytes).unwrap();
        assert_eq!(request.environment.len(), decoded.environment.len());
        assert_eq!(request.environment.get("VAR_0"), decoded.environment.get("VAR_0"));
    }
    
    #[test]
    fn test_large_arguments_list() {
        let mut arguments = Vec::new();
        for i in 0..1000 {
            arguments.push(format!("arg_{}", i));
        }
        
        let request = ElevationRequest::new(
            "cmd.exe".to_string(),
            arguments.clone(),
        );
        
        let bytes = request.to_bytes().unwrap();
        let decoded = ElevationRequest::from_bytes(&bytes).unwrap();
        
        assert_eq!(decoded.arguments.len(), 1000);
        assert_eq!(decoded.arguments[0], "arg_0");
        assert_eq!(decoded.arguments[999], "arg_999");
    }
    
    #[test]
    fn test_auth_token_handling() {
        let mut request = ElevationRequest::new(
            "cmd.exe".to_string(),
            vec![],
        );
        
        // Add auth token (simulate HMAC-signed token)
        let token = vec![
            1, 0, 0, 0,  // version
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,  // timestamp
            0xAA, 0xBB, 0xCC, 0xDD,  // lengths
        ];
        request.auth_token = token.clone();
        
        let bytes = request.to_bytes().unwrap();
        let decoded = ElevationRequest::from_bytes(&bytes).unwrap();
        
        assert_eq!(decoded.auth_token, token);
    }
    
    #[test]
    fn test_all_execution_modes() {
        let modes = vec![
            ExecutionMode::Inline,
            ExecutionMode::NewWindow,
            ExecutionMode::CurrentWindow,
            ExecutionMode::DisableInput,
            ExecutionMode::Hidden,
        ];
        
        for mode in modes {
            let mut request = ElevationRequest::new(
                "cmd.exe".to_string(),
                vec![],
            );
            request.execution_mode = mode;
            
            let bytes = request.to_bytes().unwrap();
            let decoded = ElevationRequest::from_bytes(&bytes).unwrap();
            
            assert_eq!(decoded.execution_mode, mode);
        }
    }
    
    #[test]
    fn test_output_stream_types() {
        let stdout_chunk = OutputChunk {
            stream: OutputStreamType::Stdout,
            data: vec![],
            is_final: false,
            sequence: 0,
        };
        
        let stderr_chunk = OutputChunk {
            stream: OutputStreamType::Stderr,
            data: vec![],
            is_final: false,
            sequence: 0,
        };
        
        let stdout_bytes = bincode::serialize(&stdout_chunk).unwrap();
        let stderr_bytes = bincode::serialize(&stderr_chunk).unwrap();
        
        let decoded_stdout: OutputChunk = bincode::deserialize(&stdout_bytes).unwrap();
        let decoded_stderr: OutputChunk = bincode::deserialize(&stderr_bytes).unwrap();
        
        assert!(matches!(decoded_stdout.stream, OutputStreamType::Stdout));
        assert!(matches!(decoded_stderr.stream, OutputStreamType::Stderr));
    }
    
    #[test]
    fn test_unicode_in_command_and_args() {
        let request = ElevationRequest::new(
            "cmd.exe".to_string(),
            vec![
                "test_日本語.txt".to_string(),
                "тест_кириллица.txt".to_string(),
                "emoji_🚀.txt".to_string(),
            ],
        );
        
        let bytes = request.to_bytes().unwrap();
        let decoded = ElevationRequest::from_bytes(&bytes).unwrap();
        
        assert_eq!(decoded.arguments[0], "test_日本語.txt");
        assert_eq!(decoded.arguments[1], "тест_кириллица.txt");
        assert_eq!(decoded.arguments[2], "emoji_🚀.txt");
    }
    
    #[test]
    fn test_working_directory_none_vs_some() {
        let mut request1 = ElevationRequest::new("cmd.exe".to_string(), vec![]);
        request1.working_directory = None;
        
        let mut request2 = ElevationRequest::new("cmd.exe".to_string(), vec![]);
        request2.working_directory = Some("C:\\Test".to_string());
        
        let bytes1 = request1.to_bytes().unwrap();
        let bytes2 = request2.to_bytes().unwrap();
        
        let decoded1 = ElevationRequest::from_bytes(&bytes1).unwrap();
        let decoded2 = ElevationRequest::from_bytes(&bytes2).unwrap();
        
        assert!(decoded1.working_directory.is_none());
        assert_eq!(decoded2.working_directory, Some("C:\\Test".to_string()));
    }
}
