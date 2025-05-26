use serde::Serialize;
use serde::ser::{SerializeSeq, SerializeStruct};

// BQ25730 测量数据 (简化，只包含需要序列化的字段)
#[derive(Debug, Copy, Clone, PartialEq, Serialize)]
pub struct Bq25730Measurements {
    pub psys: f32,
    pub vbus: f32,
    pub idchg: f32,
    pub ichg: f32,
    pub cmpin: f32,
    pub iin: f32,
    pub vbat: f32,
    pub vsys: f32,
}

// BQ76920 测量数据 (简化，只包含需要序列化的字段)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bq76920Measurements<const N: usize> {
    #[serde(serialize_with = "serialize_voltages")]
    pub cell_voltages: [f32; N], // 修正为原始类型
    #[serde(serialize_with = "serialize_temperatures")]
    pub temperatures: Temperatures,
    pub coulomb_counter: i16,
}

// Temperatures 结构体 (简化)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Temperatures {
    #[serde(serialize_with = "serialize_thermodynamic_temperature")]
    pub ts1: f32, // 修正为原始类型
    pub is_thermistor: bool,
}

// AllMeasurements 聚合所有设备的测量数据
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AllMeasurements<const N: usize> {
    pub bq25730: Bq25730Measurements,
    pub bq76920: Bq76920Measurements<N>,
}

// 为 ElectricPotential 实现自定义序列化
#[allow(dead_code)] // 添加此行
fn serialize_electric_potential<S>(
    value: &f32, // 修正为原始类型
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_f32(*value)
}

// 为 ThermodynamicTemperature 实现自定义序列化
fn serialize_thermodynamic_temperature<S>(
    value: &f32, // 修正为原始类型
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_f32(*value)
}

// 为 [f32] 实现自定义序列化
fn serialize_voltages<S, const N: usize>(
    voltages: &[f32; N], // 修正为原始类型
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut seq = serializer.serialize_seq(Some(N))?;
    for voltage in voltages {
        seq.serialize_element(voltage)?; // 直接序列化
    }
    seq.end()
}

// 为 Temperatures 实现自定义序列化
fn serialize_temperatures<S>(temperatures: &Temperatures, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut state = serializer.serialize_struct("Temperatures", 2)?;
    state.serialize_field("ts1", &temperatures.ts1)?; // 直接序列化
    state.serialize_field("is_thermistor", &temperatures.is_thermistor)?;
    state.end()
}