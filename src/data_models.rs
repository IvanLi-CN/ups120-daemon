use serde::{Serialize, Deserialize};
use serde::ser::{SerializeSeq, SerializeStruct};
use bitflags::bitflags;

// BQ25730 测量数据 (简化，只包含需要序列化的字段)
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
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

bitflags! {
    /// BQ76920 系统状态寄存器 (SysStat, 0x00)
    /// 固件中读取并解析此寄存器，然后通过 USB 发送给上位机。
    #[derive(Serialize, Deserialize)]
    pub struct SystemStatus: u8 {
        const UV = 0b0000_0001;     // 欠压
        const OV = 0b0000_0010;     // 过压
        const SCD = 0b0000_0100;    // 短路放电
        const OCD = 0b0000_1000;    // 过流放电
        const OTC = 0b0001_0000;    // 充电过温
        const OTD = 0b0010_0000;    // 放电过温
        const CC_READY = 0b0100_0000; // 库仑计数器就绪
        const WD = 0b1000_0000;     // 看门狗超时
    }
}

/// MOS 管状态 (SysCtrl2, 0x05)
/// 固件中读取并解析此寄存器，然后通过 USB 发送给上位机。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MosStatus {
    ChargeOn,
    DischargeOn,
    BothOn,
    BothOff,
    Unknown, // 用于处理意外情况
}

// BQ76920 测量数据 (简化，只包含需要序列化的字段)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bq76920Measurements<const N: usize> {
    #[serde(serialize_with = "serialize_voltages", deserialize_with = "deserialize_voltages")]
    pub cell_voltages: [f32; N], // 修正为原始类型
    #[serde(serialize_with = "serialize_temperatures", deserialize_with = "deserialize_temperatures")]
    pub temperatures: Temperatures,
    pub coulomb_counter: f32, // 修改为 f32
    pub system_status: SystemStatus, // 新增字段
    pub mos_status: MosStatus,       // 新增字段
}

// Temperatures 结构体 (简化)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Temperatures {
    #[serde(serialize_with = "serialize_thermodynamic_temperature")]
    pub ts1: f32, // 修正为原始类型
    pub is_thermistor: bool,
}

// AllMeasurements 聚合所有设备的测量数据
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

use serde::de::{self, Visitor, SeqAccess};
use std::fmt;
use std::marker::PhantomData;

// 为 [f32] 实现自定义反序列化
fn deserialize_voltages<'de, D, const N: usize>(
    deserializer: D,
) -> Result<[f32; N], D::Error>
where
    D: de::Deserializer<'de>,
{
    struct ArrayVisitor<const N: usize>(PhantomData<[f32; N]>);

    impl<'de, const N: usize> Visitor<'de> for ArrayVisitor<N> {
        type Value = [f32; N];

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "an array of size {}", N)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<[f32; N], A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut arr = [0.0; N]; // 默认值
            for i in 0..N {
                arr[i] = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(i, &self))?;
            }
            Ok(arr)
        }
    }

    deserializer.deserialize_seq(ArrayVisitor(PhantomData))
}

// 为 Temperatures 实现自定义反序列化
fn deserialize_temperatures<'de, D>(
    deserializer: D,
) -> Result<Temperatures, D::Error>
where
    D: de::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(field_identifier, rename_all = "lowercase")]
    enum Field { Ts1, IsThermistor }

    struct TemperaturesVisitor;

    impl<'de> Visitor<'de> for TemperaturesVisitor {
        type Value = Temperatures;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("struct Temperatures")
        }

        fn visit_map<V>(self, mut map: V) -> Result<Temperatures, V::Error>
        where
            V: de::MapAccess<'de>,
        {
            let mut ts1 = None;
            let mut is_thermistor = None;

            while let Some(key) = map.next_key()? {
                match key {
                    Field::Ts1 => {
                        if ts1.is_some() {
                            return Err(de::Error::duplicate_field("ts1"));
                        }
                        ts1 = Some(map.next_value()?);
                    }
                    Field::IsThermistor => {
                        if is_thermistor.is_some() {
                            return Err(de::Error::duplicate_field("is_thermistor"));
                        }
                        is_thermistor = Some(map.next_value()?);
                    }
                }
            }

            let ts1 = ts1.ok_or_else(|| de::Error::missing_field("ts1"))?;
            let is_thermistor = is_thermistor.ok_or_else(|| de::Error::missing_field("is_thermistor"))?;

            Ok(Temperatures { ts1, is_thermistor })
        }
    }

    deserializer.deserialize_struct(
        "Temperatures",
        &["ts1", "is_thermistor"],
        TemperaturesVisitor,
    )
}