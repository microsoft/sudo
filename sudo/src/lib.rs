// Sudo for Windows - Library
// This module exports the public API for sudo functionality

pub mod ap_detection;
pub mod broker_client;
pub mod broker_protocol;
pub mod hello_auth;

pub use ap_detection::{ElevationCapabilities, ElevationEnvironment, HelloAvailability};
pub use broker_client::{BrokerClient, check_broker_availability, elevate_via_broker};
pub use broker_protocol::{
    ElevationRequest, ElevationResponse, ExecutionMode, OutputChunk, StatusCode,
    BROKER_PIPE_NAME, DEFAULT_TIMEOUT_MS, MAX_MESSAGE_SIZE, PROTOCOL_VERSION,
};
pub use hello_auth::{HelloAuthenticator, authenticate_with_hello};
