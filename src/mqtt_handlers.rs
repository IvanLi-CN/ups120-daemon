use std::time::Duration;

use log::{debug, error, info};
use rumqttc::{AsyncClient, Event, MqttOptions, QoS, Transport};

use crate::data_models::AllMeasurements;

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

    info!("已发布所有测量数据到主题前缀 '{}'", topic_prefix);

    Ok(())
}