# CLI 上位机程序实现计划

## 目标

为 'device' 项目创建一个 CLI 上位机程序，该程序将通过 USB 连接设备，订阅设备消息，并将这些消息发送到 MQTT 服务。程序将从 `.env` 文件和环境变量中读取 MQTT 配置，并设计好日志打印。

## 计划概述

1.  **项目结构调整：** 确认根目录下的 `Cargo.toml` 是否适合作为上位机程序的入口。如果 `src/main.rs` 已经存在，我将修改它来包含上位机逻辑。
2.  **依赖管理：** 在根目录的 `Cargo.toml` 中添加必要的依赖，包括 `rusb` (或 `libusb-sys` 和 `libusb` 绑定)、`rumqttc`、`serde`、`serde_json`、`dotenv` 和 `env_logger`。还需要考虑 `binrw` 和 `uom` 的兼容性。
3.  **环境变量配置：** 定义上位机程序所需的环境变量，并提供 `.env` 文件示例。
4.  **USB 连接与数据读取：**
    *   使用 `rusb` 库连接到设备，使用 `USB_VID` 和 `USB_PID`。
    *   打开 USB 设备并配置端点。
    *   发送 `SubscribeStatus` 命令到设备。
    *   循环读取 `push_write_ep` 端点的数据。
5.  **数据解析：**
    *   使用 `binrw` 解析从 USB 读取的原始字节数据，将其反序列化为 `UsbData::StatusPush(AllMeasurements<5>)`。
    *   处理 `AllMeasurements` 中的 `uom` 类型，使其能够正确序列化为 JSON。这可能需要为 `uom` 类型实现自定义的 `serde::Serialize`。
6.  **MQTT 连接与发布：**
    *   从环境变量或 `.env` 文件中读取 MQTT Broker 地址、端口、用户名、密码等信息。
    *   使用 `rumqttc` 连接到 MQTT Broker。
    *   将解析后的 `AllMeasurements` 数据转换为 JSON 字符串。
    *   将 JSON 字符串发布到 MQTT 主题。
7.  **日志系统：**
    *   集成 `env_logger`，根据环境变量配置日志级别。
    *   在关键步骤（连接、数据收发、错误）打印日志。
8.  **错误处理与重试：**
    *   实现 USB 连接、数据解析和 MQTT 发布失败时的重试逻辑。
    *   重试失败后，打印详细的错误日志。

## 详细计划步骤

1.  **检查并修改根目录 `Cargo.toml`：**
    *   添加 `rusb` (或 `libusb-sys` 和 `libusb` 绑定)、`rumqttc`、`serde`、`serde_json`、`dotenv`、`env_logger` 依赖。
    *   确保 `binrw` 和 `uom` 的版本与设备端兼容，或者添加它们作为依赖。
    *   为 `AllMeasurements` 结构体添加 `serde` 相关的 derive 宏，并处理 `uom` 类型的序列化。

2.  **创建 `.env` 文件示例：**
    ```
    # MQTT 配置
    MQTT_BROKER_HOST=localhost
    MQTT_BROKER_PORT=1883
    MQTT_USERNAME=your_username
    MQTT_PASSWORD=your_password
    MQTT_CLIENT_ID=ups120_cli_client
    MQTT_TOPIC_PREFIX=ups120

    # USB 配置 (与 device/build.rs 中的默认值一致)
    USB_VID=0x1209
    USB_PID=0x0002

    # 日志配置
    RUST_LOG=info
    ```

