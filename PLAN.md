# USB 通信字段一致性检查与修复计划

## 1. 目标

确保上位机与固件在USB通信中的字段定义、序列化/反序列化逻辑以及数据转换逻辑完全一致。

## 2. 已识别的核心问题

上位机 `src/binrw_impls.rs` 中 `AllMeasurements<N>::read_options` 的实现与固件实际发送的 `AllMeasurementsUsbPayload` (定义于 `device/src/data_types.rs`) 在以下方面存在不一致：
    ***字节序（Endianness）**
    *   **字段读取类型** (例如 `u16` vs `u8`, `i32` vs `f32`)
    ***原始数据到物理单位的转换逻辑** (特别是 BQ25730 ADC 的偏移量和 BQ76920 温度的转换)
    *   **关键状态标志位定义** (尤其是 BQ25730 Prochot 相关)

## 3. 详细修改方案

### I. 修改上位机 `src/binrw_impls.rs` 中 `AllMeasurements<N>::read_options` 实现

* **A. 全局字节序更正**：
  * 将所有 `u16::read_options`, `i32::read_options`, `f32::read_options` 等调用中的 `Endian::Big` **全部修改为 `Endian::Little`**。

* **B. BQ25730 ADC Measurements 读取与转换修正**:
    1. **读取原始值**：将 `bq25730_psys_raw` 到 `bq25730_vsys_raw` 的读取类型从 `u8` 改为 `u16`，使用 `Endian::Little`。

        ```rust
        // 示例:
        // let bq25730_adc_vbat_raw_u8 = u8::read_options(reader, Endian::Little, args)?; // 原代码（错误）
        let bq25730_adc_vbat_raw_u16 = u16::read_options(reader, Endian::Little, args)?; // 修正后
        // ... 对其他 BQ25730 ADC 原始值做类似修改
        ```

    2. **转换逻辑**：固件发送的是原始寄存器格式的 `u16` 值。上位机需使用 `AdcMeasurementType::from_u16(raw_u16_value, OFFSET).0` 来得到以mV/mA/mW为单位的值，然后再转换为V/A/W。
        * `psys`: `(AdcPsys::from_u16(bq25730_adc_psys_raw_u16, 0 /*核对AdcPsys的OFFSET*/).0 as f32) / 1000.0`
        * `vbus`: `(AdcVbus::from_u16(bq25730_adc_vbus_raw_u16, AdcVbus::OFFSET_MV /*device/bq25730/src/data_types.rs中定义*/).0 as f32) / 1000.0`
        * `idchg`: `(AdcIdchg::from_u16(bq25730_adc_idchg_raw_u16, 0 /*核对AdcIdchg的OFFSET*/).0 as f32) / 1000.0`
        * `ichg`: `(AdcIchg::from_u16(bq25730_adc_ichg_raw_u16, 0 /*核对AdcIchg的OFFSET*/).0 as f32) / 1000.0`
        * `cmpin`: `(AdcCmpin::from_u16(bq25730_adc_cmpin_raw_u16, 0 /*核对AdcCmpin的OFFSET*/).0 as f32) / 1000.0`
        * `iin`: `(AdcIin::from_u16(bq25730_adc_iin_raw_u16, 0 /*核对AdcIin的OFFSET*/, true /*rsns_rac_is_5m_ohm*/).milliamps as f32) / 1000.0`
        * `vbat`: `(AdcVbat::from_u16(bq25730_adc_vbat_raw_u16, 0 /*AdcVbat::OFFSET_MV 应为0*/).0 as f32) / 1000.0`
        * `vsys`: `(AdcVsys::from_u16(bq25730_adc_vsys_raw_u16, 0 /*AdcVsys::OFFSET_MV 应为0*/).0 as f32) / 1000.0`
        * **注意**: `AdcVbat::OFFSET_MV` 和 `AdcVsys::OFFSET_MV` 在 `device/bq25730/src/data_types.rs` 中定义为 `0`。上位机转换时必须使用 `0` 作为偏移量。其他 ADC 类型的 OFFSET 也需要从 `device/bq25730/src/data_types.rs` 中核实。

