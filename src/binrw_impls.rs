use binrw::{BinRead, BinResult, BinWrite, io::{Read, Seek, Write}, Endian};
use super::data_models::{AllMeasurements, Bq25730Measurements, Bq76920Measurements, Temperatures, SystemStatus, MosStatus};
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

        // System Status (u8 for each boolean flag)
        let system_status_cc_ready_raw = u8::read_options(reader, Endian::Big, args)?;
        let system_status_ovr_temp_raw = u8::read_options(reader, Endian::Big, args)?;
        let system_status_uv_raw = u8::read_options(reader, Endian::Big, args)?;
        let system_status_ov_raw = u8::read_options(reader, Endian::Big, args)?;
        let system_status_scd_raw = u8::read_options(reader, Endian::Big, args)?;
        let system_status_ocd_raw = u8::read_options(reader, Endian::Big, args)?;
        let system_status_cuv_raw = u8::read_options(reader, Endian::Big, args)?;
        let system_status_cov_raw = u8::read_options(reader, Endian::Big, args)?;

        // Mos Status (u8 for each boolean flag)
        let mos_status_charge_on_raw = u8::read_options(reader, Endian::Big, args)?;
        let mos_status_discharge_on_raw = u8::read_options(reader, Endian::Big, args)?;

        Ok(Self {
            bq25730: Bq25730Measurements {
                psys: AdcPsys::from_register_value(bq25730_psys_raw as u8).0 as f32, // 强制转换为 u8
                vbus: AdcVbus::from_register_value(bq25730_vbus_raw as u8).0 as f32,
                idchg: AdcIdchg::from_register_value(bq25730_idchg_raw as u8).0 as f32,
                ichg: AdcIchg::from_register_value(bq25730_ichg_raw as u8).0 as f32,
                cmpin: AdcCmpin::from_register_value(bq25730_cmpin_raw as u8).0 as f32,
                iin: AdcIin::from_register_value(bq25730_iin_raw as u8).0 as f32,
                vbat: AdcVbat::from_register_value(bq25730_vbat_raw as u8, 2880).0 as f32,
                vsys: AdcVsys::from_register_value(bq25730_vsys_raw as u8, 2880).0 as f32,
            },
            bq76920: Bq76920Measurements {
                cell_voltages: {
                    let mut voltages = [0.0f32; N];
                    for i in 0..N {
                        voltages[i] = cell_voltages_raw[i];
                    }
                    voltages
                },
                temperatures: Temperatures {
                    ts1: temperatures_ts1_raw - 273.15, // 将开尔文转换为摄氏度
                    is_thermistor: temperatures_is_thermistor,
                },
                coulomb_counter: current_raw, // 直接使用 f32
                system_status: SystemStatus::from_bits_truncate(
                    (system_status_cc_ready_raw << 7)
                        | (system_status_ovr_temp_raw << 6)
                        | (system_status_uv_raw << 5)
                        | (system_status_ov_raw << 4)
                        | (system_status_scd_raw << 3)
                        | (system_status_ocd_raw << 2)
                        | (system_status_cuv_raw << 1)
                        | system_status_cov_raw,
                ),
                mos_status: match (mos_status_charge_on_raw != 0, mos_status_discharge_on_raw != 0) {
                    (false, false) => MosStatus::BothOff,
                    (true, false) => MosStatus::ChargeOn,
                    (false, true) => MosStatus::DischargeOn,
                    (true, true) => MosStatus::BothOn,
                },
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
        // BQ25730 Measurements (u16)
        ((self.bq25730.psys / AdcPsys::LSB_MV as f32) as u8).write_options(writer, Endian::Big, args)?;
        ((self.bq25730.vbus / AdcVbus::LSB_MV as f32) as u8).write_options(writer, Endian::Big, args)?;
        ((self.bq25730.idchg / AdcIdchg::LSB_MA as f32) as u8).write_options(writer, Endian::Big, args)?;
        ((self.bq25730.ichg / AdcIchg::LSB_MA as f32) as u8).write_options(writer, Endian::Big, args)?;
        ((self.bq25730.cmpin / AdcCmpin::LSB_MV as f32) as u8).write_options(writer, Endian::Big, args)?;
        ((self.bq25730.iin / AdcIin::LSB_MA as f32) as u8).write_options(writer, Endian::Big, args)?;
        ((self.bq25730.vbat / AdcVbat::LSB_MV as f32) as u8).write_options(writer, Endian::Big, args)?;
        ((self.bq25730.vsys / AdcVsys::LSB_MV as f32) as u8).write_options(writer, Endian::Big, args)?;

        // Cell Voltages (f32)
        for i in 0..N {
            self.bq76920.cell_voltages[i].write_options(writer, Endian::Big, args)?;
        }

        // Temperatures (f32 for ts1, u8 for is_thermistor)
        self.bq76920.temperatures.ts1.write_options(writer, Endian::Big, args)?;
        (self.bq76920.temperatures.is_thermistor as u8).write_options(writer, Endian::Big, args)?;

        // Current (f32)
        self.bq76920.coulomb_counter.write_options(writer, Endian::Big, args)?;

        // System Status (u8 for each boolean flag)
        (self.bq76920.system_status.bits() as u8).write_options(writer, Endian::Big, args)?; // 写入 SystemStatus 的原始 bits

        // Mos Status (u8 for each boolean flag)
        (match self.bq76920.mos_status { // 写入 MosStatus
            MosStatus::BothOff => 0u8,
            MosStatus::ChargeOn => 1u8,
            MosStatus::DischargeOn => 2u8,
            MosStatus::BothOn => 3u8,
            MosStatus::Unknown => 4u8, // 添加 Unknown 变体
        }).write_options(writer, Endian::Big, args)?;

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