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
    #[derive(Serialize, Deserialize)] // Default will be implemented manually
    #[serde(transparent)]
    pub struct SystemStatus: u8 {
        const OCD = 0b0000_0001;     // 过流放电 (对应固件 Bit 0)
        const SCD = 0b0000_0010;    // 短路放电 (对应固件 Bit 1)
        const OV = 0b0000_0100;     // 过压 (对应固件 Bit 2)
        const UV = 0b0000_1000;     // 欠压 (对应固件 Bit 3)
        const OVRD_ALERT = 0b0001_0000; // 覆盖警报 (对应固件 Bit 4)
        const DEVICE_XREADY = 0b0010_0000; // 设备就绪 (对应固件 Bit 5)
        const OVR_TEMP = 0b0100_0000; // 过温 (对应固件 Bit 6)
        const CC_READY = 0b1000_0000; // 库仑计数器就绪 (对应固件 Bit 7)
    }
}

impl Default for SystemStatus {
    fn default() -> Self {
        SystemStatus::empty()
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
    pub ina226: Ina226Measurements,
    pub bq25730_alerts: Bq25730Alerts,
    pub bq76920_alerts: Bq76920Alerts,
}

// INA226测量结构体 (already exists)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ina226Measurements {
    pub voltage: f32,
    pub current: f32,
    pub power: f32,
}

bitflags! {
    #[derive(Serialize, Deserialize, Default)]
    #[serde(transparent)]
    pub struct ChargerStatusFlags: u8 {
        const STAT_AC         = 0b10000000; // Input source status: 1 = AC adapter
        const ICO_DONE        = 0b01000000; // ICO routine complete
        const IN_VAP          = 0b00100000; // Charger is operated in VAP mode
        const IN_VINDPM       = 0b00010000; // Charger is in VINDPM or OTG voltage regulation
        const IN_IIN_DPM      = 0b00001000; // Charger is in IIN_DPM
        const IN_FCHRG        = 0b00000100; // Charger is in fast charge
        const IN_PCHRG        = 0b00000010; // Charger is in pre-charge
        const IN_OTG          = 0b00000001; // Charger is in OTG
    }
}

bitflags! {
    #[derive(Serialize, Deserialize, Default)]
    #[serde(transparent)]
    pub struct ChargerFaultFlags: u8 {
        const FAULT_ACOV      = 0b10000000; // ACOV fault
        const FAULT_BATOC     = 0b01000000; // BATOC fault
        const FAULT_ACOC      = 0b00100000; // ACOC fault
        const FAULT_SYSOVP    = 0b00010000; // SYSOVP fault
        const FAULT_VSYS_UVP  = 0b00001000; // VSYS_UVP fault (BQ25730 specific, not in BQ25700)
        const FAULT_CONV_OFF  = 0b00000100; // Force converter off fault (BQ25730 specific)
        const FAULT_OTG_OVP   = 0b00000010; // OTG OVP fault
        const FAULT_OTG_UVP   = 0b00000001; // OTG UVP fault
    }
}

bitflags! {
    #[derive(Serialize, Deserialize, Default)]
    #[serde(transparent)]
    pub struct ProchotLsbFlags: u8 { // Corresponds to PROCHOT_STATUS_LSB (0x38)
        const STAT_PTM          = 0b10000000; // PTM PROCHOT Profile status
        const STAT_IDCHG2       = 0b01000000; // IDCHG2 PROCHOT Profile status
        const STAT_ADPT_REMOVAL = 0b00100000; // Adapter removal PROCHOT Profile status
        const STAT_BAT_REMOVAL  = 0b00010000; // Battery removal PROCHOT Profile status
        const STAT_VSYS         = 0b00001000; // VSYS PROCHOT Profile status
        const STAT_IDCHG1       = 0b00000100; // IDCHG1 PROCHOT Profile status
        const STAT_INOM         = 0b00000010; // INOM PROCHOT Profile status
        const STAT_ICRIT        = 0b00000001; // ICRIT PROCHOT Profile status
    }
}

bitflags! {
    #[derive(Serialize, Deserialize, Default)]
    #[serde(transparent)]
    pub struct ProchotMsbFlags: u8 { // Corresponds to PROCHOT_STATUS_MSB (0x39)
        const EN_PROCHOT_EXT  = 0b10000000; // PROCHOT Pulse Extension Enable
        // Bits 6:5 for PROCHOT_WIDTH, handled separately
        const PROCHOT_CLEAR   = 0b00010000; // PROCHOT Pulse Clear
        const STAT_VAP_FAIL   = 0b00001000; // VAP failure status
        const STAT_EXIT_VAP   = 0b00000100; // Exit VAP status
        const STAT_VINDPM     = 0b00000010; // VINDPM PROCHOT Profile status
        const STAT_COMP       = 0b00000001; // CMPOUT PROCHOT Profile status
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Bq25730Alerts {
    pub charger_status_flags: ChargerStatusFlags,
    pub charger_fault_flags: ChargerFaultFlags,
    pub prochot_lsb_flags: ProchotLsbFlags,
    pub prochot_msb_flags: ProchotMsbFlags,
    pub prochot_width: u8, // Extracted from PROCHOT_STATUS_MSB bits 6:5
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Bq76920Alerts {
    pub system_status: SystemStatus, // Uses the existing SystemStatus bitflag
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