3.  **修改 `src/main.rs` 实现上位机逻辑：**

    *   **初始化日志：**
        ```rust
        use env_logger::{Builder, Target};
        use log::{info, error, warn, debug};
        // ... 其他 use 语句

        fn main() {
            Builder::from_env(env_logger::Env::default().default_filter_or("info"))
                .target(Target::Stdout)
                .init();
            info!("上位机程序启动...");
            // ...
        }
        ```

    *   **读取环境变量：**
        ```rust
        use dotenv::dotenv;
        use std::env;

        dotenv().ok(); // 加载 .env 文件

        let mqtt_broker_host = env::var("MQTT_BROKER_HOST").expect("MQTT_BROKER_HOST not set");
        let mqtt_broker_port: u16 = env::var("MQTT_BROKER_PORT").expect("MQTT_BROKER_PORT not set").parse().expect("Invalid MQTT_BROKER_PORT");
        let mqtt_username = env::var("MQTT_USERNAME").ok();
        let mqtt_password = env::var("MQTT_PASSWORD").ok();
        let mqtt_client_id = env::var("MQTT_CLIENT_ID").unwrap_or_else(|_| "ups120_cli_client".to_string());
        let mqtt_topic_prefix = env::var("MQTT_TOPIC_PREFIX").unwrap_or_else(|_| "ups120".to_string());

        let usb_vid: u16 = u16::from_str_radix(env::var("USB_VID").unwrap_or_else(|_| "0x1209".to_string()).trim_start_matches("0x"), 16).expect("Invalid USB_VID");
        let usb_pid: u16 = u16::from_str_radix(env::var("USB_PID").unwrap_or_else(|_| "0x0002".to_string()).trim_start_matches("0x"), 16).expect("Invalid USB_PID");
        ```

    *   **USB 连接与数据收发函数：**
        *   实现一个函数 `connect_and_subscribe_usb(vid, pid)`，负责连接 USB 设备，发送订阅命令，并返回一个异步流，用于接收设备推送的数据。
        *   在这个函数中处理 USB 连接的重试逻辑。
        *   使用 `rusb::open_device_with_vid_pid`。
        *   找到正确的接口和端点。根据 `device/src/usb/endpoints.rs`，端点类型是 `interrupt`，大小是 64 字节。
        *   发送 `UsbData::SubscribeStatus` 命令。
        *   循环从 `push_write_ep` 读取数据。

    *   **MQTT 连接与发布函数：**
        *   实现一个函数 `connect_mqtt_and_publish(host, port, username, password, client_id, topic_prefix)`，负责连接 MQTT Broker 并返回一个 `rumqttc::AsyncClient`。
        *   在这个函数中处理 MQTT 连接的重试逻辑。
        *   实现一个异步函数 `publish_measurements(client, topic, measurements)`，将 `AllMeasurements` 序列化为 JSON 并发布。

    *   **主循环：**
        ```rust
        #[tokio::main] // 使用 tokio 作为异步运行时
        async fn main() -> Result<(), Box<dyn std::error::Error>> {
            // ... 日志和环境变量初始化

            let mqtt_client = loop {
                match connect_mqtt_and_publish(
                    &mqtt_broker_host,
                    mqtt_broker_port,
                    mqtt_username.clone(),
                    mqtt_password.clone(),
                    &mqtt_client_id,
                    &mqtt_topic_prefix,
                ).await {
                    Ok(client) => break client,
                    Err(e) => {
                        error!("MQTT 连接失败: {:?}, 10秒后重试...", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    }
                }
            };

            loop {
                match connect_and_subscribe_usb(usb_vid, usb_pid).await {
                    Ok(mut usb_data_stream) => {
                        info!("USB 设备连接成功，开始订阅数据...");
                        while let Some(data_result) = usb_data_stream.next().await {
                            match data_result {
                                Ok(measurements) => {
                                    let topic = format!("{}/measurements", mqtt_topic_prefix);
                                    if let Err(e) = publish_measurements(&mqtt_client, &topic, measurements).await {
                                        error!("MQTT 发布失败: {:?}, 5秒后重试...", e);
                                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                                    }
                                }
                                Err(e) => {
                                    error!("USB 数据解析失败: {:?}, 尝试重新连接USB...", e);
                                    break; // 退出当前 USB 连接循环，尝试重新连接
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("USB 连接失败: {:?}, 10秒后重试...", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    }
                }
            }
        }
        ```

## Mermaid 图示：

```mermaid
graph TD
    A[CLI 程序启动] --> B{初始化日志和环境变量};
    B --> C{连接 MQTT Broker};
    C -- 成功 --> D{连接 USB 设备};
    C -- 失败 --> C;
    D -- 成功 --> E{发送 SubscribeStatus 命令};
    E --> F{循环读取 USB 数据};
    F -- 收到数据 --> G{解析 UsbData};
    G -- 解析成功 --> H{将 AllMeasurements 转换为 JSON};
    H --> I{发布 JSON 到 MQTT};
    I -- 成功 --> F;
    I -- 失败 --> I;
    G -- 解析失败 --> D;
    F -- USB 连接断开/错误 --> D;
    D -- 失败 --> D;