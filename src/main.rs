use std::env;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use binrw::{
    BinRead, BinResult, BinWrite,
    io::{Read, Seek, Write},
};
use dotenv::dotenv;
use env_logger::{Builder, Target};
use log::{debug, error, info, warn};
use rumqttc::{AsyncClient, Event, MqttOptions, QoS, Transport};
use rusb::UsbContext; // 移除 GlobalContext
use serde::Serialize;
use serde::ser::{SerializeSeq, SerializeStruct};
use tokio::sync::mpsc; // 导入 mpsc

// 复制自 device/src/shared.rs
// 移除 uom 相关的 use 语句

// BQ25730 测量数据 (简化，只包含需要序列化的字段)
#[derive(Debug, Copy, Clone, PartialEq, Serialize)]
pub struct Bq25730Measurements {
    pub psys: f32,
    pub vbus: f32,
    pub idchg: f32,
    pub ichg: f32,
    pub cmpin: f32,
    pub iin: f32,
    pub vbat: f32,
    pub vsys: f32,
}

// BQ76920 测量数据 (简化，只包含需要序列化的字段)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bq76920Measurements<const N: usize> {
    #[serde(serialize_with = "serialize_voltages")]
    pub cell_voltages: [f32; N], // 修正为原始类型
    #[serde(serialize_with = "serialize_temperatures")]
    pub temperatures: Temperatures,
    pub coulomb_counter: i16,
}

// Temperatures 结构体 (简化)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Temperatures {
    #[serde(serialize_with = "serialize_thermodynamic_temperature")]
    pub ts1: f32, // 修正为原始类型
    pub is_thermistor: bool,
}

// AllMeasurements 聚合所有设备的测量数据
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AllMeasurements<const N: usize> {
    pub bq25730: Bq25730Measurements,
    pub bq76920: Bq76920Measurements<N>,
}

// 为 ElectricPotential 实现自定义序列化
#[allow(dead_code)] // 添加此行
fn serialize_electric_potential<S>(
    value: &f32, // 修正为原始类型
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_f32(*value)
}

// 为 ThermodynamicTemperature 实现自定义序列化
fn serialize_thermodynamic_temperature<S>(
    value: &f32, // 修正为原始类型
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_f32(*value)
}

// 为 [f32] 实现自定义序列化
fn serialize_voltages<S, const N: usize>(
    voltages: &[f32; N], // 修正为原始类型
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut seq = serializer.serialize_seq(Some(N))?;
    for voltage in voltages {
        seq.serialize_element(voltage)?; // 直接序列化
    }
    seq.end()
}

// 为 Temperatures 实现自定义序列化
fn serialize_temperatures<S>(temperatures: &Temperatures, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut state = serializer.serialize_struct("Temperatures", 2)?;
    state.serialize_field("ts1", &temperatures.ts1)?; // 直接序列化
    state.serialize_field("is_thermistor", &temperatures.is_thermistor)?;
    state.end()
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

// USB 命令枚举
#[derive(Debug)]
pub enum UsbCommand {
    Unsubscribe,
}

// USB 事件枚举
#[derive(Debug)]
pub enum UsbEvent {
    Measurements(AllMeasurements<5>),
    Error(Box<dyn std::error::Error + Send + 'static>), // 添加 'static 生命周期
}

// 简化版的 AdcMeasurements 和 CellVoltages，仅用于 BinRead/BinWrite
// 实际的转换逻辑在 AllMeasurements 的 BinRead 实现中处理
#[allow(dead_code)] // 添加此行
struct AdcMeasurementsRaw {
    psys: u8,
    vbus: u8,
    idchg: u8,
    ichg: u8,
    cmpin: u8,
    iin: u8,
    vbat: u8,
    vsys: u8,
}

#[allow(dead_code)] // 添加此行
struct CellVoltagesRaw<const N: usize> {
    voltages: [f32; N], // 原始数据仍然是 f32
}

#[allow(dead_code)] // 添加此行
struct CoulombCounterRaw {
    raw_cc: i16,
}

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

// USB 连接和数据收发函数
async fn connect_and_subscribe_usb(
    handle: rusb::DeviceHandle<rusb::Context>, // 改回 DeviceHandle
    command_ep_address: u8,
) -> Result<rusb::DeviceHandle<rusb::Context>, Box<dyn std::error::Error + Send + 'static>> {
    // 改回 DeviceHandle
    // 发送 SubscribeStatus 命令
    let mut cmd_buffer = [0u8; 64];
    let mut writer = Cursor::new(&mut cmd_buffer[..]);
    match UsbData::SubscribeStatus.write_be(&mut writer) {
        Ok(_) => {}
        Err(e) => {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Binrw write error: {}", e),
            ))
                as Box<dyn std::error::Error + Send + 'static>);
        }
    };
    let cmd_len = writer.position() as usize;

    match handle.write_interrupt(
        command_ep_address,
        &cmd_buffer[..cmd_len],
        Duration::from_secs(5),
    ) {
        // 移除 .await
        Ok(_) => {
            info!("已发送 SubscribeStatus 命令");
        }
        Err(e) => {
            return Err(Box::new(e) as Box<dyn std::error::Error + Send + 'static>);
        }
    };

    Ok(handle)
}