* **C. BQ76920 Measurements 读取与转换修正**:
    1. **Cell Voltages**: 读取为 `i32` (mV)，使用 `Endian::Little`，然后转换为 `f32` (V)。

        ```rust
        // let cell_voltages_raw[i] = f32::read_options(reader, Endian::Little, args)?; // 原代码（错误）
        let voltage_i32 = i32::read_options(reader, Endian::Little, args)?; // 修正后
        cell_voltages_raw[i] = voltage_i32 as f32 / 1000.0; // 转换为 V
        ```

    2. **Temperatures**:
        * 读取 `bq76920_ts1_raw_adc` 为 `u16` (`Endian::Little`)。
        * 读取 `bq76920_ts2_present` 为 `u8`。
        * 读取 `bq76920_ts2_raw_adc` 为 `u16` (`Endian::Little`)。
        * 读取 `bq76920_ts3_present` 为 `u8`。
        * 读取 `bq76920_ts3_raw_adc` 为 `u16` (`Endian::Little`)。
        * 读取 `bq76920_is_thermistor_raw` 为 `u8`。
        * **转换逻辑**: 如果 `is_thermistor` 为 false（内部传感器），应用公式：
            `temp_C = (2500.0 - (((raw_adc_u16 as i32 * 382) - 1_200_000) as f32 / 42.0)) / 100.0`
            应用于 `ts1`, `ts2` (如果存在), `ts3` (如果存在)。
    3. **Current (Coulomb Counter)**: 读取为 `i32` (mA)，使用 `Endian::Little`，然后转换为 `f32` (A)。

        ```rust
        // let current_raw = f32::read_options(reader, Endian::Little, args)?; // 原代码（错误）
        let current_i32 = i32::read_options(reader, Endian::Little, args)?; // 修正后
        current_raw = current_i32 as f32 / 1000.0; // 转换为 A
        ```

* **D. BQ25730 Alerts 读取修正**:
    1. `charger_status`: 读取单个 `u16` (`Endian::Little`)。

        ```rust
        let bq25730_charger_status_raw_u16 = u16::read_options(reader, Endian::Little, args)?;
        let bq25730_charger_status_flags_raw = (bq25730_charger_status_raw_u16 >> 8) as u8;
        let bq25730_charger_fault_flags_raw = (bq25730_charger_status_raw_u16 & 0xFF) as u8;
        ```

    2. `prochot_status`: 读取单个 `u16` (`Endian::Little`)。

        ```rust
        let bq25730_prochot_status_raw_u16 = u16::read_options(reader, Endian::Little, args)?;
        // 从 u16 中正确提取 msb_flags, lsb_flags, 和 prochot_width
        // 固件 ProchotStatus::to_u16() 将 prochot_width 放在 bits 13:12
        // value |= ((self.prochot_width & 0x03) as u16) << 12;
        let bq25730_prochot_width = ((bq25730_prochot_status_raw_u16 >> 12) & 0x03) as u8;
        // 提取 lsb (bits 7:0)
        let bq25730_prochot_lsb_flags_raw = (bq25730_prochot_status_raw_u16 & 0xFF) as u8;
        // 提取 msb (bits 15:8), 但要屏蔽掉 prochot_width 所占用的位
        let bq25730_prochot_msb_flags_raw = ((bq25730_prochot_status_raw_u16 >> 8) & !0x30) as u8; // 假设 prochot_width 在原始MSB的 bit 5:4 (0x30 mask)
                                                                                             // 需要根据 ProchotStatusMsbFlags 的实际位域来精确屏蔽
        ```

        基于固件 `ProchotStatus::to_u16()` 的实现:
        `value |= (self.msb_flags.bits() as u16) << 8;`
        `value |= self.lsb_flags.bits() as u16;`
        `value |= ((self.prochot_width & 0x03) as u16) << 12;`
        因此，解析时：
        `let bq25730_prochot_width = ((bq25730_prochot_status_raw_u16 >> 12) & 0x03) as u8;`
        `let bq25730_prochot_lsb_flags_raw = (bq25730_prochot_status_raw_u16 & 0xFF) as u8;`
        `let bq25730_prochot_msb_flags_raw = ((bq25730_prochot_status_raw_u16 >> 8) & 0xFF) as u8; // MSB flags should be clean of width here`
        然后 `ProchotMsbFlags::from_bits_truncate(bq25730_prochot_msb_flags_raw)`。

