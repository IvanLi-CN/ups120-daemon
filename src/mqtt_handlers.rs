use std::time::Duration;

use log::{debug, error, info};
use rumqttc::{AsyncClient, Event, MqttOptions, QoS, Transport};

use crate::data_models::{AllMeasurements, ChargerStatusFlags, ChargerFaultFlags, ProchotLsbFlags, ProchotMsbFlags, SystemStatus as Bq76920SystemStatus}; // Added specific flag types

// MQTT 连接和发布函数
pub async fn connect_mqtt_and_publish(
    host: &str,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    client_id: &str,
    _topic_prefix: &str, // 添加下划线
) -> Result<AsyncClient, Box<dyn std::error::Error>> {
    let mut mqtt_options = MqttOptions::new(client_id, host, port);
    mqtt_options.set_keep_alive(Duration::from_secs(5));
    if let Some(u) = username {
        mqtt_options.set_credentials(u, password.unwrap_or_default());
    }
    mqtt_options.set_transport(Transport::Tcp); // 默认使用 TCP

    let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10); // eventloop 声明为可变

    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(rumqttc::Packet::ConnAck(_))) => {
                    info!("MQTT 连接成功!");
                }
                Ok(Event::Incoming(rumqttc::Packet::Publish(p))) => {
                    info!("收到 MQTT 消息: {:?}", p);
                }
                Ok(Event::Outgoing(rumqttc::Outgoing::PingReq)) => {
                    debug!("MQTT PingReq");
                }
                Ok(Event::Outgoing(rumqttc::Outgoing::PingResp)) => {
                    debug!("MQTT PingResp");
                }
                Ok(event) => {
                    debug!("MQTT Event: {:?}", event);
                }
                Err(e) => {
                    error!("MQTT EventLoop 错误: {:?}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await; // 错误后等待
                }
            }
        }
    });

    Ok(client)
}

