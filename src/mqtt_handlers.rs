use std::time::Duration;

use log::{debug, error, info};
use rumqttc::{AsyncClient, Event, MqttOptions, QoS, Transport};
use serde_json;

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
    topic: &str,
    measurements: AllMeasurements<5>,
) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::to_string(&measurements)?;
    client
        .publish(topic, QoS::AtLeastOnce, false, payload)
        .await?;
    info!("已发布 MQTT 消息到主题 '{}'", topic);
    Ok(())
}