### II. 修改上位机 `src/data_models.rs`

* **A. BQ76920 `Temperatures` 结构**：
  * 添加 `ts2: Option<f32>` 和 `ts3: Option<f32>` 字段。
* **B. BQ76920 `SystemStatus` 标志位**：
  * 移除 `const OVR_TEMP = 0b0100_0000;`。
* **C. BQ25730 `ProchotMsbFlags`**：
  * 严格按照固件 `device/bq25730/src/registers.rs` 的 `ProchotStatusMsbFlags` 定义进行修正。

        ```rust
        // ProchotMsbFlags in data_models.rs
        bitflags! {
            #[derive(Serialize, Deserialize, Default)]
            #[serde(transparent)]
            pub struct ProchotMsbFlags: u8 {
                const EN_PROCHOT_EXT  = 1 << 6;
                // PROCHOT_WIDTH (bits 5:4 of original MSB) is handled as a separate field 'prochot_width'
                const PROCHOT_CLEAR   = 1 << 3;
                // Bit 2 is reserved in firmware
                const STAT_VAP_FAIL   = 1 << 1;
                const STAT_EXIT_VAP   = 1 << 0;
            }
        }
        ```

* **D. BQ25730 `ProchotLsbFlags`**：
  * 严格按照固件 `device/bq25730/src/registers.rs` 的 `ProchotStatusFlags` (LSB) 定义进行修正。

        ```rust
        // ProchotLsbFlags in data_models.rs
        bitflags! {
            #[derive(Serialize, Deserialize, Default)]
            #[serde(transparent)]
            pub struct ProchotLsbFlags: u8 {
                const STAT_VINDPM       = 1 << 7;
                const STAT_COMP         = 1 << 6;
                const STAT_ICRIT        = 1 << 5;
                const STAT_INOM         = 1 << 4;
                const STAT_IDCHG1       = 1 << 3;
                const STAT_VSYS         = 1 << 2;
                const STAT_BAT_REMOVAL  = 1 << 1;
                const STAT_ADPT_REMOVAL = 1 << 0;
            }
        }
        ```

### III. 确认单位假设

* BQ76920 电芯电压：上位机期望单位为 **V**。
* BQ76920 电流：上位机期望单位为 **A**。

### IV. Mermaid 图总结

```mermaid
graph TD
    subgraph Firmware (device)
        direction LR
        F_AllMeas["AllMeasurements<5> \n(device/src/data_types.rs) \n(Physical Units/Parsed State)"] --- F_ConvertToPayload["convert_to_payload() \n(device/src/usb/mod.rs)"]
        F_ConvertToPayload --- F_UsbPayload["AllMeasurementsUsbPayload \n(device/src/data_types.rs) \n(Raw u16/i32/u8, f32 for INA226) \n(Little Endian)"]
        F_UsbPayload --- F_UsbSend["USB Send \n(device/src/usb/endpoints.rs)"]
    end

    subgraph Host (上位机 - src)
        direction LR
        H_UsbRecv["USB Receive \n(src/usb_handlers.rs)"] --- H_ByteStream["Raw Byte Stream (Little Endian)"]
        H_ByteStream --- H_BinRead["AllMeasurements<5>::read_options() \n(src/binrw_impls.rs) \n**TARGET OF FIXES**"]
        H_BinRead --- H_AllMeas["AllMeasurements<5> \n(src/data_models.rs) \n(Parsed to f32/Desired Enums)"]
    end

    F_UsbSend -.-> H_UsbRecv

    classDef error fill:#f99,stroke:#333,stroke-width:2px;
    class H_BinRead error;
```

---

此计划旨在全面解决已识别的不一致问题。

---

# 计划：将 AllMeasurementsUsbPayload 字段转换为物理单位

## 1. 目标

修改 `AllMeasurementsUsbPayload` 结构体，确保通过USB发送的测量值都具有明确的物理单位，并正确处理依赖于运行时配置（如感应电阻值、NTC热敏电阻参数）的数据转换。

## 2. 核心需求与分析回顾

