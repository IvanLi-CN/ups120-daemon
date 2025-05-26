use binrw::{BinRead, BinResult, BinWrite, io::{Read, Seek, Write}};
use super::data_models::{AllMeasurements, Bq25730Measurements, Bq76920Measurements, Temperatures};
use super::usb_types::{AdcMeasurementsRaw, CellVoltagesRaw, CoulombCounterRaw};

// Manual implementation of BinRead and BinWrite for AllMeasurements
impl<const N: usize> BinRead for AllMeasurements<N> {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> BinResult<Self> {
        let bq25730_psys_raw = u8::read_options(reader, endian, args)?;
        let bq25730_vbus_raw = u8::read_options(reader, endian, args)?;
        let bq25730_idchg_raw = u8::read_options(reader, endian, args)?;
        let bq25730_ichg_raw = u8::read_options(reader, endian, args)?;
        let bq25730_cmpin_raw = u8::read_options(reader, endian, args)?;
        let bq25730_iin_raw = u8::read_options(reader, endian, args)?;
        let bq25730_vbat_raw = u8::read_options(reader, endian, args)?;
        let bq25730_vsys_raw = u8::read_options(reader, endian, args)?;

        let mut cell_voltages_raw = [0.0f32; N]; // 原始数据仍然是 f32
        for i in 0..N {
            cell_voltages_raw[i] = f32::read_options(reader, endian, args)?;
        }

        let temperatures_ts1_raw = f32::read_options(reader, endian, args)?; // 原始数据仍然是 f32
        let temperatures_is_thermistor_raw = u8::read_options(reader, endian, args)?;
        let temperatures_is_thermistor = temperatures_is_thermistor_raw != 0;

        let coulomb_counter_raw_cc = i16::read_options(reader, endian, args)?;

        Ok(Self {
            bq25730: Bq25730Measurements {
                psys: bq25730_psys_raw as f32, // 假设需要转换为 f32
                vbus: bq25730_vbus_raw as f32,
                idchg: bq25730_idchg_raw as f32,
                ichg: bq25730_ichg_raw as f32,
                cmpin: bq25730_cmpin_raw as f32,
                iin: bq25730_iin_raw as f32, // 修复拼写错误
                vbat: bq25730_vbat_raw as f32,
                vsys: bq25730_vsys_raw as f32,
            },
            bq76920: Bq76920Measurements {
                cell_voltages: {
                    let mut voltages = [0.0f32; N]; // 修正为原始类型
                    for i in 0..N {
                        voltages[i] = cell_voltages_raw[i]; // 直接赋值
                    }
                    voltages
                },
                temperatures: Temperatures {
                    ts1: temperatures_ts1_raw, // 修正为原始类型
                    is_thermistor: temperatures_is_thermistor,
                },
                coulomb_counter: coulomb_counter_raw_cc,
            },
        })
    }
}

impl<const N: usize> BinWrite for AllMeasurements<N> {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(
        &self,
        writer: &mut W,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> BinResult<()> {
        (self.bq25730.psys as u8).write_options(writer, endian, args)?;
        (self.bq25730.vbus as u8).write_options(writer, endian, args)?;
        (self.bq25730.idchg as u8).write_options(writer, endian, args)?;
        (self.bq25730.ichg as u8).write_options(writer, endian, args)?;
        (self.bq25730.cmpin as u8).write_options(writer, endian, args)?;
        (self.bq25730.iin as u8).write_options(writer, endian, args)?;
        (self.bq25730.vbat as u8).write_options(writer, endian, args)?;
        (self.bq25730.vsys as u8).write_options(writer, endian, args)?;

        for i in 0..N {
            self.bq76920.cell_voltages[i].write_options(writer, endian, args)?; // 直接写入
        }

        self.bq76920
            .temperatures
            .ts1
            .write_options(writer, endian, args)?; // 直接写入
        (self.bq76920.temperatures.is_thermistor as u8).write_options(writer, endian, args)?;

        self.bq76920
            .coulomb_counter
            .write_options(writer, endian, args)?;

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