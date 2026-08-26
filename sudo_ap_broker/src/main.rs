// Sudo Elevation Broker Service
// Windows service that runs as LocalSystem to provide SMAA elevation

use anyhow::Result;
use std::ffi::OsString;
use tracing::{error, info};
use windows_service::{
    define_windows_service,
    service_dispatcher,
};

use sudo_ap_broker::{load_config, BrokerService};

// Define service name
const SERVICE_NAME: &str = "SudoElevationBroker";

// Service entry point
define_windows_service!(ffi_service_main, service_main);

fn service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service() {
        error!("Service error: {:#}", e);
    }
}

fn run_service() -> Result<()> {
    // Initialize logging
    init_logging()?;
    
    info!("Sudo Elevation Broker Service starting...");
    
    // Load configuration
    let config = load_config();
    info!("Configuration loaded: {:?}", config);
    
    // Create and run service
    let service = BrokerService::new(config)?;
    service.run()?;
    
    Ok(())
}

fn init_logging() -> Result<()> {
    use tracing_subscriber::{fmt, EnvFilter};
    
    // Log to file in ProgramData
    let log_dir = std::path::Path::new(r"C:\ProgramData\Microsoft\Sudo\logs");
    std::fs::create_dir_all(log_dir)?;
    
    let file_appender = tracing_appender::rolling::daily(log_dir, "broker.log");
    
    // Set up subscriber with file output
    let subscriber = fmt()
        .with_writer(file_appender)
        .with_env_filter(EnvFilter::from_default_env()
            .add_directive("sudo_ap_broker=info".parse()?))
        .with_ansi(false)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)?;
    
    Ok(())
}

fn main() -> Result<()> {
    // Dispatch service control requests
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .map_err(|e| anyhow::anyhow!("Service dispatcher failed: {:?}", e))?;
    
    Ok(())
}