* **BQ25730 ADC值**:
  * 电压值 (VBAT, VSYS, VBUS, CMPIN, PSYS) 在 `Adc*` 结构体中已经是 `u16` 类型的mV。Payload中应直接使用这些mV值。
  * 电流值 (ICHG, IDCHG, IIN) 在 `Adc*` 结构体中已经是 `u16` 类型的mA，这些值是在 `bq25730_task` 中根据当时的运行时 `rsns_bat` 和 `rsns_ac` 配置计算得出的。Payload中应直接使用这些mA值。
* **BQ76920 温度值**:
  * 原始ADC值需要转换为 0.01 °C。如果使用外部热敏电阻，转换依赖于运行时的 `NtcParameters`。
* **BQ76920 电流/电压**: Payload中已有的 `bq76920_current_ma` 和 `bq76920_cellX_mv` 单位正确，予以保留。
* **状态和告警字段**: 保留为原始的 `u16` 或 `u8` 位掩码/标志值，但字段名应更新以明确其含义 (例如 `_flags`, `_mask`)。
* **INA226 值**: Payload中已有的 `ina226_*_f32` 字段单位和类型正确，予以保留。
* **运行时配置**: `rsns_bat`, `rsns_ac` (BQ25730) 和 `NtcParameters` (BQ76920) 是动态配置的，需要从各自的设备任务传递给 `usb_task` 以便在 `convert_to_payload` 中正确处理（特别是BQ76920温度转换）。

## 3. 详细修改计划

### 第一步：修改 `AllMeasurementsUsbPayload` 结构体定义

**文件**: `device/src/data_types.rs`

**操作**:
重命名字段并调整类型以反映物理单位。状态/告警字段重命名以提高清晰度。

```rust
// device/src/data_types.rs
// ... (其他代码) ...

#[derive(Debug, Copy, Clone, PartialEq, binrw::BinWrite, defmt::Format)]
pub struct AllMeasurementsUsbPayload {
    // Fields from Bq25730Measurements -> AdcMeasurements
    pub bq25730_adc_vbat_mv: u16,       // Was bq25730_adc_vbat_raw, unit: mV
    pub bq25730_adc_vsys_mv: u16,       // Was bq25730_adc_vsys_raw, unit: mV
    pub bq25730_adc_ichg_ma: u16,       // Was bq25730_adc_ichg_raw, unit: mA
    pub bq25730_adc_idchg_ma: u16,      // Was bq25730_adc_idchg_raw, unit: mA
    pub bq25730_adc_iin_ma: u16,        // Was bq25730_adc_iin_raw, unit: mA
    pub bq25730_adc_psys_mv: u16,       // Was bq25730_adc_psys_raw, unit: mV (represents power related voltage)
    pub bq25730_adc_vbus_mv: u16,       // Was bq25730_adc_vbus_raw, unit: mV
    pub bq25730_adc_cmpin_mv: u16,      // Was bq25730_adc_cmpin_raw, unit: mV

    // Fields from Bq76920Measurements -> Bq76920CoreMeasurements<N>
    pub bq76920_cell1_mv: i32,         // Unchanged
    pub bq76920_cell2_mv: i32,         // Unchanged
    pub bq76920_cell3_mv: i32,         // Unchanged
    pub bq76920_cell4_mv: i32,         // Unchanged
    pub bq76920_cell5_mv: i32,         // Unchanged (assuming N=5 for this example)
    pub bq76920_ts1_temp_0_01c: i16,    // Was bq76920_ts1_raw_adc, unit: 0.01 °C
    pub bq76920_ts2_present: u8,       // Unchanged
    pub bq76920_ts2_temp_0_01c: i16,    // Was bq76920_ts2_raw_adc, unit: 0.01 °C (use i16::MIN if not present)
    pub bq76920_ts3_present: u8,       // Unchanged
    pub bq76920_ts3_temp_0_01c: i16,    // Was bq76920_ts3_raw_adc, unit: 0.01 °C (use i16::MIN if not present)
    pub bq76920_is_thermistor: u8,     // Unchanged
    pub bq76920_current_ma: i32,       // Unchanged

    pub bq76920_system_status_mask: u8,    // Was bq76920_system_status_bits
    pub bq76920_mos_status_mask: u8,       // Was bq76920_mos_status_bits

    // Fields from Ina226Measurements
    pub ina226_voltage_f32: f32,       // Unchanged
    pub ina226_current_f32: f32,       // Unchanged
    pub ina226_power_f32: f32,         // Unchanged

    // Fields from Bq25730Alerts
    pub bq25730_charger_status_flags: u16, // Was bq25730_charger_status_raw_u16
    pub bq25730_prochot_status_flags: u16, // Was bq25730_prochot_status_raw_u16

    // Fields from Bq76920Alerts
    pub bq76920_alerts_system_status_mask: u8, // Was bq76920_alerts_system_status_bits
}
```

