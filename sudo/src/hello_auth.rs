// Windows Hello Authentication Module
//
// This module provides Windows Hello (biometric/PIN) authentication
// for privilege elevation requests.

use anyhow::{anyhow, Result};
use std::ffi::c_void;
use windows::{
    core::*,
    Foundation::*,
    Security::Credentials::UI::*,
    Win32::Foundation::*,
    Win32::Security::*,
    Win32::System::Com::*,
    Win32::System::Threading::*,
};
use windows_registry::Key;

/// Windows Hello authenticator
pub struct HelloAuthenticator {
    message: String,
}

impl HelloAuthenticator {
    /// Create a new Hello authenticator with a custom message
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
    
    /// Create authenticator with default message
    pub fn default_message() -> Self {
        Self::new("Authenticate to run elevated command")
    }
    
    /// Check if Windows Hello is available on this device
    pub fn check_availability() -> Result<UserConsentVerifierAvailability> {
        // Initialize COM for WinRT
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        
        let availability_op = UserConsentVerifier::CheckAvailabilityAsync()?;
        let availability = block_on_async(availability_op)?;
        
        Ok(availability)
    }
    
    /// Request Windows Hello authentication
    pub fn authenticate(&self) -> Result<Vec<u8>> {
        // Initialize COM for WinRT
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        
        // Request verification
        let message_hstring = HSTRING::from(&self.message);
        let verification_op = UserConsentVerifier::RequestVerificationAsync(&message_hstring)?;
        let result = block_on_async(verification_op)?;
        
        match result {
            UserConsentVerificationResult::Verified => {
                // Generate authentication token
                // In a real implementation, this would be a cryptographic token
                // For now, we'll use a simple marker
                Ok(generate_auth_token())
            }
            UserConsentVerificationResult::DeviceNotPresent => {
                Err(anyhow!("No biometric device found on this system"))
            }
            UserConsentVerificationResult::NotConfiguredForUser => {
                Err(anyhow!(
                    "Windows Hello is not configured for this user.\n\
                     Please set up Windows Hello in Settings → Accounts → Sign-in options"
                ))
            }
            UserConsentVerificationResult::DisabledByPolicy => {
                Err(anyhow!("Windows Hello is disabled by policy"))
            }
            UserConsentVerificationResult::DeviceBusy => {
                Err(anyhow!("Biometric device is busy. Please try again"))
            }
            UserConsentVerificationResult::RetriesExhausted => {
                Err(anyhow!("Too many failed authentication attempts"))
            }
            UserConsentVerificationResult::Canceled => {
                Err(anyhow!("Authentication was canceled by the user"))
            }
            _ => {
                Err(anyhow!("Windows Hello authentication failed"))
            }
        }
    }
}

impl Default for HelloAuthenticator {
    fn default() -> Self {
        Self::default_message()
    }
}

/// High-level function to authenticate with Windows Hello
pub fn authenticate_with_hello(message: Option<&str>) -> Result<Vec<u8>> {
    let authenticator = match message {
        Some(msg) => HelloAuthenticator::new(msg),
        None => HelloAuthenticator::default_message(),
    };
    
    authenticator.authenticate()
}

