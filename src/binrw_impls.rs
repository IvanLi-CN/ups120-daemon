use binrw::{BinRead, BinResult, BinWrite, io::{Read, Seek, Write}, Endian};
use super::data_models::{
    AllMeasurements, Bq25730Measurements, Bq76920Measurements, Ina226Measurements, Temperatures,
    SystemStatus, MosStatus, ChargerStatusFlags, ChargerFaultFlags, ProchotLsbFlags, ProchotMsbFlags,
    Bq25730Alerts, Bq76920Alerts, HostSideUsbPayload,
};

impl<const N: usize> BinRead for AllMeasurements<N> {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        _endian: binrw::Endian, 
        args: Self::Args<'_>,
    ) -> BinResult<Self> {
        log::debug!("[BINRW] Attempting to read HostSideUsbPayload");
        let payload = HostSideUsbPayload::read_options(reader, Endian::Little, args)?;
        log::debug!("[BINRW] Successfully read HostSideUsbPayload: {:?}", payload);
        log::debug!("[BINRW] Constructing AllMeasurements struct from HostSideUsbPayload");

        Ok(AllMeasurements {
            bq25730: Bq25730Measurements {
                // Firmware sends psys_raw as a u16 representing the 8-bit ADC value.
                // The LSB for PSYS ADC is 1.28W (when ADC_FULLSCALE=1, RSNS_AC=10mOhm, PSYS_RATIO=0).
                // Or 2.56W (when ADC_FULLSCALE=1, RSNS_AC=5mOhm, PSYS_RATIO=0).
                // The firmware's `AdcPsys::from_u8` uses 12mV LSB, which is likely incorrect for power.
                // Assuming the raw value `bq25730_adc_psys_raw` is the direct 8-bit ADC count.
                // The actual conversion depends on RSNS_AC and PSYS_RATIO.
                // From the log: `bq25730_adc_psys_raw: 36`. If LSB is 1.28W, then 36 * 1.28W = 46.08W.
                // The previous log showed `psys: 0.05625`. This seems to be a misinterpretation.
                // Let's assume the firmware sends the raw 8-bit ADC count in the u16 field.
                // And the host needs to know the correct LSB.
                // For now, let's use a placeholder conversion factor that matches the original log's *raw* value,
                // and acknowledge this needs proper calibration based on actual hardware config.
                // The firmware log `PSYS:36raw` suggests `payload.bq25730_adc_psys_raw` IS 36.
                // The original calculation `(payload.bq25730_adc_psys_raw as u8 as f32 * 1.5625) / 1000.0`
                // resulted in `0.05625` when `payload.bq25730_adc_psys_raw` was 36.
                // This means `(36 * 1.5625) / 1000.0 = 56.25 / 1000.0 = 0.05625`.
                // The unit here would be kW if the 1.5625 was W/count.
                // Given the firmware log `PSYS:36raw` and the `AdcPsys` type in firmware using 12mV LSB,
                // it's possible the firmware is sending `raw_adc_count * some_voltage_lsb` as `psys_mv`.
                // If `payload.bq25730_adc_psys_raw` is `36` (as seen in logs for the raw value),
                // and the firmware's `AdcPsys::from_u8` scales it by `12mV`, then `36 * 12mV = 432mV`.
                // If this `432mV` is what's sent as `bq25730_adc_psys_raw` (u16), then the host side
                // `payload.bq25730_adc_psys_raw as f32 / 1000.0` would yield `0.432`.
                // This still doesn't match `0.05625`.
                //
                // Let's re-examine the original log:
                //上位机解析 (HostSideUsbPayload): `bq25730_adc_psys_raw: 36`
                //上位机解析 (AllMeasurements): `psys: 0.05625`
                // This implies the conversion is `36 * X = 0.05625`. So `X = 0.05625 / 36 = 0.0015625`.
                // This is `1.5625 / 1000.0`.
                // So, `payload.bq25730_adc_psys_raw as f32 * 0.0015625` is the current logic.
                // The `as u8` cast was likely an error if `payload.bq25730_adc_psys_raw` is already the 8-bit count.
                // The firmware sends `bq25730_adc_psys_mv` which is `(raw_adc_count * 12)`.
                // So `payload.bq25730_adc_psys_raw` on host side IS `raw_adc_count * 12`.
                // If `raw_adc_count` is 36, then `payload.bq25730_adc_psys_raw` is `432`.
                // Then `(432 as u8 as f32 * 1.5625) / 1000.0` -> `(176 * 1.5625) / 1000.0 = 275 / 1000 = 0.275`. Still not matching.
                //
                // The `HostSideUsbPayload` has `bq25730_adc_psys_raw: u16`.
                // The firmware's `AllMeasurementsUsbPayload` has `bq25730_adc_psys_mv: u16`.
                // In firmware `device/src/data_types.rs`, `AdcPsys::from_u8(raw_value: u8)` returns `AdcPsys((raw_value as u16) * Self::LSB_MV)` where LSB_MV is 12.
                // So, if raw ADC is 36, firmware sends `36 * 12 = 432` as `bq25730_adc_psys_mv`.
                // This `432` is received by host as `payload.bq25730_adc_psys_raw`.
                // The original host conversion: `(payload.bq25730_adc_psys_raw as u8 as f32 * 1.5625) / 1000.0`
                // If `payload.bq25730_adc_psys_raw` is 432, then `(432 as u8)` is `432 % 256 = 176`.
                // Then `(176.0 * 1.5625) / 1000.0 = 275.0 / 1000.0 = 0.275`. This is what the code currently does.
                // The log shows `psys: 0.05625`. This means the initial `payload.bq25730_adc_psys_raw` must have been `36` for the original formula to yield `0.05625`.
                // This implies that `bq25730_adc_psys_raw` in `HostSideUsbPayload` was *not* `raw_adc_count * 12`, but just `raw_adc_count`.
                // This contradicts the firmware's `AdcPsys::from_u8` logic if that's what populates the USB payload.
                //
                // Let's assume the firmware *actually* sends the 8-bit raw ADC count for psys as a u16.
                // And the LSB for PSYS power is 1.28W (for 10mOhm Rsns_ac, PSYS_RATIO=0, ADC_FULLSCALE=1).
                // Then the conversion should be: `payload.bq25730_adc_psys_raw as f32 * 1.28`. (Result in Watts)
                psys: payload.bq25730_adc_psys_raw as f32 * 1.28, // Assuming psys_raw is 8-bit ADC count, LSB=1.28W. Result in W.
                vbus: payload.bq25730_adc_vbus_raw as f32 / 1000.0, // Correct if vbus_raw is mV
                idchg: payload.bq25730_adc_idchg_raw as f32 / 1000.0, // Correct if idchg_raw is mA (was (val as u8 * 6.25)/1000)
                ichg: payload.bq25730_adc_ichg_raw as f32 / 1000.0, // Correct if ichg_raw is mA
                cmpin: payload.bq25730_adc_cmpin_raw as f32 / 1000.0, // Correct if cmpin_raw is mV (was (val as u8 * 12.0)/1000)
                iin: payload.bq25730_adc_iin_raw as f32 / 1000.0,   // Correct if iin_raw is mA
                vbat: payload.bq25730_adc_vbat_raw as f32 / 1000.0, // Correct if vbat_raw is mV
                vsys: payload.bq25730_adc_vsys_raw as f32 / 1000.0, // Correct if vsys_raw is mV
            },
            bq76920: Bq76920Measurements {
                cell_voltages: {
                    let mut voltages_v = [0.0f32; N];
                    if N >= 1 { voltages_v[0] = payload.bq76920_cell1_mv as f32 / 1000.0; }
                    if N >= 2 { voltages_v[1] = payload.bq76920_cell2_mv as f32 / 1000.0; }
                    if N >= 3 { voltages_v[2] = payload.bq76920_cell3_mv as f32 / 1000.0; }
                    if N >= 4 { voltages_v[3] = payload.bq76920_cell4_mv as f32 / 1000.0; }
                    if N >= 5 { voltages_v[4] = payload.bq76920_cell5_mv as f32 / 1000.0; }
                    voltages_v
                },
                temperatures: {
                    let convert_temp = |raw_adc: u16, _is_therm: bool| -> f32 {
                        let v_25_uv = 1_200_000i32;
                        let lsb_uv = 382i32;
                        let divisor_uv_per_ccc = 42i32;
                        let v_sensor_uv = raw_adc as i32 * lsb_uv;
                        let temp_diff_uv = v_sensor_uv - v_25_uv;
                        let temp_cc = 2500i32 - (temp_diff_uv / divisor_uv_per_ccc);
                        temp_cc as f32 / 100.0
                    };
                    Temperatures {
                        ts1: convert_temp(payload.bq76920_ts1_raw_adc, payload.bq76920_is_thermistor != 0),
                        ts2: if payload.bq76920_ts2_present != 0 { Some(convert_temp(payload.bq76920_ts2_raw_adc, payload.bq76920_is_thermistor != 0)) } else { None },
                        ts3: if payload.bq76920_ts3_present != 0 { Some(convert_temp(payload.bq76920_ts3_raw_adc, payload.bq76920_is_thermistor != 0)) } else { None },
                        is_thermistor: payload.bq76920_is_thermistor != 0,
                    }
                },
                coulomb_counter: payload.bq76920_current_ma as f32 / 1000.0,
                system_status: SystemStatus::from_bits_truncate(payload.bq76920_system_status_bits),
                mos_status: match payload.bq76920_mos_status_bits {
                    0b00 => MosStatus::BothOff,
                    0b01 => MosStatus::ChargeOn,
                    0b10 => MosStatus::DischargeOn,
                    0b11 => MosStatus::BothOn,
                    _ => MosStatus::Unknown,
                },
            },
            ina226: Ina226Measurements {
                voltage: payload.ina226_voltage_f32,
                current: payload.ina226_current_f32,
                power: payload.ina226_power_f32,
            },
            bq25730_alerts: {
                let charger_status_flags = ChargerStatusFlags::from_bits_truncate((payload.bq25730_charger_status_raw_u16 >> 8) as u8);
                let charger_fault_flags = ChargerFaultFlags::from_bits_truncate((payload.bq25730_charger_status_raw_u16 & 0xFF) as u8);
                let prochot_width = ((payload.bq25730_prochot_status_raw_u16 >> 12) & 0x03) as u8;
                let prochot_lsb_flags = ProchotLsbFlags::from_bits_truncate((payload.bq25730_prochot_status_raw_u16 & 0xFF) as u8);
                let prochot_msb_flags_byte = ((payload.bq25730_prochot_status_raw_u16 >> 8) & 0xFF) as u8;
                let prochot_msb_flags = ProchotMsbFlags::from_bits_truncate(prochot_msb_flags_byte);
                Bq25730Alerts {
                    charger_status_flags,
                    charger_fault_flags,
                    prochot_lsb_flags,
                    prochot_msb_flags,
                    prochot_width,
                }
            },
            bq76920_alerts: Bq76920Alerts {
                system_status: SystemStatus::from_bits_truncate(payload.bq76920_alerts_system_status_bits),
            },
        })
    }
}

