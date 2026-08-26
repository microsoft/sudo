// Administrator Protection Detection Module
// 
// This module detects whether Windows Administrator Protection (AP) is enabled
// and determines the system's capability to support AP-based elevation.

use std::collections::HashMap;
use windows::{
    core::*,
    Win32::System::Registry::*,
    Security::Credentials::UI::*,
};

/// Represents the various elevation environments sudo can encounter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationEnvironment {
    /// Standard UAC with split-token model (legacy Windows behavior)
    StandardUAC,
    
    /// Administrator Protection enabled with Windows Hello available
    AdminProtectionWithHello,
    
    /// Administrator Protection enabled but Windows Hello not configured
    AdminProtectionWithoutHello,
    
    /// User is not an administrator
    NoAdminPrivileges,
    
    /// Unable to determine the environment
    Unknown,
}

/// Result of checking Windows Hello availability
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelloAvailability {
    Available,
    NotAvailable,
    DeviceNotCapable,
    DeviceBusy,
    Unknown,
}

// Error message constants for better localization and maintenance
const ERROR_NO_ADMIN_PRIVILEGES: &str = "You must be a member of the Administrators group to use sudo.";
const ERROR_AP_WITHOUT_HELLO: &str = "❌ Administrator Protection requires Windows Hello\n\n\
    To use sudo with Administrator Protection:\n\
    1. Open Settings → Accounts → Sign-in options\n\
    2. Set up Windows Hello (PIN, Face, or Fingerprint)\n\
    3. Try sudo again\n\n\
    Alternative: Disable Administrator Protection in Windows Security settings";
const ERROR_BROKER_UNAVAILABLE: &str = "⚠️  Administrator Protection is enabled but the elevation broker service is not available.\n\n\
    To fix this:\n\
    1. Run: sc start SudoElevationBroker\n\
    2. Or reinstall sudo: winget install Microsoft.Sudo\n\n\
    If the problem persists, check Windows Event Logs for errors.";
const ERROR_UNKNOWN_ENVIRONMENT: &str = "Unable to determine elevation capabilities. Please check system configuration.";

/// Comprehensive system elevation capability information
#[derive(Debug, Clone)]
pub struct ElevationCapabilities {
    pub environment: ElevationEnvironment,
    pub ap_enabled: bool,
    pub hello_available: HelloAvailability,
    pub user_is_admin: bool,
    pub broker_service_available: bool,
    pub windows_version: String,
}

impl ElevationCapabilities {
    /// Detect all elevation capabilities of the current system
    pub fn detect() -> Result<Self> {
        let ap_enabled = is_admin_protection_enabled()?;
        let hello_available = check_hello_availability()?;
        let user_is_admin = is_current_user_admin()?;
        let broker_service_available = is_broker_service_available()?;
        let windows_version = get_windows_version()?;
        
        let environment = determine_environment(
            ap_enabled,
            hello_available,
            user_is_admin,
        );
        
        Ok(Self {
            environment,
            ap_enabled,
            hello_available,
            user_is_admin,
            broker_service_available,
            windows_version,
        })
    }
    
    /// Check if the system can perform elevation
    pub fn can_elevate(&self) -> bool {
        match self.environment {
            ElevationEnvironment::StandardUAC => self.user_is_admin,
            ElevationEnvironment::AdminProtectionWithHello => {
                self.user_is_admin && self.broker_service_available
            }
            ElevationEnvironment::AdminProtectionWithoutHello => false,
            ElevationEnvironment::NoAdminPrivileges => false,
            ElevationEnvironment::Unknown => false,
        }
    }
    
    /// Get user-friendly error message if elevation is not possible
    pub fn get_elevation_error_message(&self) -> Option<String> {
        if self.can_elevate() {
            return None;
        }
        
        match self.environment {
            ElevationEnvironment::NoAdminPrivileges => {
                Some(ERROR_NO_ADMIN_PRIVILEGES.to_string())
            }
            ElevationEnvironment::AdminProtectionWithoutHello => {
                Some(ERROR_AP_WITHOUT_HELLO.to_string())
            }
            ElevationEnvironment::AdminProtectionWithHello if !self.broker_service_available => {
                Some(ERROR_BROKER_UNAVAILABLE.to_string())
            }
            ElevationEnvironment::Unknown => {
                Some(ERROR_UNKNOWN_ENVIRONMENT.to_string())
            }
            _ => None,
        }
    }
}