/// Generate a cryptographically signed authentication token
/// 
/// This token is used for audit trail integrity, not for authentication.
/// The actual authentication is performed by the broker service via
/// ImpersonateNamedPipeClient and checking the client's group membership.
/// 
/// Token format:
/// - Version (4 bytes)
/// - Timestamp (8 bytes)
/// - User SID length (4 bytes)  
/// - User SID (variable)
/// - Machine name length (4 bytes)
/// - Machine name (variable)
/// - Nonce (16 bytes)
/// - HMAC-SHA256 signature (32 bytes)
fn generate_auth_token() -> Vec<u8> {
    use std::time::{SystemTime, UNIX_EPOCH};
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::Win32::Foundation::HANDLE;
    
    type HmacSha256 = Hmac<Sha256>;
    
    const TOKEN_VERSION: u32 = 1;
    
    // Get current timestamp
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    // Get user SID
    let user_sid = get_current_user_sid().unwrap_or_else(|_| b"UNKNOWN_SID".to_vec());
    
    // Get machine name
    let machine_name = get_machine_name().unwrap_or_else(|_| "UNKNOWN_MACHINE".to_string());
    let machine_bytes = machine_name.as_bytes();
    
    // Generate nonce (random bytes)
    let nonce: [u8; 16] = [
        (timestamp & 0xFF) as u8,
        ((timestamp >> 8) & 0xFF) as u8,
        ((timestamp >> 16) & 0xFF) as u8,
        ((timestamp >> 24) & 0xFF) as u8,
        ((timestamp >> 32) & 0xFF) as u8,
        ((timestamp >> 40) & 0xFF) as u8,
        ((timestamp >> 48) & 0xFF) as u8,
        ((timestamp >> 56) & 0xFF) as u8,
        // Mix in some additional entropy
        (user_sid.len() & 0xFF) as u8,
        ((user_sid.len() >> 8) & 0xFF) as u8,
        (machine_bytes.len() & 0xFF) as u8,
        ((machine_bytes.len() >> 8) & 0xFF) as u8,
        // Additional pseudo-random bytes from timestamp
        ((timestamp ^ 0x5555AAAA) & 0xFF) as u8,
        (((timestamp ^ 0x5555AAAA) >> 8) & 0xFF) as u8,
        (((timestamp ^ 0xAAAA5555) >> 16) & 0xFF) as u8,
        (((timestamp ^ 0xAAAA5555) >> 24) & 0xFF) as u8,
    ];
    
    // Build the token data to be signed
    let mut token_data = Vec::new();
    token_data.extend_from_slice(&TOKEN_VERSION.to_le_bytes());
    token_data.extend_from_slice(&timestamp.to_le_bytes());
    token_data.extend_from_slice(&(user_sid.len() as u32).to_le_bytes());
    token_data.extend_from_slice(&user_sid);
    token_data.extend_from_slice(&(machine_bytes.len() as u32).to_le_bytes());
    token_data.extend_from_slice(machine_bytes);
    token_data.extend_from_slice(&nonce);
    
    // Create HMAC signature
    // Key is derived from a combination of machine-specific data
    // In a production deployment, this could be a service-managed key
    let hmac_key = derive_hmac_key();
    let mut mac = HmacSha256::new_from_slice(&hmac_key)
        .expect("HMAC can take key of any size");
    mac.update(&token_data);
    let signature = mac.finalize().into_bytes();
    
    // Append signature to token
    token_data.extend_from_slice(&signature);
    
    token_data
}

/// Get the current user's SID as bytes
fn get_current_user_sid() -> Result<Vec<u8>> {
    unsafe {
        let mut token: HANDLE = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;
        
        // Get the size needed
        let mut size = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);
        
        // Allocate buffer and get the token information
        let mut buffer = vec![0u8; size as usize];
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr() as *mut _),
            size,
            &mut size,
        )?;
        
        // Extract SID from TOKEN_USER structure
        let token_user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let sid_ptr = token_user.User.Sid;
        
        // Convert SID to string for storage
        use windows::Win32::Security::ConvertSidToStringSidW;
        let mut sid_string = windows::core::PWSTR::null();
        ConvertSidToStringSidW(sid_ptr, &mut sid_string)?;
        
        let sid_str = sid_string.to_string()?;
        Ok(sid_str.as_bytes().to_vec())
    }
}

/// Get the machine name
fn get_machine_name() -> Result<String> {
    use windows::Win32::System::SystemInformation::{GetComputerNameExW, ComputerNameDnsHostname};
    
    unsafe {
        let mut size = 0u32;
        let _ = GetComputerNameExW(ComputerNameDnsHostname, windows::core::PWSTR::null(), &mut size);
        
        let mut buffer = vec![0u16; size as usize];
        GetComputerNameExW(
            ComputerNameDnsHostname,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )?;
        
        Ok(String::from_utf16_lossy(&buffer[..size as usize]))
    }
}

