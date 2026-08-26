// Sudo Elevation Broker Service Library
// Provides core functionality for the Windows service

pub mod audit_logger;
pub mod elevation;
pub mod pipe_server;
pub mod service;

pub use audit_logger::AuditLogger;
pub use elevation::ElevationHandler;
pub use pipe_server::PipeServer;
pub use service::{BrokerService, BrokerServiceControl};

/// Service configuration
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// Named pipe name
    pub pipe_name: String,
    
    /// Maximum concurrent elevations
    pub max_concurrent_elevations: usize,
    
    /// Default timeout in milliseconds
    pub default_timeout_ms: u32,
    
    /// Whether to require Windows Hello
    pub require_hello: bool,
    
    /// Whether to audit to Event Log
    pub audit_to_event_log: bool,
    
    /// Log level (trace, debug, info, warn, error)
    pub log_level: String,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            pipe_name: r"\\.\pipe\SudoElevationBroker".to_string(),
            max_concurrent_elevations: 10,
            default_timeout_ms: 30000,
            require_hello: true,
            audit_to_event_log: true,
            log_level: "info".to_string(),
        }
    }
}

/// Load configuration from file or use defaults
pub fn load_config() -> ServiceConfig {
    // Try to load from C:\ProgramData\Microsoft\Sudo\config.toml
    // For now, return defaults
    // TODO: Implement TOML parsing
    ServiceConfig::default()
}