/// Check if Administrator Protection is enabled via registry
fn is_admin_protection_enabled() -> Result<bool> {
    unsafe {
        // Open the system policies registry key
        let mut hkey = HKEY::default();
        let result = RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System"),
            0,
            KEY_READ,
            &mut hkey,
        );
        
        if result.is_err() {
            // If we can't read the registry, assume AP is not enabled
            return Ok(false);
        }
        
        // Check FilterAdministratorToken
        // When AP is enabled, this value is set to 1
        let mut filter_token: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let filter_result = RegQueryValueExW(
            hkey,
            w!("FilterAdministratorToken"),
            None,
            None,
            Some(&mut filter_token as *mut u32 as *mut u8),
            Some(&mut size),
        );
        
        // Check EnableLUA (UAC enabled)
        let mut enable_lua: u32 = 0;
        let mut lua_size = std::mem::size_of::<u32>() as u32;
        let lua_result = RegQueryValueExW(
            hkey,
            w!("EnableLUA"),
            None,
            None,
            Some(&mut enable_lua as *mut u32 as *mut u8),
            Some(&mut lua_size),
        );
        
        RegCloseKey(hkey).ok();
        
        // AP is enabled if FilterAdministratorToken == 1 and EnableLUA == 1
        Ok(filter_result.is_ok() && filter_token == 1 && 
           lua_result.is_ok() && enable_lua == 1)
    }
}

/// Check Windows Hello availability
fn check_hello_availability() -> Result<HelloAvailability> {
    // Note: This is a simplified check. In production, we'd need async handling
    // For now, we'll use a blocking check with timeout
    
    use windows::Security::Credentials::UI::*;
    
    // Try to check availability
    match UserConsentVerifier::CheckAvailabilityAsync() {
        Ok(async_op) => {
            // In a real implementation, we'd properly await this
            // For this POC, we'll use a simplified blocking approach
            // or mark as Unknown and check at elevation time
            Ok(HelloAvailability::Unknown)
        }
        Err(_) => Ok(HelloAvailability::NotAvailable),
    }
}

/// Check if current user is a member of the Administrators group
fn is_current_user_admin() -> Result<bool> {
    use windows::Win32::Security::*;
    use windows::Win32::Foundation::*;
    
    unsafe {
        let mut admin_group = SID::default();
        let mut sid_size = std::mem::size_of::<SID>() as u32;
        
        // Create well-known SID for Administrators group
        let result = CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            None,
            PSID(&mut admin_group as *mut SID as *mut std::ffi::c_void),
            &mut sid_size,
        );
        
        if result.is_err() {
            return Ok(false);
        }
        
        // Check if current user is a member
        let mut is_member = BOOL::default();
        let check_result = CheckTokenMembership(
            None,
            PSID(&admin_group as *const SID as *const std::ffi::c_void),
            &mut is_member,
        );
        
        if check_result.is_ok() {
            Ok(is_member.as_bool())
        } else {
            Ok(false)
        }
    }
}

/// Check if the elevation broker service is installed and running
fn is_broker_service_available() -> Result<bool> {
    use windows::Win32::System::Services::*;
    use windows::Win32::Foundation::*;
    
    unsafe {
        // Open service control manager
        let scm = OpenSCManagerW(
            None,
            None,
            SC_MANAGER_CONNECT,
        )?;
        
        // Try to open our service
        let service_result = OpenServiceW(
            scm,
            w!("SudoElevationBroker"),
            SERVICE_QUERY_STATUS,
        );
        
        if let Ok(service) = service_result {
            // Query service status
            let mut status = SERVICE_STATUS::default();
            let status_result = QueryServiceStatus(service, &mut status);
            
            CloseServiceHandle(service).ok();
            CloseServiceHandle(scm).ok();
            
            if status_result.is_ok() {
                return Ok(status.dwCurrentState == SERVICE_RUNNING);
            }
        } else {
            CloseServiceHandle(scm).ok();
        }
        
        Ok(false)
    }
}

/// Get Windows version information
fn get_windows_version() -> Result<String> {
    use windows::Win32::System::SystemInformation::*;
    
    unsafe {
        let mut version_info = OSVERSIONINFOEXW::default();
        version_info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOEXW>() as u32;
        
        // Note: GetVersionEx is deprecated, but we'll use it for now
        // In production, we should use RtlGetVersion or other methods
        let info_ptr = &mut version_info as *mut OSVERSIONINFOEXW as *mut OSVERSIONINFOW;
        
        #[allow(deprecated)]
        if GetVersionExW(info_ptr).as_bool() {
            Ok(format!(
                "{}.{}.{}",
                version_info.dwMajorVersion,
                version_info.dwMinorVersion,
                version_info.dwBuildNumber
            ))
        } else {
            Ok("Unknown".to_string())
        }
    }
}