pub async fn publish_measurements(
    client: &AsyncClient,
    topic_prefix: &str,
    measurements: AllMeasurements<5>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 发布 BQ25730 测量数据
    let bq25730 = &measurements.bq25730;
    client.publish(format!("{}/bq25730/psys", topic_prefix), QoS::AtLeastOnce, false, bq25730.psys.to_string()).await?;
    client.publish(format!("{}/bq25730/vbus", topic_prefix), QoS::AtLeastOnce, false, bq25730.vbus.to_string()).await?;
    client.publish(format!("{}/bq25730/idchg", topic_prefix), QoS::AtLeastOnce, false, bq25730.idchg.to_string()).await?;
    client.publish(format!("{}/bq25730/ichg", topic_prefix), QoS::AtLeastOnce, false, bq25730.ichg.to_string()).await?;
    client.publish(format!("{}/bq25730/cmpin", topic_prefix), QoS::AtLeastOnce, false, bq25730.cmpin.to_string()).await?;
    client.publish(format!("{}/bq25730/iin", topic_prefix), QoS::AtLeastOnce, false, bq25730.iin.to_string()).await?;
    client.publish(format!("{}/bq25730/vbat", topic_prefix), QoS::AtLeastOnce, false, bq25730.vbat.to_string()).await?;
    client.publish(format!("{}/bq25730/vsys", topic_prefix), QoS::AtLeastOnce, false, bq25730.vsys.to_string()).await?;

    // 发布 BQ76920 测量数据
    let bq76920 = &measurements.bq76920;
    for (i, voltage) in bq76920.cell_voltages.iter().enumerate() {
        client.publish(format!("{}/bq76920/cell_voltages/{}", topic_prefix, i), QoS::AtLeastOnce, false, voltage.to_string()).await?;
    }
    client.publish(format!("{}/bq76920/temperatures/ts1", topic_prefix), QoS::AtLeastOnce, false, bq76920.temperatures.ts1.to_string()).await?;
    client.publish(format!("{}/bq76920/coulomb_counter", topic_prefix), QoS::AtLeastOnce, false, bq76920.coulomb_counter.to_string()).await?;
    client.publish(format!("{}/bq76920/system_status", topic_prefix), QoS::AtLeastOnce, false, format!("{:?}", bq76920.system_status)).await?; // 使用 Debug 格式化
    client.publish(format!("{}/bq76920/mos_status", topic_prefix), QoS::AtLeastOnce, false, format!("{:?}", bq76920.mos_status)).await?; // 使用 Debug 格式化

    // --- Publish BQ25730 Status ---
    let bq25730_status = &measurements.bq25730_alerts; // Renamed for clarity, still Bq25730Alerts type
    let bq25730_status_base_topic = format!("{}/bq25730/status", topic_prefix); // Changed "alerts" to "status"

    // ChargerStatusFlags
    let csf = bq25730_status.charger_status_flags;
    client.publish(format!("{}/charger/stat_ac", bq25730_status_base_topic), QoS::AtLeastOnce, false, csf.contains(ChargerStatusFlags::STAT_AC).to_string()).await?;
    client.publish(format!("{}/charger/ico_done", bq25730_status_base_topic), QoS::AtLeastOnce, false, csf.contains(ChargerStatusFlags::ICO_DONE).to_string()).await?;
    client.publish(format!("{}/charger/in_vap", bq25730_status_base_topic), QoS::AtLeastOnce, false, csf.contains(ChargerStatusFlags::IN_VAP).to_string()).await?;
    client.publish(format!("{}/charger/in_vindpm", bq25730_status_base_topic), QoS::AtLeastOnce, false, csf.contains(ChargerStatusFlags::IN_VINDPM).to_string()).await?;
    client.publish(format!("{}/charger/in_iin_dpm", bq25730_status_base_topic), QoS::AtLeastOnce, false, csf.contains(ChargerStatusFlags::IN_IIN_DPM).to_string()).await?;
    client.publish(format!("{}/charger/in_fchrg", bq25730_status_base_topic), QoS::AtLeastOnce, false, csf.contains(ChargerStatusFlags::IN_FCHRG).to_string()).await?;
    client.publish(format!("{}/charger/in_pchrg", bq25730_status_base_topic), QoS::AtLeastOnce, false, csf.contains(ChargerStatusFlags::IN_PCHRG).to_string()).await?;
    client.publish(format!("{}/charger/in_otg", bq25730_status_base_topic), QoS::AtLeastOnce, false, csf.contains(ChargerStatusFlags::IN_OTG).to_string()).await?;

    // ChargerFaultFlags
    let cff = bq25730_status.charger_fault_flags;
    client.publish(format!("{}/charger_fault/acov", bq25730_status_base_topic), QoS::AtLeastOnce, false, cff.contains(ChargerFaultFlags::FAULT_ACOV).to_string()).await?;
    client.publish(format!("{}/charger_fault/batoc", bq25730_status_base_topic), QoS::AtLeastOnce, false, cff.contains(ChargerFaultFlags::FAULT_BATOC).to_string()).await?;
    client.publish(format!("{}/charger_fault/acoc", bq25730_status_base_topic), QoS::AtLeastOnce, false, cff.contains(ChargerFaultFlags::FAULT_ACOC).to_string()).await?;
    client.publish(format!("{}/charger_fault/sysovp", bq25730_status_base_topic), QoS::AtLeastOnce, false, cff.contains(ChargerFaultFlags::FAULT_SYSOVP).to_string()).await?;
    client.publish(format!("{}/charger_fault/vsys_uvp", bq25730_status_base_topic), QoS::AtLeastOnce, false, cff.contains(ChargerFaultFlags::FAULT_VSYS_UVP).to_string()).await?;
    client.publish(format!("{}/charger_fault/conv_off", bq25730_status_base_topic), QoS::AtLeastOnce, false, cff.contains(ChargerFaultFlags::FAULT_CONV_OFF).to_string()).await?;
    client.publish(format!("{}/charger_fault/otg_ovp", bq25730_status_base_topic), QoS::AtLeastOnce, false, cff.contains(ChargerFaultFlags::FAULT_OTG_OVP).to_string()).await?;
    client.publish(format!("{}/charger_fault/otg_uvp", bq25730_status_base_topic), QoS::AtLeastOnce, false, cff.contains(ChargerFaultFlags::FAULT_OTG_UVP).to_string()).await?;

    // ProchotLsbFlags
    let plf = bq25730_status.prochot_lsb_flags;
    client.publish(format!("{}/prochot/lsb_stat_vindpm", bq25730_status_base_topic), QoS::AtLeastOnce, false, plf.contains(ProchotLsbFlags::STAT_VINDPM).to_string()).await?;
    client.publish(format!("{}/prochot/lsb_stat_comp", bq25730_status_base_topic), QoS::AtLeastOnce, false, plf.contains(ProchotLsbFlags::STAT_COMP).to_string()).await?;
    client.publish(format!("{}/prochot/lsb_stat_icrit", bq25730_status_base_topic), QoS::AtLeastOnce, false, plf.contains(ProchotLsbFlags::STAT_ICRIT).to_string()).await?;
    client.publish(format!("{}/prochot/lsb_stat_inom", bq25730_status_base_topic), QoS::AtLeastOnce, false, plf.contains(ProchotLsbFlags::STAT_INOM).to_string()).await?;
    client.publish(format!("{}/prochot/lsb_stat_idchg1", bq25730_status_base_topic), QoS::AtLeastOnce, false, plf.contains(ProchotLsbFlags::STAT_IDCHG1).to_string()).await?;
    client.publish(format!("{}/prochot/lsb_stat_vsys", bq25730_status_base_topic), QoS::AtLeastOnce, false, plf.contains(ProchotLsbFlags::STAT_VSYS).to_string()).await?;
    client.publish(format!("{}/prochot/lsb_stat_bat_removal", bq25730_status_base_topic), QoS::AtLeastOnce, false, plf.contains(ProchotLsbFlags::STAT_BAT_REMOVAL).to_string()).await?;
    client.publish(format!("{}/prochot/lsb_stat_adpt_removal", bq25730_status_base_topic), QoS::AtLeastOnce, false, plf.contains(ProchotLsbFlags::STAT_ADPT_REMOVAL).to_string()).await?;

    // ProchotMsbFlags
    let pmf = bq25730_status.prochot_msb_flags;
    client.publish(format!("{}/prochot/msb_en_prochot_ext", bq25730_status_base_topic), QoS::AtLeastOnce, false, pmf.contains(ProchotMsbFlags::EN_PROCHOT_EXT).to_string()).await?;
    client.publish(format!("{}/prochot/msb_prochot_clear", bq25730_status_base_topic), QoS::AtLeastOnce, false, pmf.contains(ProchotMsbFlags::PROCHOT_CLEAR).to_string()).await?;
    client.publish(format!("{}/prochot/msb_stat_vap_fail", bq25730_status_base_topic), QoS::AtLeastOnce, false, pmf.contains(ProchotMsbFlags::STAT_VAP_FAIL).to_string()).await?;
    client.publish(format!("{}/prochot/msb_stat_exit_vap", bq25730_status_base_topic), QoS::AtLeastOnce, false, pmf.contains(ProchotMsbFlags::STAT_EXIT_VAP).to_string()).await?;
    client.publish(format!("{}/prochot/width", bq25730_status_base_topic), QoS::AtLeastOnce, false, bq25730_status.prochot_width.to_string()).await?;

    // --- Publish BQ76920 Status ---
    let bq76920_status = &measurements.bq76920_alerts; // Renamed for clarity
    let bq76920_status_base_topic = format!("{}/bq76920/status", topic_prefix); // Changed "alerts" to "status"
    let ss = bq76920_status.system_status;
    client.publish(format!("{}/system/ocd", bq76920_status_base_topic), QoS::AtLeastOnce, false, ss.contains(Bq76920SystemStatus::OCD).to_string()).await?;
    client.publish(format!("{}/system/scd", bq76920_status_base_topic), QoS::AtLeastOnce, false, ss.contains(Bq76920SystemStatus::SCD).to_string()).await?;
    client.publish(format!("{}/system/ov", bq76920_status_base_topic), QoS::AtLeastOnce, false, ss.contains(Bq76920SystemStatus::OV).to_string()).await?;
    client.publish(format!("{}/system/uv", bq76920_status_base_topic), QoS::AtLeastOnce, false, ss.contains(Bq76920SystemStatus::UV).to_string()).await?;
    client.publish(format!("{}/system/ovrd_alert", bq76920_status_base_topic), QoS::AtLeastOnce, false, ss.contains(Bq76920SystemStatus::OVRD_ALERT).to_string()).await?;
    client.publish(format!("{}/system/device_xready", bq76920_status_base_topic), QoS::AtLeastOnce, false, ss.contains(Bq76920SystemStatus::DEVICE_XREADY).to_string()).await?;
    client.publish(format!("{}/system/cc_ready", bq76920_status_base_topic), QoS::AtLeastOnce, false, ss.contains(Bq76920SystemStatus::CC_READY).to_string()).await?;


    info!("已发布所有测量和告警数据到主题前缀 '{}'", topic_prefix);

    Ok(())
}