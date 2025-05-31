use binrw::{BinRead, BinResult, BinWrite, io::{Read, Seek, Write}, Endian};
use super::data_models::{
    AllMeasurements, Bq25730Measurements, Bq76920Measurements, Ina226Measurements, Temperatures,
    SystemStatus, MosStatus, ChargerStatusFlags, ChargerFaultFlags, ProchotLsbFlags, ProchotMsbFlags,
    Bq25730Alerts, Bq76920Alerts,
};
use bq25730_async_rs::data_types::{AdcPsys, AdcVbus, AdcIdchg, AdcIchg, AdcCmpin, AdcIin, AdcVbat, AdcVsys};


// Manual implementation of BinRead and BinWrite for AllMeasurements
impl<const N: usize> BinRead for AllMeasurements<N> {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        _endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> BinResult<Self> {
        // BQ25730 Measurements (u8)
        let bq25730_psys_raw = u8::read_options(reader, Endian::Big, args)?;
        let bq25730_vbus_raw = u8::read_options(reader, Endian::Big, args)?;
        let bq25730_idchg_raw = u8::read_options(reader, Endian::Big, args)?;
        let bq25730_ichg_raw = u8::read_options(reader, Endian::Big, args)?;
        let bq25730_cmpin_raw = u8::read_options(reader, Endian::Big, args)?;
        let bq25730_iin_raw = u8::read_options(reader, Endian::Big, args)?;
        let bq25730_vbat_raw = u8::read_options(reader, Endian::Big, args)?;
        let bq25730_vsys_raw = u8::read_options(reader, Endian::Big, args)?;

        // Cell Voltages (f32)
        let mut cell_voltages_raw = [0.0f32; N];
        for i in 0..N {
            cell_voltages_raw[i] = f32::read_options(reader, Endian::Big, args)?;
        }

        // Temperatures (f32 for ts1, u8 for is_thermistor)
        let temperatures_ts1_raw = f32::read_options(reader, Endian::Big, args)?;
        let temperatures_is_thermistor_raw = u8::read_options(reader, Endian::Big, args)?;
        let temperatures_is_thermistor = temperatures_is_thermistor_raw != 0;

        // Current (f32)
        let current_raw = f32::read_options(reader, Endian::Big, args)?;

        // System Status (u8)
        let system_status_byte = u8::read_options(reader, Endian::Big, args)?;

        // Mos Status (u8)
        let mos_status_byte = u8::read_options(reader, Endian::Big, args)?;

        // INA226 Measurements (f32)
        let ina226_voltage = f32::read_options(reader, Endian::Big, args)?;
        let ina226_current = f32::read_options(reader, Endian::Big, args)?;
        let ina226_power = f32::read_options(reader, Endian::Big, args)?;

        // BQ25730 Alerts
        let bq25730_charger_status_flags_raw = u8::read_options(reader, Endian::Big, args)?;
        let bq25730_charger_fault_flags_raw = u8::read_options(reader, Endian::Big, args)?;
        let bq25730_prochot_lsb_flags_raw = u8::read_options(reader, Endian::Big, args)?;
        let bq25730_prochot_msb_flags_raw = u8::read_options(reader, Endian::Big, args)?;
        let bq25730_prochot_width = (bq25730_prochot_msb_flags_raw >> 5) & 0x03;

        // BQ76920 Alerts
        let bq76920_system_status_alerts_raw = u8::read_options(reader, Endian::Big, args)?;


        Ok(AllMeasurements { // 明确指定类型参数，让编译器推断 N
            bq25730: Bq25730Measurements {
                // Assuming AdcPsys::LSB_MV is actually mW/LSB for psys, and result of from_register_value is mW
                psys: (AdcPsys::from_u8(bq25730_psys_raw).0 as f32) / 1000.0, // Convert mW to W
                // Assuming AdcVbus::from_u8 results in mV
                vbus: (AdcVbus::from_u8(bq25730_vbus_raw).0 as f32) / 1000.0, // Convert mV to V
                // Assuming AdcIdchg::from_u8 results in mA
                idchg: (AdcIdchg::from_u8(bq25730_idchg_raw).0 as f32) / 1000.0, // Convert mA to A
                // Assuming AdcIchg::from_u8 results in mA
                ichg: (AdcIchg::from_u8(bq25730_ichg_raw).0 as f32) / 1000.0, // Convert mA to A
                // Assuming AdcCmpin::from_u8 results in mV
                cmpin: (AdcCmpin::from_u8(bq25730_cmpin_raw).0 as f32) / 1000.0, // Convert mV to V
                // Assuming AdcIin::from_u8 results in mA
                iin: (AdcIin::from_u8(bq25730_iin_raw, true).milliamps as f32) / 1000.0, // Convert mA to A
                // AdcVbat::from_register_value(_lsb: u8, msb: u8, offset_mv: u16)
                vbat: (AdcVbat::from_register_value(0, bq25730_vbat_raw, 2880).0 as f32) / 1000.0, // Convert mV to V
                vsys: (AdcVsys::from_register_value(0, bq25730_vsys_raw, 2880).0 as f32) / 1000.0, // Convert mV to V
            },
            bq76920: Bq76920Measurements {
                cell_voltages: cell_voltages_raw, // Direct assignment if types match
                temperatures: Temperatures {
                    ts1: temperatures_ts1_raw - 273.15, // 将开尔文转换为摄氏度
                    is_thermistor: temperatures_is_thermistor,
                },
                coulomb_counter: current_raw,
                system_status: SystemStatus::from_bits_truncate(system_status_byte),
                mos_status: match mos_status_byte {
                    0 => MosStatus::BothOff,
                    1 => MosStatus::ChargeOn,
                    2 => MosStatus::DischargeOn,
                    3 => MosStatus::BothOn,
                    _ => MosStatus::Unknown, // Default or error handling
                },
            },
            ina226: Ina226Measurements {
                voltage: ina226_voltage,
                current: ina226_current,
                power: ina226_power,
            },
            bq25730_alerts: Bq25730Alerts {
                charger_status_flags: ChargerStatusFlags::from_bits_truncate(bq25730_charger_status_flags_raw),
                charger_fault_flags: ChargerFaultFlags::from_bits_truncate(bq25730_charger_fault_flags_raw),
                prochot_lsb_flags: ProchotLsbFlags::from_bits_truncate(bq25730_prochot_lsb_flags_raw),
                prochot_msb_flags: ProchotMsbFlags::from_bits_truncate(bq25730_prochot_msb_flags_raw),
                prochot_width: bq25730_prochot_width,
            },
            bq76920_alerts: Bq76920Alerts {
                system_status: SystemStatus::from_bits_truncate(bq76920_system_status_alerts_raw),
            },
        })
    }
}

