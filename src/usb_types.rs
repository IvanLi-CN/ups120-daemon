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
    Error(Box<dyn std::error::Error + Send + 'static>),
}