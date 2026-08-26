// Windows Service implementation

use anyhow::{Context, Result};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use tracing::{debug, error, info, warn};
use windows_service::{
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
};

use crate::{PipeServer, ServiceConfig};

/// Broker service control
pub struct BrokerServiceControl {
    shutdown_flag: Arc<AtomicBool>,
}

impl BrokerServiceControl {
    pub fn new(shutdown_flag: Arc<AtomicBool>) -> Self {
        Self { shutdown_flag }
    }
    
    pub fn handle_control(&self, control: ServiceControl) -> ServiceControlHandlerResult {
        match control {
            ServiceControl::Interrogate => {
                debug!("Service interrogate");
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Stop => {
                info!("Service stop requested");
                self.shutdown_flag.store(true, Ordering::Relaxed);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Shutdown => {
                info!("System shutdown - stopping service");
                self.shutdown_flag.store(true, Ordering::Relaxed);
                ServiceControlHandlerResult::NoError
            }
            _ => {
                warn!("Unhandled service control: {:?}", control);
                ServiceControlHandlerResult::NotImplemented
            }
        }
    }
}

/// Main broker service
pub struct BrokerService {
    config: ServiceConfig,
}

impl BrokerService {
    pub fn new(config: ServiceConfig) -> Result<Self> {
        Ok(Self { config })
    }
    
    pub fn run(self) -> Result<()> {
        // Create shutdown flag
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let shutdown_flag_clone = shutdown_flag.clone();
        
        // Register service control handler
        let event_handler = move |control| {
            let control_handler = BrokerServiceControl::new(shutdown_flag_clone.clone());
            control_handler.handle_control(control)
        };
        
        let status_handle = service_control_handler::register(
            "SudoElevationBroker",
            event_handler,
        ).context("Failed to register service control handler")?;
        
        // Tell SCM we're starting
        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StartPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(5),
            process_id: None,
        })?;
        
        info!("Service starting - initializing pipe server");
        
        // Create pipe server
        let pipe_server = match PipeServer::new(
            &self.config.pipe_name,
            self.config.max_concurrent_elevations,
            shutdown_flag.clone(),
        ) {
            Ok(server) => server,
            Err(e) => {
                error!("Failed to create pipe server: {:#}", e);
                status_handle.set_service_status(ServiceStatus {
                    service_type: ServiceType::OWN_PROCESS,
                    current_state: ServiceState::Stopped,
                    controls_accepted: ServiceControlAccept::empty(),
                    exit_code: ServiceExitCode::Win32(1),
                    checkpoint: 0,
                    wait_hint: Duration::default(),
                    process_id: None,
                })?;
                return Err(e);
            }
        };
        
        // Tell SCM we're running
        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;
        
        info!("Service running - listening for elevation requests");
        
        // Run server (blocks until shutdown)
        if let Err(e) = pipe_server.run() {
            error!("Pipe server error: {:#}", e);
            
            // Tell SCM we're stopping with error
            status_handle.set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::Stopped,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code: ServiceExitCode::Win32(1),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            })?;
            
            return Err(e);
        }
        
        info!("Service shutting down gracefully");
        
        // Tell SCM we're stopping
        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StopPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(5),
            process_id: None,
        })?;
        
        // Final status: stopped
        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;
        
        Ok(())
    }
}
