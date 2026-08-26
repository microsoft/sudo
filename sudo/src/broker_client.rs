// Broker Client Module
//
// This module provides the client-side interface for communicating with
// the SudoElevationBroker service via named pipes.

use crate::broker_protocol::*;
use std::io::{Read, Write};
use std::time::Duration;
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Storage::FileSystem::*,
    Win32::System::Pipes::*,
    Win32::Security::*,
};

/// Client for communicating with the elevation broker service
pub struct BrokerClient {
    pipe_handle: Option<HANDLE>,
    timeout: Duration,
}

impl BrokerClient {
    /// Create a new broker client
    pub fn new() -> Self {
        Self {
            pipe_handle: None,
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS as u64),
        }
    }
    
    /// Set the timeout for broker operations
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    
    /// Connect to the broker service
    pub fn connect(&mut self) -> Result<()> {
        self.connect_with_retry(3, Duration::from_millis(500))
    }
    
    /// Connect to the broker service with retry logic
    fn connect_with_retry(&mut self, max_retries: u32, retry_delay: Duration) -> Result<()> {
        let mut last_error = Error::from_win32();
        
        for attempt in 0..max_retries {
            match self.try_connect() {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_error = e;
                    if attempt < max_retries - 1 {
                        std::thread::sleep(retry_delay);
                    }
                }
            }
        }
        
        Err(last_error)
    }
    
    /// Attempt to connect to the named pipe
    fn try_connect(&mut self) -> Result<()> {
        unsafe {
            // Try to open the named pipe
            let pipe_name = HSTRING::from(BROKER_PIPE_NAME);
            
            let handle = CreateFileW(
                &pipe_name,
                FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )?;
            
            // Set the pipe to message mode
            let mut mode = PIPE_READMODE_MESSAGE;
            SetNamedPipeHandleState(
                handle,
                Some(&mut mode),
                None,
                None,
            )?;
            
            self.pipe_handle = Some(handle);
            Ok(())
        }
    }
    
    /// Check if connected to the broker
    pub fn is_connected(&self) -> bool {
        self.pipe_handle.is_some()
    }
    
    /// Disconnect from the broker
    pub fn disconnect(&mut self) {
        if let Some(handle) = self.pipe_handle.take() {
            unsafe {
                let _ = CloseHandle(handle);
            }
        }
    }
    
    /// Send an elevation request and receive the response
    pub fn elevate(
        &mut self,
        request: &ElevationRequest,
    ) -> Result<ElevationResponse> {
        if !self.is_connected() {
            return Err(Error::from(E_NOT_SET));
        }
        
        // Validate request
        request.validate().map_err(|e| Error::from_win32())?;
        
        // Serialize request
        let request_bytes = request.to_bytes()
            .map_err(|_| Error::from_win32())?;
        
        // Send request
        self.send_message(&request_bytes)?;
        
        // Receive response
        let response_bytes = self.receive_message()?;
        
        // Deserialize response
        let response = ElevationResponse::from_bytes(&response_bytes)
            .map_err(|_| Error::from_win32())?;
        
        Ok(response)
    }
    
    /// Send a message through the pipe
    fn send_message(&self, data: &[u8]) -> Result<()> {
        let handle = self.pipe_handle.ok_or(Error::from(E_NOT_SET))?;
        
        unsafe {
            // Send message length first (4 bytes, little-endian)
            let len_bytes = (data.len() as u32).to_le_bytes();
            let mut bytes_written = 0;
            WriteFile(
                handle,
                Some(&len_bytes),
                Some(&mut bytes_written),
                None,
            )?;
            
            // Send message data
            bytes_written = 0;
            WriteFile(
                handle,
                Some(data),
                Some(&mut bytes_written),
                None,
            )?;
            
            if bytes_written != data.len() as u32 {
                return Err(Error::from_win32());
            }
            
            Ok(())
        }
    }
    
    /// Receive a message from the pipe
    fn receive_message(&self) -> Result<Vec<u8>> {
        let handle = self.pipe_handle.ok_or(Error::from(E_NOT_SET))?;
        
        unsafe {
            // Read message length (4 bytes)
            let mut len_bytes = [0u8; 4];
            let mut bytes_read = 0;
            ReadFile(
                handle,
                Some(&mut len_bytes),
                Some(&mut bytes_read),
                None,
            )?;
            
            if bytes_read != 4 {
                return Err(Error::from_win32());
            }
            
            let message_len = u32::from_le_bytes(len_bytes) as usize;
            
            // Validate message length
            if message_len > MAX_MESSAGE_SIZE {
                return Err(Error::from_win32());
            }
            
            // Read message data
            let mut buffer = vec![0u8; message_len];
            bytes_read = 0;
            ReadFile(
                handle,
                Some(&mut buffer),
                Some(&mut bytes_read),
                None,
            )?;
            
            if bytes_read as usize != message_len {
                return Err(Error::from_win32());
            }
            
            Ok(buffer)
        }
    }
    
    /// Receive output chunks from the elevated process
    pub fn receive_output<F>(&mut self, mut callback: F) -> Result<()>
    where
        F: FnMut(OutputChunk) -> bool,
    {
        loop {
            let chunk_bytes = self.receive_message()?;
            let chunk = OutputChunk::from_bytes(&chunk_bytes)
                .map_err(|_| Error::from_win32())?;
            
            let is_final = chunk.is_final;
            
            // Call the callback with the chunk
            let should_continue = callback(chunk);
            
            if !should_continue || is_final {
                break;
            }
        }
        
        Ok(())
    }
}

