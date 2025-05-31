use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use binrw::{BinRead, BinWrite};
use log::{debug, error, info, warn};
use rusb::UsbContext;
use tokio::sync::mpsc;

use super::usb_types::{UsbCommand, UsbEvent, UsbError, UsbData as HostUsbData};
use super::binrw_impls::UsbData;

// USB 连接和数据收发函数
pub async fn connect_and_subscribe_usb(
    handle: rusb::DeviceHandle<rusb::Context>, 
    command_ep_address: u8,
    response_ep_address: u8,
) -> Result<rusb::DeviceHandle<rusb::Context>, UsbError> {
    // Minor comment to force re-evaluation
    let mut cmd_buffer = [0u8; 64];
    let mut writer = Cursor::new(&mut cmd_buffer[..]);
    HostUsbData::SubscribeStatus.write_be(&mut writer).map_err(|e| UsbError::BinrwError(e.to_string()))?;
    let cmd_len = writer.position() as usize;

    match handle.write_interrupt(
        command_ep_address,
        &cmd_buffer[..cmd_len],
        Duration::from_secs(5),
    ) {
        Ok(len_written) => {
            info!("已发送 SubscribeStatus 命令 ({} bytes)", len_written);
        }
        Err(e) => {
            error!("发送 SubscribeStatus 命令失败: {:?}", e);
            return Err(UsbError::from(e));
        }
    };

    info!("等待来自响应端点 {:#02x} 的 StatusResponse...", response_ep_address);
    let mut resp_buf = [0u8; 256];
    match handle.read_interrupt(response_ep_address, &mut resp_buf, Duration::from_secs(5)) {
        Ok(n) => {
            info!("从响应端点读取到 {} 字节。", n);
            log::debug!("上位机接收用于响应的原始字节: {:x?}", &resp_buf[..n]);
            match UsbData::read_be(&mut Cursor::new(&resp_buf[..n])) { 
                Ok(UsbData::StatusResponse(_measurements)) => {
                    info!("成功收到 StatusResponse 确认。");
                }
                Ok(other_data) => {
                    error!("收到意外的响应类型: {:?}", other_data);
                    return Err(UsbError::UnexpectedResponse);
                }
                Err(e) => {
                    error!("解析 StatusResponse 失败: {:?}", e);
                    return Err(UsbError::ResponseParseError(e.to_string()));
                }
            }
        }
        Err(e) => {
            error!("读取 StatusResponse 失败: {:?}", e);
            if e == rusb::Error::Timeout {
                return Err(UsbError::Timeout);
            }
            return Err(UsbError::ResponseReadFailed(e.to_string()));
        }
    }
    Ok(handle)
}

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

        let (handle_option, command_ep_address, response_ep_address_opt, push_ep_address_opt) =
            match find_and_open_usb_device(&usb_context, usb_vid, usb_pid).await {
                Ok(h_info) => h_info,
                Err(e) => {
                    error!("USB 设备查找或打开失败: {}, 25秒后重试...", e); // 增加重试延迟
                    tokio::time::sleep(Duration::from_secs(25)).await; // 增加重试延迟
                    continue;
                }
            };
        
        let mut current_handle = match handle_option { 
            Some(h) => h,
            None => { 
                error!("find_and_open_usb_device 返回 None handle，这是不期望的。");
                let _ = event_tx.send(UsbEvent::Error(UsbError::DeviceNotFound)).await;
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
        };
        
        let response_ep_address = match response_ep_address_opt {
            Some(ep) => ep,
            None => {
                error!("未能获取响应端点地址，尝试重新连接USB...");
                let _ = event_tx.send(UsbEvent::Error(UsbError::EndpointNotFound("响应端点未找到".to_string()))).await;
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        let push_ep_address = match push_ep_address_opt {
            Some(ep) => ep,
            None => {
                error!("未能获取推送端点地址，尝试重新连接USB...");
                let _ = event_tx.send(UsbEvent::Error(UsbError::EndpointNotFound("推送端点未找到".to_string()))).await;
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        current_handle = match connect_and_subscribe_usb(current_handle, command_ep_address, response_ep_address).await {
            Ok(h) => h,
            Err(e) => { 
                error!("USB 订阅失败: {}, 尝试重新连接USB...", e);
                if let Err(send_err) = event_tx.send(UsbEvent::Error(e)).await { 
                    error!("发送 USB 错误事件失败: {:?}", send_err);
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        let handle_arc = Arc::new(Mutex::new(Some(current_handle)));
        let read_buffer_arc = Arc::new(Mutex::new(vec![0u8; 256]));

        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(UsbCommand::Subscribe) => {
                            info!("USB 管理任务收到订阅命令。尝试重新连接并订阅...");
                            break; 
                        }
                        Some(UsbCommand::Unsubscribe) => { 
                            info!("USB 管理任务收到取消订阅命令 (placeholder logic)。");
                            let _ = event_tx.send(UsbEvent::Error(UsbError::Other("Unsubscribe not fully implemented yet".to_string()))).await;
                            break; 
                        }
                        None => {
                            info!("命令通道关闭，USB 管理任务退出。");
                            return;
                        }
                    }
                }
                read_result = async {
                    debug!("尝试从 USB IN 端点 {:#02x} 读取数据...", push_ep_address);
                    let handle_clone = Arc::clone(&handle_arc);
                    let read_buffer_clone = Arc::clone(&read_buffer_arc);
                    let push_ep_address_clone = push_ep_address;
                    let read_timeout = Duration::from_secs(10);

                    tokio::task::spawn_blocking(move || {
                        let mut locked_handle_option = handle_clone.lock().unwrap();
                        if let Some(handle_inner) = locked_handle_option.as_mut() { 
                            let mut locked_buf = read_buffer_clone.lock().unwrap();
                            handle_inner.read_interrupt(push_ep_address_clone, &mut locked_buf, read_timeout)
                        } else {
                            Err(rusb::Error::NoDevice) 
                        }
                    }).await.unwrap_or_else(|_join_error| Err(rusb::Error::Other)) 
                } => {
                    match read_result {
                        Ok(n) => {
                            if n == 0 {
                                debug!("从 USB IN 端点 {:#02x} 读取到 0 字节数据，可能为正常轮询。", push_ep_address);
                                continue; 
                            }
                            debug!("成功从 USB IN 端点 {:#02x} 读取到 {} 字节数据。", push_ep_address, n);
                            let measurements_result = {
                                let locked_buf = read_buffer_arc.lock().unwrap();
                                log::debug!("上位机接收推送原始字节: {:x?}", &locked_buf[..n]);
                                let mut reader = Cursor::new(&locked_buf[..n]);
                                UsbData::read_be(&mut reader) 
                            };

                            match measurements_result {
                                Ok(UsbData::StatusPush(measurements)) => {
                                    if let Err(e) = event_tx.send(UsbEvent::Measurements(measurements)).await {
                                        error!("发送 USB 测量数据失败: {:?}", e);
                                    }
                                }
                                Ok(other_data) => {
                                    warn!("收到非 StatusPush 的 USB 数据类型: {:?}", other_data);
                                    if let Err(e) = event_tx.send(UsbEvent::Error(UsbError::UnexpectedResponse)).await {
                                        error!("发送 USB 错误事件失败: {:?}", e);
                                    }
                                }
                                Err(e) => {
                                    error!("USB 推送数据解析失败: {:?}", e);
                                    if let Err(send_err) = event_tx.send(UsbEvent::Error(UsbError::BinrwError(e.to_string()))).await {
                                        error!("发送 USB 解析错误事件失败: {:?}", send_err);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("USB 读取失败: {:?}", e);
                            let usb_error = UsbError::from(e); 
                            if let Err(send_err) = event_tx.send(UsbEvent::Error(usb_error)).await {
                                error!("发送 USB 读取错误事件失败: {:?}", send_err);
                            }
                            break; 
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
) -> Result<(Option<rusb::DeviceHandle<rusb::Context>>, u8, Option<u8>, Option<u8>), UsbError> {
    let device_list = context.devices().map_err(UsbError::from)?;
    let mut device_found_rusb = None;

    for device_rusb in device_list.iter() {
        let device_desc = device_rusb.device_descriptor().map_err(UsbError::from)?;
        if device_desc.vendor_id() == vid && device_desc.product_id() == pid {
            info!(
                "找到 USB 设备: {:04x}:{:04x} (Bus: {}, Addr: {})",
                device_desc.vendor_id(),
                device_desc.product_id(),
                device_rusb.bus_number(),
                device_rusb.address()
            );
            device_found_rusb = Some(device_rusb);
            break;
        }
    }

    let device_rusb = device_found_rusb.ok_or(UsbError::DeviceNotFound)?;

    let mut handle = device_rusb.open().map_err(|e| UsbError::OpenFailed(e.to_string()))?; // handle IS mut
    info!("已打开 USB 设备句柄。");

    // 尝试重置设备，看是否有助于解决重连问题
    // 将 reset 调用提前到内核驱动处理之前
    info!("尝试对 USB 设备执行端口重置 (提前调用)...");
    if let Err(e) = handle.reset() {
        warn!("USB 设备重置失败 (提前调用): {}. 继续尝试...", e);
        // 不将此视为致命错误，但记录下来。
    } else {
        info!("USB 设备已成功重置 (提前调用)。");
        // 重置后可能需要短暂延时，让设备重新稳定
        // tokio::time::sleep(Duration::from_millis(200)).await; // 可选的短暂延时增加
    }

    let interface_number = 1;
    let mut detached_here = false;

    if cfg!(any(target_os = "linux", target_os = "macos")) {
        match handle.kernel_driver_active(interface_number) { 
            Ok(true) => {
                info!("内核驱动已附加到接口{}，尝试分离...", interface_number);
                if let Err(e) = handle.detach_kernel_driver(interface_number) {
                    warn!("分离接口{}内核驱动失败: {:?}. 将立即返回错误。", interface_number, e);
                    return Err(UsbError::DetachFailed(e.to_string())); // 返回错误
                } else {
                    info!("接口{}内核驱动已成功分离。", interface_number);
                    detached_here = true;
                }
            }
            Ok(false) => { /* No driver active, nothing to detach */ }
            Err(e) => {
                warn!("检查接口{}内核驱动状态失败: {:?}. 继续尝试...", interface_number, e);
            }
        }
    }

    // 已将 reset 调用提前

    if let Err(e) = handle.set_active_configuration(1) {
         if detached_here {
            if let Err(attach_err) = handle.attach_kernel_driver(interface_number) {
                warn!("配置失败后，重新附加内核驱动到接口 {} 失败: {:?}", interface_number, attach_err);
            }
         }
         return Err(UsbError::SetConfigurationFailed(e.to_string()));
    }
    info!("已设置 USB 配置 1。");

    if let Err(e) = handle.claim_interface(interface_number) { 
        if detached_here { 
            if let Err(attach_err) = handle.attach_kernel_driver(interface_number) {
                 warn!("声明接口失败后，重新附加内核驱动到接口 {} 失败: {:?}", interface_number, attach_err);
            }
        }
        return Err(UsbError::ClaimInterfaceFailed(e.to_string()));
    }
    info!("已声明 USB 接口 {}。", interface_number);

    let config_descriptor = device_rusb.active_config_descriptor().map_err(UsbError::from)?;
    let mut command_ep_address = 0u8;
    let mut response_ep_address: Option<u8> = None;
    let mut push_ep_address: Option<u8> = None;
    let mut in_interrupt_eps = Vec::new();
    
    let mut found_claimed_interface_descriptors = false; 
    for iface in config_descriptor.interfaces() { 
        for iface_desc in iface.descriptors() {    
            if iface_desc.interface_number() == interface_number {
                found_claimed_interface_descriptors = true;
                for endpoint_descriptor in iface_desc.endpoint_descriptors() { 
                    if endpoint_descriptor.transfer_type() == rusb::TransferType::Interrupt {
                        match endpoint_descriptor.direction() {
                            rusb::Direction::Out => {
                                if command_ep_address == 0 { 
                                    command_ep_address = endpoint_descriptor.address();
                                    info!("找到 USB 命令 OUT 端点: {:#02x} on interface {}", command_ep_address, interface_number);
                                } else {
                                    info!("找到额外的 USB 命令 OUT 端点: {:#02x} on interface {} (已忽略)", endpoint_descriptor.address(), interface_number);
                                }
                            }
                            rusb::Direction::In => {
                                in_interrupt_eps.push(endpoint_descriptor.address());
                                info!("找到 USB IN 中断端点: {:#02x} on interface {}", endpoint_descriptor.address(), interface_number);
                            }
                        }
                    }
                }
                break; 
            }
        }
        if found_claimed_interface_descriptors {
            break; 
        }
    }
    if !found_claimed_interface_descriptors { 
        return Err(UsbError::EndpointNotFound(format!("接口 {} 的描述符未找到", interface_number)));
    }

    if command_ep_address == 0 {
        return Err(UsbError::EndpointNotFound(format!("命令 OUT 端点未在接口 {} 上找到", interface_number)));
    }

    if !in_interrupt_eps.is_empty() {
        response_ep_address = Some(in_interrupt_eps[0]);
        info!("USB 响应 IN 端点设置为: {:#02x}", in_interrupt_eps[0]);
        if in_interrupt_eps.len() > 1 {
            push_ep_address = Some(in_interrupt_eps[1]);
            info!("USB 推送 IN 端点设置为: {:#02x}", in_interrupt_eps[1]);
        } else {
            warn!("只找到一个 USB IN 中断端点 {:#02x}。将用作响应和推送端点。", in_interrupt_eps[0]);
            push_ep_address = Some(in_interrupt_eps[0]);
        }
    } else {
        error!("在接口 {} 上未能找到任何 USB IN 中断端点。", interface_number);
        return Err(UsbError::EndpointNotFound(format!("IN 端点未在接口 {} 上找到", interface_number)));
    }
    
    if response_ep_address.is_none() || push_ep_address.is_none() {
        return Err(UsbError::EndpointNotFound("未能成功分配响应或推送IN端点".to_string()));
    }

    Ok((Some(handle), command_ep_address, response_ep_address, push_ep_address))
}

pub async fn send_unsubscribe_command(
    handle: rusb::DeviceHandle<rusb::Context>, 
    command_ep_address: u8,
) -> Result<(), UsbError> {
    info!("正在发送取消订阅命令...");
    let mut cmd_buffer = [0u8; 64];
    let mut writer = Cursor::new(&mut cmd_buffer[..]);
    HostUsbData::UnsubscribeStatus.write_be(&mut writer).map_err(|e| UsbError::BinrwError(e.to_string()))?;
    let cmd_len = writer.position() as usize;

    match handle.write_interrupt(
        command_ep_address,
        &cmd_buffer[..cmd_len],
        Duration::from_secs(5),
    ) {
        Ok(len_written) => {
            info!("已成功发送取消订阅命令 ({} bytes)。", len_written);
            Ok(())
        }
        Err(e) => {
            error!("发送取消订阅命令失败: {:?}", e);
            Err(UsbError::from(e))
        }
    }
}