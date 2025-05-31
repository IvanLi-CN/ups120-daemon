// ADC原始值转物理值
pub fn adc_to_voltage(raw: u16) -> f32 {
    // 转换公式：V = raw * LSB
    raw as f32 * 0.001 // 示例：1mV/LSB
}

pub fn adc_to_temperature(raw: i16) -> f32 {
    // 转换公式：T = raw * 0.01°C
    raw as f32 * 0.01
}

// No specific conversion functions like convert_bq25730 or convert_bq76920 are needed here
// if the BinRead implementation in src/binrw_impls.rs correctly converts
// raw byte data directly into the final host data model types with correct physical units.

// Imports that might be needed by other utility functions, if any, can be added here.
// For now, keeping it minimal.
// use crate::data_models::{...};
// use bq25730_async_rs::data_types::{...};
// use bq769x0_async_rs::data_types::{...};