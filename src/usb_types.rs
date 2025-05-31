use binrw::{BinRead, BinWrite};
use super::data_models::AllMeasurements;

#[repr(u8)]
#[derive(BinRead, BinWrite, Debug, Clone)] // 移除 Copy
pub enum UsbData {
    // Commands
    #[brw(magic = 0x00u8)]
    SubscribeStatus,
    #[brw(magic = 0x01u8)]
    UnsubscribeStatus,

    // Responses
    #[brw(magic = 0x80u8)]
    StatusResponse(AllMeasurements<5>),

    // Push Data
    #[brw(magic = 0xC0u8)]
    StatusPush(AllMeasurements<5>),
}

// USB 命令枚举 (现在可以从 UsbData 中派生)
#[derive(Debug)]
pub enum UsbCommand {
    Subscribe,
    Unsubscribe,
}

// USB 事件枚举 (现在可以从 UsbData 中派生)
#[derive(Debug)]
pub enum UsbEvent {
    Measurements(AllMeasurements<5>),
    Error(UsbError), // Changed to use UsbError
}

#[derive(Debug)]
pub enum UsbError {
    DeviceNotFound,
    OpenFailed(String),
    SetConfigurationFailed(String),
    ClaimInterfaceFailed(String),
    DetachFailed(String), // 新增: 内核驱动分离失败
    EndpointNotFound(String),
    CommandWriteFailed(String),
    ResponseReadFailed(String),
    ResponseParseError(String),
    UnexpectedResponse,
    SubscriptionFailed(String), // General subscription failure
    RusbError(rusb::Error),
    IoError(std::io::Error),
    BinrwError(String), // For binrw read/write errors
    Timeout, // For timeout errors specifically
    Other(String),
}

impl std::fmt::Display for UsbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsbError::DeviceNotFound => write!(f, "USB device not found"),
            UsbError::OpenFailed(s) => write!(f, "Failed to open USB device: {}", s),
            UsbError::SetConfigurationFailed(s) => write!(f, "Failed to set USB configuration: {}", s),
            UsbError::ClaimInterfaceFailed(s) => write!(f, "Failed to claim USB interface: {}", s),
            UsbError::DetachFailed(s) => write!(f, "Failed to detach kernel driver: {}", s),
            UsbError::EndpointNotFound(s) => write!(f, "USB endpoint not found: {}", s),
            UsbError::CommandWriteFailed(s) => write!(f, "Failed to write USB command: {}", s),
            UsbError::ResponseReadFailed(s) => write!(f, "Failed to read USB response: {}", s),
            UsbError::ResponseParseError(s) => write!(f, "Failed to parse USB response: {}", s),
            UsbError::UnexpectedResponse => write!(f, "Received unexpected USB response"),
            UsbError::SubscriptionFailed(s) => write!(f, "USB subscription failed: {}", s),
            UsbError::RusbError(e) => write!(f, "Rusb error: {}", e),
            UsbError::IoError(e) => write!(f, "IO error: {}", e),
            UsbError::BinrwError(s) => write!(f, "Binrw error: {}", s),
            UsbError::Timeout => write!(f, "USB operation timed out"),
            UsbError::Other(s) => write!(f, "USB error: {}", s),
        }
    }
}

impl std::error::Error for UsbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            UsbError::RusbError(e) => Some(e),
            UsbError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rusb::Error> for UsbError {
    fn from(err: rusb::Error) -> Self {
        if err == rusb::Error::Timeout {
            UsbError::Timeout
        } else {
            UsbError::RusbError(err)
        }
    }
}

impl From<std::io::Error> for UsbError {
    fn from(err: std::io::Error) -> Self {
        UsbError::IoError(err)
    }
}

// Helper to convert binrw::Error to UsbError::BinrwError
impl From<binrw::Error> for UsbError {
    fn from(err: binrw::Error) -> Self {
        UsbError::BinrwError(err.to_string())
    }
}