### 第二步：定义运行时配置结构体

**文件**: `device/src/shared.rs` (或新的 `config.rs`)

**操作**:
创建用于传递 BQ25730 和 BQ76920 运行时特定配置的结构体。

```rust
// device/src/shared.rs

// Add these structs (ensure necessary imports for SenseResistorValue and NtcParameters)
#[derive(Clone, Copy, Debug, defmt::Format, PartialEq)]
pub struct Bq25730RuntimeConfig {
    pub rsns_bat: bq25730_async_rs::data_types::SenseResistorValue,
    pub rsns_ac: bq25730_async_rs::data_types::SenseResistorValue,
}

impl Default for Bq25730RuntimeConfig {
    fn default() -> Self {
        Self {
            rsns_bat: bq25730_async_rs::data_types::SenseResistorValue::R5mOhm, // Example
            rsns_ac: bq25730_async_rs::data_types::SenseResistorValue::R10mOhm, // Example
        }
    }
}

#[derive(Clone, Copy, Debug, defmt::Format, PartialEq)]
pub struct Bq76920RuntimeConfig {
    pub ntc_params: Option<bq769x0_async_rs::data_types::NtcParameters>,
    // pub rsense_mohm: u32, // If needed for direct current calculation in USB payload later
}

impl Default for Bq76920RuntimeConfig {
    fn default() -> Self {
        Self { ntc_params: None }
    }
}
// Note: NtcParameters might need to implement defmt::Format or be handled carefully if it doesn't.
// If NtcParameters is large or complex, consider only passing what's essential or a reference.
```

### 第三步：为运行时配置设置 Pub/Sub 通道

**文件**: `device/src/shared.rs`

**操作**:
为新的配置结构体添加 `Channel`、`Publisher` 和 `Subscriber` 类型，并更新 `init_pubsubs()`。

```rust
// device/src/shared.rs
// ... (existing pubsubs) ...

pub static BQ25730_RUNTIME_CONFIG_CHANNEL: Channel<CriticalSectionRawMutex, Bq25730RuntimeConfig, 1, 1, 1> = Channel::new();
pub type Bq25730RuntimeConfigPublisher<'a> = Publisher<'a, CriticalSectionRawMutex, Bq25730RuntimeConfig, 1, 1, 1>;
pub type Bq25730RuntimeConfigSubscriber<'a> = Subscriber<'a, CriticalSectionRawMutex, Bq25730RuntimeConfig, 1, 1, 1>;

pub static BQ76920_RUNTIME_CONFIG_CHANNEL: Channel<CriticalSectionRawMutex, Bq76920RuntimeConfig, 1, 1, 1> = Channel::new();
pub type Bq76920RuntimeConfigPublisher<'a> = Publisher<'a, CriticalSectionRawMutex, Bq76920RuntimeConfig, 1, 1, 1>;
pub type Bq76920RuntimeConfigSubscriber<'a> = Subscriber<'a, CriticalSectionRawMutex, Bq76920RuntimeConfig, 1, 1, 1>;

#[allow(clippy::type_complexity)]
pub fn init_pubsubs() -> (
    // ... existing returned types ...
    Bq25730RuntimeConfigPublisher<'static>,
    &'static Channel<CriticalSectionRawMutex, Bq25730RuntimeConfig, 1, 1, 1>,
    Bq76920RuntimeConfigPublisher<'static>,
    &'static Channel<CriticalSectionRawMutex, Bq76920RuntimeConfig, 1, 1, 1>,
) {
    // ... existing initializations ...
    (
        // ... existing returned values ...
        BQ25730_RUNTIME_CONFIG_CHANNEL.publisher().unwrap(),
        &BQ25730_RUNTIME_CONFIG_CHANNEL,
        BQ76920_RUNTIME_CONFIG_CHANNEL.publisher().unwrap(),
        &BQ76920_RUNTIME_CONFIG_CHANNEL,
    )
}
```