impl<const N: usize> BinWrite for AllMeasurements<N> {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(
        &self,
        writer: &mut W,
        _endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> BinResult<()> {
        // BQ25730 Measurements (u8)
        // Assuming self.bq25730.psys is in W, AdcPsys::LSB_MV is mW/LSB (needs verification)
        (((self.bq25730.psys * 1000.0) / AdcPsys::LSB_MV as f32).round() as u8).write_options(writer, Endian::Big, args)?;
        // Assuming self.bq25730.vbus is in V, AdcVbus::LSB_MV is mV/LSB
        (((self.bq25730.vbus * 1000.0) / AdcVbus::LSB_MV as f32).round() as u8).write_options(writer, Endian::Big, args)?;
        // Assuming self.bq25730.idchg is in A, AdcIdchg::LSB_MA is mA/LSB
        (((self.bq25730.idchg * 1000.0) / AdcIdchg::LSB_MA as f32).round() as u8).write_options(writer, Endian::Big, args)?;
        // Assuming self.bq25730.ichg is in A, AdcIchg::LSB_MA is mA/LSB
        (((self.bq25730.ichg * 1000.0) / AdcIchg::LSB_MA as f32).round() as u8).write_options(writer, Endian::Big, args)?;
        // Assuming self.bq25730.cmpin is in V, AdcCmpin::LSB_MV is mV/LSB
        (((self.bq25730.cmpin * 1000.0) / AdcCmpin::LSB_MV as f32).round() as u8).write_options(writer, Endian::Big, args)?;
        // Assuming self.bq25730.iin is in A, AdcIin::LSB_MA is mA/LSB
        (((self.bq25730.iin * 1000.0) / 100.0 as f32).round() as u8).write_options(writer, Endian::Big, args)?;
        // Assuming self.bq25730.vbat is in V. Convert V to mV, subtract offset, then divide by LSB.
        ((((self.bq25730.vbat * 1000.0) - 2880.0) / AdcVbat::LSB_MV as f32).round() as u8).write_options(writer, Endian::Big, args)?;
        ((((self.bq25730.vsys * 1000.0) - 2880.0) / AdcVsys::LSB_MV as f32).round() as u8).write_options(writer, Endian::Big, args)?;

        // BQ76920 Cell Voltages (f32)
        for i in 0..N {
            self.bq76920.cell_voltages[i].write_options(writer, Endian::Big, args)?;
        }

        // BQ76920 Temperatures (f32 for ts1, u8 for is_thermistor)
        (self.bq76920.temperatures.ts1 + 273.15).write_options(writer, Endian::Big, args)?; // Convert Celsius to Kelvin for writing if firmware expects Kelvin
        (self.bq76920.temperatures.is_thermistor as u8).write_options(writer, Endian::Big, args)?;

        // BQ76920 Current (f32)
        self.bq76920.coulomb_counter.write_options(writer, Endian::Big, args)?;

        // BQ76920 System Status (u8)
        (self.bq76920.system_status.bits()).write_options(writer, Endian::Big, args)?;

        // BQ76920 Mos Status (u8)
        (match self.bq76920.mos_status {
            MosStatus::BothOff => 0u8,
            MosStatus::ChargeOn => 1u8,
            MosStatus::DischargeOn => 2u8,
            MosStatus::BothOn => 3u8,
            MosStatus::Unknown => 4u8, // Or a default error value
        }).write_options(writer, Endian::Big, args)?;

        // INA226 Measurements (f32)
        self.ina226.voltage.write_options(writer, Endian::Big, args)?;
        self.ina226.current.write_options(writer, Endian::Big, args)?;
        self.ina226.power.write_options(writer, Endian::Big, args)?;

        // BQ25730 Alerts
        self.bq25730_alerts.charger_status_flags.bits().write_options(writer, Endian::Big, args)?;
        self.bq25730_alerts.charger_fault_flags.bits().write_options(writer, Endian::Big, args)?;
        self.bq25730_alerts.prochot_lsb_flags.bits().write_options(writer, Endian::Big, args)?;
        // Combine Prochot MSB flags and width for writing
        let prochot_msb_byte_to_write = (self.bq25730_alerts.prochot_msb_flags.bits() & !0b01100000) | ((self.bq25730_alerts.prochot_width & 0x03) << 5);
        prochot_msb_byte_to_write.write_options(writer, Endian::Big, args)?;


        // BQ76920 Alerts
        self.bq76920_alerts.system_status.bits().write_options(writer, Endian::Big, args)?;

        Ok(())
    }
}

// 复制自 device/src/usb/endpoints.rs
#[repr(u8)]
#[derive(BinRead, BinWrite, Debug, Clone)]
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