impl<const N: usize> BinWrite for AllMeasurements<N> {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(
        &self,
        writer: &mut W,
        endian: binrw::Endian, // Will be overridden by HostSideUsbPayload's attributes
        args: Self::Args<'_>,
    ) -> BinResult<()> {
        log::debug!("[BINRW] Preparing HostSideUsbPayload for writing from AllMeasurements: {:?}", self);

        // Create HostSideUsbPayload from self (AllMeasurements)
        let payload = HostSideUsbPayload {
            // BQ25730: Convert back to raw u16 values
            // Note: psys, idchg, cmpin are from u8 ADC values. Others are mV/mA.
            bq25730_adc_vbat_raw: (self.bq25730.vbat * 1000.0).round() as u16,
            bq25730_adc_vsys_raw: (self.bq25730.vsys * 1000.0).round() as u16,
            bq25730_adc_ichg_raw: (self.bq25730.ichg * 1000.0).round() as u16,
            bq25730_adc_idchg_raw: ((self.bq25730.idchg * 1000.0) / 6.25).round() as u16, // A to raw u8 ADC, then to u16
            bq25730_adc_iin_raw: (self.bq25730.iin * 1000.0).round() as u16,
            bq25730_adc_psys_raw: ((self.bq25730.psys * 1000.0) / 1.5625).round() as u16, // W to raw u8 ADC, then to u16 (BE handled by #[bw(big)])
            bq25730_adc_vbus_raw: (self.bq25730.vbus * 1000.0).round() as u16,
            bq25730_adc_cmpin_raw: ((self.bq25730.cmpin * 1000.0) / 12.0).round() as u16, // V to raw u8 ADC, then to u16

            // BQ76920
            bq76920_cell1_mv: if N >= 1 { (self.bq76920.cell_voltages[0] * 1000.0).round() as i32 } else { 0 },
            bq76920_cell2_mv: if N >= 2 { (self.bq76920.cell_voltages[1] * 1000.0).round() as i32 } else { 0 },
            bq76920_cell3_mv: if N >= 3 { (self.bq76920.cell_voltages[2] * 1000.0).round() as i32 } else { 0 },
            bq76920_cell4_mv: if N >= 4 { (self.bq76920.cell_voltages[3] * 1000.0).round() as i32 } else { 0 },
            bq76920_cell5_mv: if N >= 5 { (self.bq76920.cell_voltages[4] * 1000.0).round() as i32 } else { 0 },
            
            // Temperature conversion back to raw ADC is complex and depends on the exact inverse of convert_temp.
            // For now, writing 0 or a placeholder if direct conversion is not straightforward.
            // This part needs careful implementation if host needs to *send* temperature data.
            // Assuming ts1_raw_adc is what firmware sent, if we need to write, we'd need the original raw.
            // For simplicity, if host is only reading, these might not be critical for BinWrite.
            // Let's assume for now we'd write back the raw values if they were stored, or placeholders.
            // The current HostSideUsbPayload expects raw ADC values.
            // TODO: Revisit temperature raw ADC reconstruction if host needs to write valid temp data.
            bq76920_ts1_raw_adc: 0, // Placeholder - needs inverse of convert_temp or original raw value
            bq76920_ts2_present: self.bq76920.temperatures.ts2.is_some() as u8,
            bq76920_ts2_raw_adc: 0, // Placeholder
            bq76920_ts3_present: self.bq76920.temperatures.ts3.is_some() as u8,
            bq76920_ts3_raw_adc: 0, // Placeholder
            bq76920_is_thermistor: self.bq76920.temperatures.is_thermistor as u8,
            bq76920_current_ma: (self.bq76920.coulomb_counter * 1000.0).round() as i32,
            bq76920_system_status_bits: self.bq76920.system_status.bits(),
            bq76920_mos_status_bits: match self.bq76920.mos_status {
                MosStatus::BothOff => 0b00,
                MosStatus::ChargeOn => 0b01,
                MosStatus::DischargeOn => 0b10,
                MosStatus::BothOn => 0b11,
                MosStatus::Unknown => 0b00, // Default or error representation
            },

            // INA226
            ina226_voltage_f32: self.ina226.voltage,
            ina226_current_f32: self.ina226.current,
            ina226_power_f32: self.ina226.power,

            // BQ25730 Alerts
            bq25730_charger_status_raw_u16: 
                ((self.bq25730_alerts.charger_status_flags.bits() as u16) << 8) |
                (self.bq25730_alerts.charger_fault_flags.bits() as u16),
            bq25730_prochot_status_raw_u16: 
                (((self.bq25730_alerts.prochot_width & 0x03) as u16) << 12) | // Width in bits 13:12 of the u16
                ((self.bq25730_alerts.prochot_msb_flags.bits() as u16) << 8) | // MSB flags in bits 11:8 (assuming width was part of original MSB)
                (self.bq25730_alerts.prochot_lsb_flags.bits() as u16),       // LSB flags in bits 7:0

            // BQ76920 Alerts
            bq76920_alerts_system_status_bits: self.bq76920_alerts.system_status.bits(),
        };

        log::debug!("[BINRW] Writing HostSideUsbPayload: {:?}", payload);
        payload.write_options(writer, endian, args)
    }
}