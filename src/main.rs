use dotenv::dotenv;
use env_logger::{Builder, Target};
use log::{error, info};
use std::env;
use std::time::Duration;
use tokio::sync::mpsc;

// Ensure UsbEvent is imported correctly and data_models module is available
use ups120_daemon::{
    mqtt_handlers::*,
    usb_handlers::*,
    usb_types::{UsbCommand, UsbEvent}, // UsbEvent is defined in usb_types
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(Target::Stdout)
        .init();
    info!("上位机程序启动...");
    dotenv().ok(); // 加载 .env 文件

    let mqtt_broker_host = env::var("MQTT_BROKER_HOST").expect("MQTT_BROKER_HOST not set");
    let mqtt_broker_port: u16 = env::var("MQTT_BROKER_PORT")
        .expect("MQTT_BROKER_PORT not set")
        .parse()
        .expect("Invalid MQTT_BROKER_PORT");
    info!("MQTT 地址: {}:{}", mqtt_broker_host, mqtt_broker_port);
    let mqtt_username = env::var("MQTT_USERNAME").ok();
    let mqtt_password = env::var("MQTT_PASSWORD").ok();
    let mqtt_client_id =
        env::var("MQTT_CLIENT_ID").unwrap_or_else(|_| "ups120_cli_client".to_string());
    let mqtt_topic_prefix = env::var("MQTT_TOPIC_PREFIX").unwrap_or_else(|_| "ups120".to_string());

    let usb_vid: u16 = u16::from_str_radix(
        env::var("USB_VID")
            .unwrap_or_else(|_| "0x1209".to_string())
            .trim_start_matches("0x"),
        16,
    )
    .expect("Invalid USB_VID");
    let usb_pid: u16 = u16::from_str_radix(
        env::var("USB_PID")
            .unwrap_or_else(|_| "0x0002".to_string())
            .trim_start_matches("0x"),
        16,
    )
    .expect("Invalid USB_PID");

    let mqtt_client = loop {
        match connect_mqtt_and_publish(
            &mqtt_broker_host,
            mqtt_broker_port,
            mqtt_username.clone(),
            mqtt_password.clone(),
            &mqtt_client_id,
            &mqtt_topic_prefix,
        )
        .await
        {
            Ok(client) => break client,
            Err(e) => {
                error!("MQTT 连接失败: {:?}, 10秒后重试...", e);
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
    };

    // 创建 MPSC 渠道
    let (usb_cmd_tx, usb_cmd_rx) = mpsc::channel::<UsbCommand>(32);
    // UsbEvent itself is not generic. Its Measurements variant carries data_models::AllMeasurements<5>.
    let (usb_event_tx, mut usb_event_rx) = mpsc::channel::<UsbEvent>(32);

    // 启动 USB 管理任务
    tokio::spawn(usb_manager_task(usb_vid, usb_pid, usb_cmd_rx, usb_event_tx));

    // 主循环，处理 USB 事件和 MQTT 发布
    let main_loop_result: Result<(), Box<dyn std::error::Error>> = loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("收到 Ctrl+C 信号，正在执行优雅退出...");
                if let Err(e) = usb_cmd_tx.send(UsbCommand::Unsubscribe).await {
                    error!("发送取消订阅命令到 USB 管理任务失败: {:?}", e);
                }
                info!("程序退出。");
                break Ok(());
            }
            Some(usb_event) = usb_event_rx.recv() => {
                match usb_event {
                    // measurements_data is already of type data_models::AllMeasurements<5>
                    UsbEvent::Measurements(measurements_data) => {
                        info!("[LOG POINT 3] Received Processed Measurements: {:?}", measurements_data);

                        // No further conversion needed here as measurements_data is already the correct type.
                        // The conversion from HostSideUsbPayload to data_models::AllMeasurements<5>
                        // is assumed to happen within usb_handlers.rs before sending the UsbEvent::Measurements.

                        info!("[LOG POINT 3.5] Publishing Processed Measurements to MQTT: {:?}", measurements_data);
                        let topic = format!("{}/measurements_all", mqtt_topic_prefix);
                        if let Err(e) =
                            publish_measurements(&mqtt_client, &topic, measurements_data).await // Pass measurements_data directly
                        {
                            error!("MQTT 发布失败: {:?}, 5秒后重试...", e);
                            tokio::time::sleep(Duration::from_secs(5)).await;
                        }
                    }
                    UsbEvent::Error(e) => {
                        error!("USB 管理任务报告错误: {:?}, 尝试重新连接USB...", e);
                    }
                }
            }
            else => {
                info!("USB 事件流结束，主循环退出。");
                break Ok(());
            }
        }
    };

    main_loop_result
}