### 第四步：从设备任务发布运行时配置

**文件**: `device/src/bq25730_task.rs`
**操作**:
修改 `bq25730_task` 接收 `Bq25730RuntimeConfigPublisher` 并在初始化后发布配置。

```rust
// device/src/bq25730_task.rs
// ... imports ...
use crate::shared::Bq25730RuntimeConfigPublisher; // Add this

#[embassy_executor::task]
pub async fn bq25730_task(
    i2c_bus: I2cDevice<'static, CriticalSectionRawMutex, I2c<'static, embassy_stm32::mode::Async>>,
    address: u8,
    bq25730_alerts_publisher: Bq25730AlertsPublisher<'static>,
    bq25730_measurements_publisher: Bq25730MeasurementsPublisher<'static>,
    mut bq76920_measurements_subscriber: Bq76920MeasurementsSubscriber<'static, 5>,
    bq25730_runtime_config_publisher: Bq25730RuntimeConfigPublisher<'static>, // <-- Add this
) {
    // ... (task setup) ...
    if let Err(e) = bq25730.init().await {
        error!("Failed to initialize BQ25730: {:?}", e);
        return;
    }

    // Publish runtime config after init
    let current_chip_config = bq25730.config(); // Assuming Bq25730 struct has a method to get its current config
    let runtime_conf = crate::shared::Bq25730RuntimeConfig {
        rsns_bat: current_chip_config.rsns_bat,
        rsns_ac: current_chip_config.rsns_ac,
    };
    bq25730_runtime_config_publisher.publish_immediate(runtime_conf);
    info!("[BQ25730] Published runtime config: {:?}", runtime_conf);

    // ... (rest of the task loop) ...
}
```

**文件**: `device/src/bq76920_task.rs` (假设存在类似结构)
**操作**:
类似地修改 `bq76920_task` 以接收 `Bq76920RuntimeConfigPublisher` 并在其 `NtcParameters` (和 `rsense_mohm` 如果需要) 确定后发布 `Bq76920RuntimeConfig`。

### 第五步：在 `usb_task` 中消费运行时配置

**文件**: `device/src/usb/mod.rs`
**操作**:
修改 `usb_task` 接收配置 subscribers，并在任务开始时等待初始配置。

```rust
// device/src/usb/mod.rs
// ... imports ...
use crate::shared::{
    Bq25730RuntimeConfigSubscriber, Bq76920RuntimeConfigSubscriber, // Add these
    // ... other subscribers ...
    // RuntimeConfig, // Assuming a combined struct for convenience, or pass separately
};


#[embassy_executor::task]
pub async fn usb_task(
    driver: usb::Driver<'static, peripherals::USB>,
    measurements_publisher: MeasurementsPublisher<'static, 5>,
    mut bq25730_measurements_subscriber: Bq25730MeasurementsSubscriber<'static>,
    mut ina226_measurements_subscriber: Ina226MeasurementsSubscriber<'static>,
    mut bq76920_measurements_subscriber: Bq76920MeasurementsSubscriber<'static, 5>,
    mut bq25730_alerts_subscriber: Bq25730AlertsSubscriber<'static>,
    mut bq76920_alerts_subscriber: Bq76920AlertsSubscriber<'static>,
    mut bq25730_runtime_config_subscriber: Bq25730RuntimeConfigSubscriber<'static>, // <-- Add
    mut bq76920_runtime_config_subscriber: Bq76920RuntimeConfigSubscriber<'static>, // <-- Add
) {
    // ... (USB setup) ...

    let main_usb_processing_fut = async {
        // ... (latest measurement variables) ...

        // Wait for initial runtime configurations
        let bq25730_conf = bq25730_runtime_config_subscriber.next_message_pure().await;
        defmt::info!("[USB Task] Received BQ25730 runtime config: {:?}", bq25730_conf);

        let bq76920_conf = bq76920_runtime_config_subscriber.next_message_pure().await;
        defmt::info!("[USB Task] Received BQ76920 runtime config: {:?}", bq76920_conf);

        loop {
            // ... (select logic for measurements and commands) ...

            // When calling convert_to_payload:
            // let command_payload = convert_to_payload(&aggregated_data, &bq25730_conf, &bq76920_conf);
            // let status_update_payload = convert_to_payload(&aggregated_data, &bq25730_conf, &bq76920_conf);
        }
    };
    // ... (rest of usb_task) ...
}
```