// USB 管理任务
async fn usb_manager_task(
    usb_vid: u16,
    usb_pid: u16,
    mut cmd_rx: mpsc::Receiver<UsbCommand>,
    event_tx: mpsc::Sender<UsbEvent>,
) {
    loop {
        let usb_context = match rusb::Context::new() {
            Ok(ctx) => ctx,
            Err(e) => {
                error!("创建 USB 上下文失败: {:?}, 10秒后重试...", e);
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
        };

        let (handle, command_ep_address, push_ep_address) = match find_and_open_usb_device(
            &usb_context, // 使用新创建的 usb_context
            usb_vid,
            usb_pid,
        )
        .await
        {
            Ok(h_info) => h_info,
            Err(e) => {
                error!("USB 设备查找或打开失败: {}, 10秒后重试...", e); // e 现在是 String
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
        };

        let handle = match connect_and_subscribe_usb(
            // 移除 mut
            handle,
            command_ep_address,
        )
        .await
        {
            Ok(h) => h,
            Err(e) => {
                error!("USB 订阅失败: {:?}, 尝试重新连接USB...", e);
                if let Err(send_err) = event_tx.send(UsbEvent::Error(e)).await {
                    error!("发送 USB 错误事件失败: {:?}", send_err);
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        let handle_arc = Arc::new(Mutex::new(Some(handle))); // 将 handle 包装在 Arc<Mutex<Option<...>>> 中
        let read_buffer_arc = Arc::new(Mutex::new(vec![0u8; 128])); // 将 read_buffer 包装在 Arc<Mutex> 中

        // 内部循环，处理 USB 数据读取和命令
        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(UsbCommand::Unsubscribe) => {
                            info!("USB 管理任务收到取消订阅命令。");
                            // 重新查找并打开设备以发送取消订阅命令
                            let unsubscribe_context = match rusb::Context::new() {
                                Ok(ctx) => ctx,
                                Err(e) => {
                                    error!("创建取消订阅 USB 上下文失败: {:?}", e);
                                    break; // 退出内部循环，外部循环会重试
                                }
                            };
                            match find_and_open_usb_device(&unsubscribe_context, usb_vid, usb_pid).await {
                                Ok((h, ep, _)) => {
                                    // 将同步的 USB 写入操作包装在 spawn_blocking 中
                                    let handle_clone = Arc::new(Mutex::new(Some(h))); // 克隆 Arc<Mutex<Option<handle>>>
                                    let command_ep_address_clone = ep;
                                    match tokio::time::timeout(Duration::from_secs(2), tokio::task::spawn_blocking(move || {
                                        let mut locked_handle_option: std::sync::MutexGuard<'_, Option<rusb::DeviceHandle<rusb::Context>>> = handle_clone.lock().unwrap();
                                        let handle_inner = locked_handle_option.take().expect("USB handle should be present for unsubscribe");
                                        let result = send_unsubscribe_command(handle_inner, command_ep_address_clone);
                                        // send_unsubscribe_command 消耗了 handle_inner，所以不需要放回
                                        result
                                    }).await.unwrap()).await { // unwrap 内部的 Result，因为 spawn_blocking 可能会返回 JoinError
                                        Ok(Ok(_)) => info!("已成功发送取消订阅命令。"),
                                        Ok(Err(e)) => error!("发送取消订阅命令失败: {:?}", e),
                                        Err(_) => error!("发送取消订阅命令超时。"),
                                    }
                                }
                                Err(e) => error!("重新打开 USB 设备以发送取消订阅命令失败: {}", e), // e 现在是 String
                            }
                            break; // 退出内部循环，外部循环会重试
                        }
                        None => {
                            info!("命令通道关闭，USB 管理任务退出。");
                            return; // 退出整个任务
                        }
                    }
                }
                read_result = async {
                    debug!("尝试从 USB IN 端点 {:#02x} 读取数据...", push_ep_address);
                    let handle_clone = Arc::clone(&handle_arc); // 克隆 Arc
                    let read_buffer_clone = Arc::clone(&read_buffer_arc); // 克隆 Arc
                    let push_ep_address_clone = push_ep_address;
                    let read_timeout = Duration::from_secs(10);

                    tokio::task::spawn_blocking(move || {
                        let mut locked_handle_option: std::sync::MutexGuard<'_, Option<rusb::DeviceHandle<rusb::Context>>> = handle_clone.lock().unwrap();
                        let handle_inner = locked_handle_option.take().expect("USB handle should be present"); // 移除 mut
                        let mut locked_buf = read_buffer_clone.lock().unwrap();
                        let result = handle_inner.read_interrupt(push_ep_address_clone, &mut locked_buf, read_timeout);
                        *locked_handle_option = Some(handle_inner); // 放回 handle
                        result
                    }).await.unwrap() // unwrap 内部的 Result，因为 spawn_blocking 可能会返回 JoinError
                } => {
                    match read_result {
                        Ok(n) => {
                            debug!("成功从 USB IN 端点 {:#02x} 读取到 {} 字节数据。", push_ep_address, n); // 修复拼写错误
                            let measurements_result = {
                                let locked_buf = read_buffer_arc.lock().unwrap(); // 在这里锁定 buf
                                let mut reader = Cursor::new(&locked_buf[..n]);
                                UsbData::read_be(&mut reader)
                            }; // 锁在这里释放

                            match measurements_result {
                                Ok(UsbData::StatusPush(measurements)) => {
                                    if let Err(e) = event_tx.send(UsbEvent::Measurements(measurements)).await {
                                        error!("发送 USB 测量数据失败: {:?}", e);
                                    }
                                }
                                Ok(_) => {
                                    warn!("收到非 StatusPush 的 USB 数据");
                                    if let Err(e) = event_tx.send(UsbEvent::Error(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "收到非 StatusPush 的 USB 数据")) as Box<dyn std::error::Error + Send + 'static>)).await {
                                        error!("发送 USB 错误事件失败: {:?}", e);
                                    }
                                }
                                Err(e) => {
                                    error!("USB 数据解析失败或读取错误: {:?}", e);
                                    if let Err(send_err) = event_tx.send(UsbEvent::Error(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("Binrw error: {}", e))) as Box<dyn std::error::Error + Send + 'static>)).await {
                                        error!("发送 USB 错误事件失败: {:?}", send_err);
                                    }
                                    break; // 退出内部循环，外部循环会重试
                                }
                            }
                        }
                        Err(e) => {
                            error!("USB 读取失败: {:?}", e);
                            if let Err(send_err) = event_tx.send(UsbEvent::Error(Box::new(e) as Box<dyn std::error::Error + Send + 'static>)).await {
                                error!("发送 USB 错误事件失败: {:?}", send_err);
                            }
                            break; // 退出内部循环，外部循环会重试
                        }
                    }
                }
            }
        }
    }
}

