use super::data_models::AllMeasurements;

// USB 命令枚举
#[derive(Debug)]
pub enum UsbCommand {
    Unsubscribe,
}

// USB 事件枚举
#[derive(Debug)]
pub enum UsbEvent {
    Measurements(AllMeasurements<5>),
    Error(Box<dyn std::error::Error + Send + 'static>), // 添加 'static 生命周期
}

// 简化版的 AdcMeasurements 和 CellVoltages，仅用于 BinRead/BinWrite
// 实际的转换逻辑在 AllMeasurements 的 BinRead 实现中处理
#[allow(dead_code)] // 添加此行
pub struct AdcMeasurementsRaw {
    psys: u8,
    vbus: u8,
    idchg: u8,
    ichg: u8,
    cmpin: u8,
    iin: u8,
    vbat: u8,
    vsys: u8,
}

#[allow(dead_code)] // 添加此行
pub struct CellVoltagesRaw<const N: usize> {
    voltages: [f32; N], // 原始数据仍然是 f32
}

#[allow(dead_code)] // 添加此行
pub struct CoulombCounterRaw {
    raw_cc: i16,
}