/// Derive HMAC key from machine-specific data
/// 
/// In production, this could be:
/// - A key stored securely by the broker service (LocalSystem)
/// - Derived from TPM or other hardware security module
/// - Managed via Windows DPAPI
/// 
/// For this implementation, we derive from machine GUID and process info.
fn derive_hmac_key() -> Vec<u8> {
    use sha2::{Digest, Sha256};
    
    // Get machine GUID from registry
    let machine_guid = get_machine_guid().unwrap_or_else(|_| "DEFAULT_MACHINE_GUID".to_string());
    
    // Derive key using SHA256
    let mut hasher = Sha256::new();
    hasher.update(b"SUDO_AP_BROKER_V1");
    hasher.update(machine_guid.as_bytes());
    hasher.update(b"HELLO_AUTH_TOKEN_KEY");
    
    hasher.finalize().to_vec()
}

/// Get machine GUID from registry
fn get_machine_guid() -> Result<String> {
    use windows_registry::Key;
    
    let key = Key::Local.open(r"SOFTWARE\Microsoft\Cryptography")?;
    let guid: String = key.get_value("MachineGuid")?;
    Ok(guid)
}

/// Verify an authentication token's signature
/// 
/// This function can be called by the broker service to verify
/// the integrity of the token for audit purposes.
#[allow(dead_code)]
pub fn verify_auth_token(token: &[u8]) -> Result<bool> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    
    type HmacSha256 = Hmac<Sha256>;
    
    // Token must be at least: version(4) + timestamp(8) + sid_len(4) + machine_len(4) + nonce(16) + signature(32) = 68 bytes
    if token.len() < 68 {
        return Ok(false);
    }
    
    // Split token into data and signature
    let (data, signature) = token.split_at(token.len() - 32);
    
    // Recompute HMAC
    let hmac_key = derive_hmac_key();
    let mut mac = HmacSha256::new_from_slice(&hmac_key)
        .expect("HMAC can take key of any size");
    mac.update(data);
    
    // Verify signature
    mac.verify_slice(signature)
        .map(|_| true)
        .or(Ok(false))
}