// MQTT 连接和发布函数
async fn connect_mqtt_and_publish(
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

async fn publish_measurements(
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
    let mqtt_topic_prefix = env::var("MQTT_TOPIC_PREFIX").unwrap_or_else(|_| "ups120".to_string()); // 移除下划线

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
                break Ok(()); // 退出主循环并返回 Ok(())
            }
            Some(usb_event) = usb_event_rx.recv() => {
                match usb_event {
                    UsbEvent::Measurements(measurements) => {
                        let topic = format!("{}/measurements", mqtt_topic_prefix);
                        if let Err(e) =
                            publish_measurements(&mqtt_client, &topic, measurements).await
                        {
                            error!("MQTT 发布失败: {:?}, 5秒后重试...", e);
                            tokio::time::sleep(Duration::from_secs(5)).await;
                        }
                    }
                    UsbEvent::Error(e) => {
                        error!("USB 管理任务报告错误: {:?}, 尝试重新连接USB...", e);
                        // 这里不需要 break，因为 usb_manager_task 会自动尝试重新连接
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

async fn find_and_open_usb_device(
    context: &rusb::Context,
    vid: u16,
    pid: u16,
) -> Result<(rusb::DeviceHandle<rusb::Context>, u8, u8), String> {
    // 修改返回类型为 String
    let device_list = context
        .devices()
        .map_err(|e| format!("获取 USB 设备列表失败: {}", e))?;
    let mut device_found = None;

    for device in device_list.iter() {
        let device_desc = device
            .device_descriptor()
            .map_err(|e| format!("获取设备描述符失败: {}", e))?;
        if device_desc.vendor_id() == vid && device_desc.product_id() == pid {
            info!(
                "找到 USB 设备: {:04x}:{:04x} (Bus: {}, Addr: {})",
                device_desc.vendor_id(),
                device_desc.product_id(),
                device.bus_number(),
                device.address()
            );
            device_found = Some(device);
            break;
        }
    }

    let device = match device_found {
        Some(d) => d,
        None => return Err("未找到指定 VID/PID 的 USB 设备".to_string()),
    };

    let handle = device
        .open()
        .map_err(|e| format!("打开 USB 设备失败: {}", e))?; // 移除 mut
    info!("已打开 USB 设备句柄。");

    // 尝试设置配置
    match handle.set_active_configuration(1) {
        Ok(_) => info!("已设置 USB 配置 1。"),
        Err(e) => return Err(format!("设置 USB 配置失败: {}", e)),
    }

    // 尝试声明接口
    match handle.claim_interface(1) {
        Ok(_) => info!("已声明 USB 接口 1。"),
        Err(e) => return Err(format!("声明 USB 接口失败: {}", e)),
    }

    // 找到 OUT 端点 (Command Endpoint)
    let config_descriptor = device
        .active_config_descriptor()
        .map_err(|e| format!("获取配置描述符失败: {}", e))?;
    let mut command_ep_address = 0;
    for interface in config_descriptor.interfaces() {
        for descriptor in interface.descriptors() {
            for endpoint in descriptor.endpoint_descriptors() {
                if endpoint.direction() == rusb::Direction::Out
                    && endpoint.transfer_type() == rusb::TransferType::Interrupt
                {
                    command_ep_address = endpoint.address();
                    info!("找到 USB 命令 OUT 端点: {:#02x}", command_ep_address);
                    break;
                }
            }
            if command_ep_address != 0 {
                break;
            }
        }
        if command_ep_address != 0 {
            break;
        }
    }

    if command_ep_address == 0 {
        return Err("未找到 USB 命令 OUT 端点".to_string());
    }

    // 找到 IN 端点 (Push Endpoint)
    // 初始化端点地址变量
    let mut command_ep_address = 0;
    let mut response_ep_address = 0;
    let mut push_ep_address = 0;
    // 用于收集所有 IN 中断端点地址的向量
    let mut in_interrupt_eps = Vec::new();

    // 遍历配置描述符中的所有接口
    for interface in config_descriptor.interfaces() {
        // 遍历接口中的所有描述符
        for descriptor in interface.descriptors() {
            // 遍历描述符中的所有端点
            for endpoint in descriptor.endpoint_descriptors() {
                // 检查端点是否为中断类型
                if endpoint.transfer_type() == rusb::TransferType::Interrupt {
                    // 如果是 OUT 方向，则认为是命令端点
                    if endpoint.direction() == rusb::Direction::Out {
                        command_ep_address = endpoint.address();
                        info!("找到 USB 命令 OUT 端点: {:#02x}", command_ep_address);
                    }
                    // 如果是 IN 方向，则添加到 IN 中断端点列表中
                    else if endpoint.direction() == rusb::Direction::In {
                        in_interrupt_eps.push(endpoint.address());
                    }
                }
            }
        }
    }

    // 根据收集到的 IN 中断端点数量，分配响应和推送端点
    if in_interrupt_eps.len() >= 2 {
        // 第一个 IN 中断端点作为响应端点
        response_ep_address = in_interrupt_eps[0];
        // 第二个 IN 中断端点作为推送端点
        push_ep_address = in_interrupt_eps[1];
        info!("找到 USB 响应 IN 端点: {:#02x}", response_ep_address);
        info!("找到 USB 推送 IN 端点: {:#02x}", push_ep_address);
    } else {
        // 如果没有找到足够的 IN 中断端点，则记录错误
        error!(
            "未能找到足够的 USB IN 中断端点。找到 {} 个。",
            in_interrupt_eps.len()
        );
    }

    if push_ep_address == 0 {
        return Err("未找到 USB 推送 IN 端点".to_string());
    }

    Ok((handle, command_ep_address, push_ep_address))
}

async fn send_unsubscribe_command(
    handle: rusb::DeviceHandle<rusb::Context>, // 改回所有权
    command_ep_address: u8,
) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
    info!("正在发送取消订阅命令...");
    let mut cmd_buffer = [0u8; 64];
    let mut writer = Cursor::new(&mut cmd_buffer[..]);
    match UsbData::UnsubscribeStatus.write_be(&mut writer) {
        Ok(_) => {}
        Err(e) => {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Binrw write error: {}", e),
            ))
                as Box<dyn std::error::Error + Send + 'static>);
        }
    };
    let cmd_len = writer.position() as usize;

    match handle.write_interrupt(
        command_ep_address,
        &cmd_buffer[..cmd_len],
        Duration::from_secs(5),
    ) {
        Ok(_) => {
            info!("已成功发送取消订阅命令。");
            Ok(())
        }
        Err(e) => {
            error!("发送取消订阅命令失败: {:?}", e);
            Err(Box::new(e) as Box<dyn std::error::Error + Send + 'static>)
        }
    }
}