### 第六步：更新 `convert_to_payload` 函数

**文件**: `device/src/usb/mod.rs`
**操作**:
修改函数签名以接收运行时配置，并使用它们进行正确的转换。

```rust
// device/src/usb/mod.rs

// Assuming Bq25730RuntimeConfig and Bq76920RuntimeConfig are in scope
// (e.g. use crate::shared::{Bq25730RuntimeConfig, Bq76920RuntimeConfig};)
fn convert_to_payload(
    data: &AllMeasurements<5>,
    bq25730_conf: &crate::shared::Bq25730RuntimeConfig, // Use the actual type
    bq76920_conf: &crate::shared::Bq76920RuntimeConfig, // Use the actual type
) -> crate::data_types::AllMeasurementsUsbPayload {
    // BQ25730 Voltages (already in mV in data.bq25730.adc_measurements)
    let bq25730_adc_vbat_mv = data.bq25730.adc_measurements.vbat.0;
    let bq25730_adc_vsys_mv = data.bq25730.adc_measurements.vsys.0;
    // BQ25730 Currents (already in mA in data.bq25730.adc_measurements)
    let bq25730_adc_ichg_ma = data.bq25730.adc_measurements.ichg.milliamps;
    let bq25730_adc_idchg_ma = data.bq25730.adc_measurements.idchg.milliamps;
    let bq25730_adc_iin_ma = data.bq25730.adc_measurements.iin.milliamps;
    // ... other BQ25730 fields ...
    let bq25730_adc_psys_mv = data.bq25730.adc_measurements.psys.0;
    let bq25730_adc_vbus_mv = data.bq25730.adc_measurements.vbus.0;
    let bq25730_adc_cmpin_mv = data.bq25730.adc_measurements.cmpin.0;


    // BQ76920 Temperatures
    let temp_data_result = data
        .bq76920
        .core_measurements
        .temperatures
        .into_temperature_data(bq76920_conf.ntc_params.as_ref()); // Pass Option<&NtcParameters>

    let (ts1_temp_0_01c, ts2_temp_0_01c, ts3_temp_0_01c) = match temp_data_result {
        Ok(td) => (
            td.ts1,
            td.ts2.unwrap_or(i16::MIN), // Use a sentinel for None, e.g., i16::MIN
            td.ts3.unwrap_or(i16::MIN),
        ),
        Err(_e) => {
            defmt::warn!("Failed to convert BQ76920 temp data for USB payload: {:?}", _e);
            (i16::MIN, i16::MIN, i16::MIN) // Default to sentinel on error
        }
    };

    crate::data_types::AllMeasurementsUsbPayload {
        bq25730_adc_vbat_mv,
        bq25730_adc_vsys_mv,
        bq25730_adc_ichg_ma,
        bq25730_adc_idchg_ma,
        bq25730_adc_iin_ma,
        bq25730_adc_psys_mv,
        bq25730_adc_vbus_mv,
        bq25730_adc_cmpin_mv,

        bq76920_cell1_mv: data.bq76920.core_measurements.cell_voltages.voltages[0],
        bq76920_cell2_mv: data.bq76920.core_measurements.cell_voltages.voltages[1],
        bq76920_cell3_mv: data.bq76920.core_measurements.cell_voltages.voltages[2],
        bq76920_cell4_mv: data.bq76920.core_measurements.cell_voltages.voltages[3],
        bq76920_cell5_mv: data.bq76920.core_measurements.cell_voltages.voltages[4], // Assuming N=5
        bq76920_ts1_temp_0_01c: ts1_temp_0_01c,
        bq76920_ts2_present: data.bq76920.core_measurements.temperatures.ts2.is_some() as u8,
        bq76920_ts2_temp_0_01c: ts2_temp_0_01c,
        bq76920_ts3_present: data.bq76920.core_measurements.temperatures.ts3.is_some() as u8,
        bq76920_ts3_temp_0_01c: ts3_temp_0_01c,
        bq76920_is_thermistor: data.bq76920.core_measurements.temperatures.is_thermistor as u8,
        bq76920_current_ma: data.bq76920.core_measurements.current,
        bq76920_system_status_mask: data.bq76920.core_measurements.system_status.0.bits(),
        bq76920_mos_status_mask: data.bq76920.core_measurements.mos_status.0.bits(),

        ina226_voltage_f32: data.ina226.voltage,
        ina226_current_f32: data.ina226.current,
        ina226_power_f32: data.ina226.power,

        bq25730_charger_status_flags: data.bq25730_alerts.charger_status.to_u16(),
        bq25730_prochot_status_flags: data.bq25730_alerts.prochot_status.to_u16(),

        bq76920_alerts_system_status_mask: data.bq76920_alerts.system_status.0.bits(),
    }
}
```