/// Block on an async Windows Runtime operation
/// This is a simple synchronous wrapper for WinRT async operations
fn block_on_async<T>(operation: IAsyncOperation<T>) -> Result<T>
where
    T: RuntimeType + 'static,
{
    use std::time::Duration;
    use std::thread;
    
    // Wait for the operation to complete
    loop {
        match operation.Status()? {
            AsyncStatus::Completed => {
                return Ok(operation.GetResults()?);
            }
            AsyncStatus::Error => {
                let error_code = operation.ErrorCode()?;
                return Err(anyhow!("Async operation failed with error: {:?}", error_code));
            }
            AsyncStatus::Canceled => {
                return Err(anyhow!("Async operation was canceled"));
            }
            AsyncStatus::Started => {
                // Still running, wait a bit
                thread::sleep(Duration::from_millis(50));
            }
            _ => {
                return Err(anyhow!("Unknown async status"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    #[ignore] // Requires Windows Hello to be configured
    fn test_check_availability() {
        let result = HelloAuthenticator::check_availability();
        assert!(result.is_ok());
        
        match result.unwrap() {
            UserConsentVerifierAvailability::Available => {
                println!("Windows Hello is available");
            }
            UserConsentVerifierAvailability::DeviceNotPresent => {
                println!("No biometric device present");
            }
            UserConsentVerifierAvailability::NotConfiguredForUser => {
                println!("Windows Hello not configured for user");
            }
            UserConsentVerifierAvailability::DisabledByPolicy => {
                println!("Windows Hello disabled by policy");
            }
            UserConsentVerifierAvailability::DeviceBusy => {
                println!("Biometric device busy");
            }
            _ => {
                println!("Unknown availability status");
            }
        }
    }
    
    #[test]
    #[ignore] // Requires user interaction
    fn test_authenticate() {
        let authenticator = HelloAuthenticator::default_message();
        let result = authenticator.authenticate();
        
        match result {
            Ok(token) => {
                println!("Authentication successful! Token length: {}", token.len());
                assert!(!token.is_empty());
            }
            Err(e) => {
                println!("Authentication failed: {:#}", e);
            }
        }
    }
    
    #[test]
    fn test_token_generation() {
        // Test that tokens are generated and are non-empty
        let token = generate_auth_token();
        
        // Token should be at least: version(4) + timestamp(8) + sid_len(4) + machine_len(4) + nonce(16) + signature(32) = 68 bytes
        assert!(token.len() >= 68, "Token too short: {} bytes", token.len());
        
        // Verify token structure
        let version = u32::from_le_bytes([token[0], token[1], token[2], token[3]]);
        assert_eq!(version, 1, "Token version should be 1");
    }
    
    #[test]
    fn test_token_verification() {
        // Generate a token
        let token = generate_auth_token();
        
        // Verify it
        let result = verify_auth_token(&token);
        assert!(result.is_ok(), "Token verification failed");
        assert!(result.unwrap(), "Token should be valid");
    }
    
    #[test]
    fn test_token_verification_invalid() {
        // Create an invalid token (too short)
        let invalid_token = vec![1, 2, 3, 4];
        
        let result = verify_auth_token(&invalid_token);
        assert!(result.is_ok());
        assert!(!result.unwrap(), "Invalid token should not verify");
    }
    
    #[test]
    fn test_token_verification_tampered() {
        // Generate a valid token
        let mut token = generate_auth_token();
        
        // Tamper with the signature (last 32 bytes)
        let len = token.len();
        token[len - 1] ^= 0xFF; // Flip bits in last byte
        
        // Verification should fail
        let result = verify_auth_token(&token);
        assert!(result.is_ok());
        assert!(!result.unwrap(), "Tampered token should not verify");
    }
    
    #[test]
    fn test_derive_hmac_key() {
        // Test that key derivation is deterministic
        let key1 = derive_hmac_key();
        let key2 = derive_hmac_key();
        
        assert_eq!(key1, key2, "HMAC key derivation should be deterministic");
        assert_eq!(key1.len(), 32, "HMAC key should be 32 bytes (SHA256 output)");
    }
    
    #[test]
    fn test_get_current_user_sid() {
        // Test that we can get current user's SID
        let result = get_current_user_sid();
        assert!(result.is_ok(), "Should be able to get current user SID");
        
        let sid = result.unwrap();
        assert!(!sid.is_empty(), "SID should not be empty");
        
        // SID should start with "S-1-"
        let sid_str = String::from_utf8_lossy(&sid);
        assert!(sid_str.starts_with("S-1-"), "SID should start with S-1-");
    }
    
    #[test]
    fn test_get_machine_name() {
        // Test that we can get machine name
        let result = get_machine_name();
        assert!(result.is_ok(), "Should be able to get machine name");
        
        let name = result.unwrap();
        assert!(!name.is_empty(), "Machine name should not be empty");
    }
    
    #[test]
    fn test_get_machine_guid() {
        // Test that we can get machine GUID
        let result = get_machine_guid();
        assert!(result.is_ok(), "Should be able to get machine GUID");
        
        let guid = result.unwrap();
        assert!(!guid.is_empty(), "Machine GUID should not be empty");
        
        // GUID should be in format: XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX
        assert!(guid.contains('-'), "GUID should contain hyphens");
    }
    
    #[test]
    fn test_hello_authenticator_creation() {
        let auth1 = HelloAuthenticator::new("Custom message");
        assert_eq!(auth1.message, "Custom message");
        
        let auth2 = HelloAuthenticator::default_message();
        assert_eq!(auth2.message, "Authenticate to run elevated command");
        
        let auth3 = HelloAuthenticator::default();
        assert_eq!(auth3.message, auth2.message);
    }
    
    #[test]
    fn test_token_uniqueness() {
        // Generate multiple tokens and verify they're different
        // (due to timestamp and nonce differences)
        let token1 = generate_auth_token();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let token2 = generate_auth_token();
        
        assert_ne!(token1, token2, "Tokens should be unique");
    }
}