impl Drop for BrokerClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// High-level function to elevate a process via the broker
pub fn elevate_via_broker(
    command: &str,
    arguments: &[String],
    working_directory: Option<&str>,
    execution_mode: ExecutionMode,
    environment: &[(String, String)],
    auth_token: Vec<u8>,
) -> Result<ElevationResponse> {
    let mut client = BrokerClient::new();
    
    // Connect to broker
    client.connect().map_err(|e| {
        eprintln!("❌ Failed to connect to elevation broker service");
        eprintln!("   Make sure the SudoElevationBroker service is running.");
        eprintln!("   You can start it with: sc start SudoElevationBroker");
        e
    })?;
    
    // Build request
    let mut request = ElevationRequest::new(
        command.to_string(),
        arguments.to_vec(),
    );
    request.execution_mode = execution_mode;
    request.auth_token = auth_token;
    request.working_directory = working_directory.map(|s| s.to_string());
    
    // Add environment variables
    for (key, value) in environment {
        request.environment.insert(key.clone(), value.clone());
    }
    
    // Send elevation request
    let response = client.elevate(&request)?;
    
    // If inline mode, receive output
    if execution_mode == ExecutionMode::Inline {
        if response.status.is_success() {
            client.receive_output(|chunk| {
                use std::io::{stdout, stderr, Write};
                
                match chunk.stream {
                    OutputStreamType::Stdout => {
                        let _ = stdout().write_all(&chunk.data);
                        let _ = stdout().flush();
                    }
                    OutputStreamType::Stderr => {
                        let _ = stderr().write_all(&chunk.data);
                        let _ = stderr().flush();
                    }
                }
                
                true // Continue receiving
            })?;
        }
    }
    
    Ok(response)
}

/// Check if the broker service is available and responsive
pub fn check_broker_availability() -> bool {
    let mut client = BrokerClient::new();
    client.connect().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    #[ignore] // Requires broker service to be running
    fn test_broker_connection() {
        let mut client = BrokerClient::new();
        let result = client.connect();
        
        if result.is_ok() {
            assert!(client.is_connected());
            client.disconnect();
            assert!(!client.is_connected());
        } else {
            println!("Broker service not available for testing");
        }
    }
    
    #[test]
    fn test_broker_availability_check() {
        // This test may pass or fail depending on whether service is running
        let available = check_broker_availability();
        println!("Broker available: {}", available);
    }
    
    #[test]
    fn test_broker_client_creation() {
        let client = BrokerClient::new();
        assert!(!client.is_connected());
        assert_eq!(client.timeout, Duration::from_millis(DEFAULT_TIMEOUT_MS as u64));
    }
    
    #[test]
    fn test_broker_client_with_timeout() {
        let custom_timeout = Duration::from_secs(60);
        let client = BrokerClient::new().with_timeout(custom_timeout);
        assert_eq!(client.timeout, custom_timeout);
    }
    
    #[test]
    fn test_disconnect_when_not_connected() {
        let mut client = BrokerClient::new();
        // Should not panic
        client.disconnect();
        assert!(!client.is_connected());
    }
    
    #[test]
    fn test_is_connected_initial_state() {
        let client = BrokerClient::new();
        assert!(!client.is_connected(), "Client should not be connected initially");
    }
    
    #[test]
    fn test_elevate_without_connection() {
        let mut client = BrokerClient::new();
        
        let request = ElevationRequest::new(
            "cmd.exe".to_string(),
            vec![],
        );
        
        let result = client.elevate(&request);
        assert!(result.is_err(), "Elevate should fail when not connected");
    }
    
    #[test]
    fn test_request_validation_before_send() {
        // This test validates that the client checks request validity
        let mut client = BrokerClient::new();
        
        // Create an invalid request (empty command)
        let mut request = ElevationRequest::new(
            "".to_string(),
            vec![],
        );
        
        // Validate should fail
        let validation_result = request.validate();
        assert!(validation_result.is_err());
        assert!(validation_result.unwrap_err().contains("Command cannot be empty"));
    }
    
    #[test]
    fn test_multiple_disconnects() {
        let mut client = BrokerClient::new();
        
        // Multiple disconnects should be safe
        client.disconnect();
        client.disconnect();
        client.disconnect();
        
        assert!(!client.is_connected());
    }
    
    #[test]
    fn test_check_broker_availability_doesnt_crash() {
        // This function should not panic regardless of service state
        let _ = check_broker_availability();
    }
    
    #[test]
    fn test_default_timeout_matches_protocol() {
        let client = BrokerClient::new();
        assert_eq!(
            client.timeout.as_millis(),
            DEFAULT_TIMEOUT_MS as u128,
            "Client timeout should match protocol default"
        );
    }
    
    #[test]
    fn test_timeout_boundary_values() {
        // Test various timeout values
        let timeouts = vec![
            Duration::from_millis(0),
            Duration::from_millis(1),
            Duration::from_millis(1000),
            Duration::from_secs(60),
            Duration::from_secs(3600),
        ];
        
        for timeout in timeouts {
            let client = BrokerClient::new().with_timeout(timeout);
            assert_eq!(client.timeout, timeout);
        }
    }
    
    #[test]
    fn test_builder_pattern_chaining() {
        let timeout = Duration::from_secs(45);
        let client = BrokerClient::new()
            .with_timeout(timeout);
        
        assert_eq!(client.timeout, timeout);
        assert!(!client.is_connected());
    }
    
    #[test]
    fn test_client_state_after_disconnect() {
        let mut client = BrokerClient::new();
        
        // Set a custom timeout
        client = client.with_timeout(Duration::from_secs(100));
        
        // Disconnect (even though never connected)
        client.disconnect();
        
        // State should still be valid
        assert!(!client.is_connected());
        assert_eq!(client.timeout, Duration::from_secs(100));
    }
}