/// Determine the elevation environment based on detected capabilities
fn determine_environment(
    ap_enabled: bool,
    hello_available: HelloAvailability,
    user_is_admin: bool,
) -> ElevationEnvironment {
    if !user_is_admin {
        return ElevationEnvironment::NoAdminPrivileges;
    }
    
    if !ap_enabled {
        return ElevationEnvironment::StandardUAC;
    }
    
    // AP is enabled
    match hello_available {
        HelloAvailability::Available => ElevationEnvironment::AdminProtectionWithHello,
        HelloAvailability::NotAvailable | 
        HelloAvailability::DeviceNotCapable |
        HelloAvailability::DeviceBusy => ElevationEnvironment::AdminProtectionWithoutHello,
        HelloAvailability::Unknown => ElevationEnvironment::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_capabilities_detection() {
        // This test will only work on Windows systems
        if cfg!(windows) {
            let caps = ElevationCapabilities::detect();
            assert!(caps.is_ok(), "Should be able to detect capabilities");
            
            let caps = caps.unwrap();
            println!("Detected environment: {:?}", caps.environment);
            println!("AP enabled: {}", caps.ap_enabled);
            println!("Hello available: {:?}", caps.hello_available);
            println!("User is admin: {}", caps.user_is_admin);
            println!("Broker available: {}", caps.broker_service_available);
        }
    }
    
    #[test]
    fn test_hello_availability_values() {
        // Test that all enum values can be compared
        assert_eq!(HelloAvailability::Available, HelloAvailability::Available);
        assert_ne!(HelloAvailability::Available, HelloAvailability::NotAvailable);
        assert_ne!(HelloAvailability::DeviceNotCapable, HelloAvailability::DeviceBusy);
    }
    
    #[test]
    fn test_elevation_environment_values() {
        // Test that all enum values can be compared
        assert_eq!(ElevationEnvironment::StandardUAC, ElevationEnvironment::StandardUAC);
        assert_ne!(ElevationEnvironment::StandardUAC, ElevationEnvironment::NoAdminPrivileges);
        assert_ne!(
            ElevationEnvironment::AdminProtectionWithHello,
            ElevationEnvironment::AdminProtectionWithoutHello
        );
    }
    
    #[test]
    fn test_can_elevate_no_admin_privileges() {
        let caps = ElevationCapabilities {
            environment: ElevationEnvironment::NoAdminPrivileges,
            ap_enabled: false,
            hello_available: HelloAvailability::NotAvailable,
            user_is_admin: false,
            broker_service_available: false,
            windows_version: "11.0.22621".to_string(),
        };
        
        assert!(!caps.can_elevate(), "Non-admin users cannot elevate");
    }
    
    #[test]
    fn test_can_elevate_standard_uac() {
        let caps = ElevationCapabilities {
            environment: ElevationEnvironment::StandardUAC,
            ap_enabled: false,
            hello_available: HelloAvailability::NotAvailable,
            user_is_admin: true,
            broker_service_available: false,
            windows_version: "10.0.19045".to_string(),
        };
        
        assert!(caps.can_elevate(), "Standard UAC with admin user can elevate");
    }
    
    #[test]
    fn test_can_elevate_ap_with_hello() {
        let caps = ElevationCapabilities {
            environment: ElevationEnvironment::AdminProtectionWithHello,
            ap_enabled: true,
            hello_available: HelloAvailability::Available,
            user_is_admin: true,
            broker_service_available: true,
            windows_version: "11.0.22621".to_string(),
        };
        
        assert!(caps.can_elevate(), "AP with Hello and broker can elevate");
    }
    
    #[test]
    fn test_can_elevate_ap_without_broker() {
        let caps = ElevationCapabilities {
            environment: ElevationEnvironment::AdminProtectionWithHello,
            ap_enabled: true,
            hello_available: HelloAvailability::Available,
            user_is_admin: true,
            broker_service_available: false,
            windows_version: "11.0.22621".to_string(),
        };
        
        assert!(!caps.can_elevate(), "AP without broker cannot elevate");
    }
    
    #[test]
    fn test_can_elevate_ap_without_hello() {
        let caps = ElevationCapabilities {
            environment: ElevationEnvironment::AdminProtectionWithoutHello,
            ap_enabled: true,
            hello_available: HelloAvailability::NotAvailable,
            user_is_admin: true,
            broker_service_available: true,
            windows_version: "11.0.22621".to_string(),
        };
        
        assert!(!caps.can_elevate(), "AP without Hello cannot elevate");
    }
    
    #[test]
    fn test_error_message_no_admin() {
        let caps = ElevationCapabilities {
            environment: ElevationEnvironment::NoAdminPrivileges,
            ap_enabled: false,
            hello_available: HelloAvailability::NotAvailable,
            user_is_admin: false,
            broker_service_available: false,
            windows_version: "11.0.22621".to_string(),
        };
        
        let error = caps.get_elevation_error_message();
        assert!(error.is_some());
        assert!(error.unwrap().contains("Administrators group"));
    }
    
    #[test]
    fn test_error_message_ap_without_hello() {
        let caps = ElevationCapabilities {
            environment: ElevationEnvironment::AdminProtectionWithoutHello,
            ap_enabled: true,
            hello_available: HelloAvailability::NotAvailable,
            user_is_admin: true,
            broker_service_available: true,
            windows_version: "11.0.22621".to_string(),
        };
        
        let error = caps.get_elevation_error_message();
        assert!(error.is_some());
        let msg = error.unwrap();
        assert!(msg.contains("Windows Hello"));
        assert!(msg.contains("Settings"));
    }
    
    #[test]
    fn test_error_message_broker_unavailable() {
        let caps = ElevationCapabilities {
            environment: ElevationEnvironment::AdminProtectionWithHello,
            ap_enabled: true,
            hello_available: HelloAvailability::Available,
            user_is_admin: true,
            broker_service_available: false,
            windows_version: "11.0.22621".to_string(),
        };
        
        let error = caps.get_elevation_error_message();
        assert!(error.is_some());
        let msg = error.unwrap();
        assert!(msg.contains("broker service"));
        assert!(msg.contains("SudoElevationBroker"));
    }
    
    #[test]
    fn test_error_message_unknown_environment() {
        let caps = ElevationCapabilities {
            environment: ElevationEnvironment::Unknown,
            ap_enabled: false,
            hello_available: HelloAvailability::Unknown,
            user_is_admin: false,
            broker_service_available: false,
            windows_version: "11.0.22621".to_string(),
        };
        
        let error = caps.get_elevation_error_message();
        assert!(error.is_some());
        assert!(error.unwrap().contains("Unable to determine"));
    }
    
    #[test]
    fn test_no_error_when_can_elevate() {
        let caps = ElevationCapabilities {
            environment: ElevationEnvironment::StandardUAC,
            ap_enabled: false,
            hello_available: HelloAvailability::NotAvailable,
            user_is_admin: true,
            broker_service_available: false,
            windows_version: "10.0.19045".to_string(),
        };
        
        assert!(caps.can_elevate());
        assert!(caps.get_elevation_error_message().is_none());
    }
    
    #[test]
    fn test_windows_version_field() {
        let caps = ElevationCapabilities {
            environment: ElevationEnvironment::StandardUAC,
            ap_enabled: false,
            hello_available: HelloAvailability::NotAvailable,
            user_is_admin: true,
            broker_service_available: false,
            windows_version: "11.0.22621".to_string(),
        };
        
        assert_eq!(caps.windows_version, "11.0.22621");
        assert!(!caps.windows_version.is_empty());
    }
    
    #[test]
    fn test_capabilities_clone() {
        let caps = ElevationCapabilities {
            environment: ElevationEnvironment::StandardUAC,
            ap_enabled: false,
            hello_available: HelloAvailability::Available,
            user_is_admin: true,
            broker_service_available: false,
            windows_version: "11.0.22621".to_string(),
        };
        
        let cloned = caps.clone();
        assert_eq!(caps.environment, cloned.environment);
        assert_eq!(caps.ap_enabled, cloned.ap_enabled);
        assert_eq!(caps.hello_available, cloned.hello_available);
        assert_eq!(caps.user_is_admin, cloned.user_is_admin);
        assert_eq!(caps.windows_version, cloned.windows_version);
    }
    
    #[test]
    fn test_all_environment_types_covered_in_can_elevate() {
        // This test ensures can_elevate handles all environment types
        let environments = vec![
            ElevationEnvironment::StandardUAC,
            ElevationEnvironment::AdminProtectionWithHello,
            ElevationEnvironment::AdminProtectionWithoutHello,
            ElevationEnvironment::NoAdminPrivileges,
            ElevationEnvironment::Unknown,
        ];
        
        for env in environments {
            let caps = ElevationCapabilities {
                environment: env,
                ap_enabled: false,
                hello_available: HelloAvailability::NotAvailable,
                user_is_admin: false,
                broker_service_available: false,
                windows_version: "11.0.22621".to_string(),
            };
            
            // Should not panic
            let _ = caps.can_elevate();
        }
    }
    
    #[test]
    fn test_all_hello_availability_values() {
        let values = vec![
            HelloAvailability::Available,
            HelloAvailability::NotAvailable,
            HelloAvailability::DeviceNotCapable,
            HelloAvailability::DeviceBusy,
            HelloAvailability::Unknown,
        ];
        
        // All values should be copyable and comparable
        for val in &values {
            let copied = *val;
            assert_eq!(*val, copied);
        }
    }
}