### 第七步：更新 `main.rs`

**文件**: `device/src/main.rs`
**操作**:
在 `main` 函数中，从 `init_pubsubs()` 获取新的配置 publishers 和 subscribers，并将它们正确传递给 `bq25730_task`, `bq76920_task` 和 `usb_task`。

```rust
// device/src/main.rs
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // ... (heap init) ...

    let (
        measurements_publisher,
        _measurements_channel,
        bq25730_alerts_publisher,
        bq25730_alerts_channel,
        bq76920_alerts_publisher,
        bq76920_alerts_channel,
        bq25730_measurements_publisher,
        bq25730_measurements_channel,
        bq76920_measurements_publisher,
        bq76920_measurements_channel,
        ina226_measurements_publisher,
        ina226_measurements_channel,
        // Add new config pub/sub
        bq25730_runtime_config_publisher,
        bq25730_runtime_config_channel,
        bq76920_runtime_config_publisher,
        bq76920_runtime_config_channel,
    ) = shared::init_pubsubs();

    // ... (STM32 init, USB driver init) ...

    spawner
        .spawn(usb::usb_task(
            usb_driver,
            measurements_publisher,
            bq25730_measurements_channel.subscriber().unwrap(),
            ina226_measurements_channel.subscriber().unwrap(),
            bq76920_measurements_channel.subscriber().unwrap(),
            bq25730_alerts_channel.subscriber().unwrap(),
            bq76920_alerts_channel.subscriber().unwrap(),
            bq25730_runtime_config_channel.subscriber().unwrap(), // <-- Pass subscriber
            bq76920_runtime_config_channel.subscriber().unwrap(), // <-- Pass subscriber
        ))
        .unwrap();

    // ... (I2C init) ...

    spawner
        .spawn(bq25730_task::bq25730_task(
            I2cDevice::new(i2c_bus_mutex),
            bq25730_address,
            bq25730_alerts_publisher,
            bq25730_measurements_publisher,
            bq76920_measurements_channel.subscriber().unwrap(),
            bq25730_runtime_config_publisher, // <-- Pass publisher
        ))
        .unwrap();

    // ... (ina226_task spawn) ...

    spawner
        .spawn(bq76920_task::bq76920_task(
            bq76920_i2c_bus,
            bq76920_address,
            bq76920_alerts_publisher,
            bq76920_measurements_publisher,
            bq76920_runtime_config_publisher, // <-- Pass publisher for BQ76920
        ))
        .unwrap();
    // Note: bq76920_task needs to be updated to accept and use Bq76920RuntimeConfigPublisher

    // ... (rest of main) ...
}
```

*确保 `bq76920_task` 也被更新以接收和使用其配置 publisher。*

### 第八步：测试和验证

* 编译固件。
* 使用上位机（如果可用）或USB分析工具检查 `AllMeasurementsUsbPayload` 的内容。
* 验证所有字段是否具有正确的物理单位和值。
* 特别测试依赖于运行时配置的转换（BQ76920温度，如果使用外部热敏电阻）。

---
