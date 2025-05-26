use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use binrw::{
    BinRead, BinWrite,
    io::{Read, Seek, Write},
};
use log::{debug, error, info, warn};
use rusb::UsbContext;
use tokio::sync::mpsc;

use super::usb_types::{UsbCommand, UsbEvent};
use super::binrw_impls::UsbData;
use super::data_models::AllMeasurements;

// USB 连接和数据收发函数
pub async fn connect_and_subscribe_usb(
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
pub async fn usb_manager_task(
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

pub async fn find_and_open_usb_device(
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
    #[allow(unused_assignments)] // response_ep_address is assigned but never read
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

pub async fn send_unsubscribe_command(
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