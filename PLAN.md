# USB 通信功能排查与修复计划

**目标：** 定位并解决上位机无法从 `device` 固件通过 USB 正常订阅和接收数据的问题。

**背景分析回顾：**
根据对固件 ([`device/src/main.rs`](device/src/main.rs:1), [`device/src/usb/mod.rs`](device/src/usb/mod.rs:1), [`device/src/usb/endpoints.rs`](device/src/usb/endpoints.rs:1)) 的代码审查，固件具备以下 USB 通信能力：

- **命令接收与处理：**
  - 能够接收上位机发送的命令，特别是 `SubscribeStatus` (magic byte `0x00`) 和 `UnsubscribeStatus` (magic byte `0x01`)。
  - `process_command` 函数 ([`device/src/usb/endpoints.rs:88`](device/src/usb/endpoints.rs:88)) 负责处理这些命令并更新内部的 `status_subscription_active` ([`device/src/usb/endpoints.rs:33`](device/src/usb/endpoints.rs:33)) 状态。
- **响应发送：**
  - 当收到 `SubscribeStatus` 命令后，固件会通过其响应端点 (`response_write_ep` - [`device/src/usb/endpoints.rs:29`](device/src/usb/endpoints.rs:29)) 发送一个 `StatusResponse` ([`device/src/usb/endpoints.rs:20`](device/src/usb/endpoints.rs:20))。此响应以 magic byte `0x80` 开头，并包含当前的 `AllMeasurements` ([`device/src/data_types.rs:49`](device/src/data_types.rs:49)) 数据。
- **数据推送：**
  - 当 `status_subscription_active` ([`device/src/usb/endpoints.rs:33`](device/src/usb/endpoints.rs:33)) 为 `true` 时，固件会通过其推送端点 (`push_write_ep` - [`device/src/usb/endpoints.rs:30`](device/src/usb/endpoints.rs:30)) 定期发送 `StatusPush` ([`device/src/usb/endpoints.rs:24`](device/src/usb/endpoints.rs:24)) 数据。此推送数据以 magic byte `0xC0` 开头，并包含 `AllMeasurements` ([`device/src/data_types.rs:49`](device/src/data_types.rs:49)) 数据。

**排查与修复步骤：**

1. **验证上位机正确发送 `SubscribeStatus` 命令：**
    - **操作：** 检查上位机代码，确保其向固件的命令端点（`command_read_ep` - [`device/src/usb/endpoints.rs:28`](device/src/usb/endpoints.rs:28) 在固件侧）发送了正确的 `SubscribeStatus` 命令。
    - **验证：** 命令应为单个字节 `0x00`。
    - **工具：** 可以使用 USB 分析工具（如 Wireshark 与 USBPcap，或特定平台的 USB 嗅探工具）捕获 USB 流量，确认该字节已发送。
    - **固件侧日志：** 固件在 `usb_task` ([`device/src/usb/mod.rs:124`](device/src/usb/mod.rs:124)) 中有 `defmt::info!("USB command received: {:?}", cmd);` 日志，可以确认是否收到了命令。

2. **验证上位机正确接收并解析 `StatusResponse`：**
    - **操作：** 检查上位机代码，确保其在发送 `SubscribeStatus` 命令后，能够从固件的响应端点接收数据。
    - **验证：** 上位机应能接收到以 `0x80` 开头的数据包，并能根据 `AllMeasurements` ([`device/src/data_types.rs:49`](device/src/data_types.rs:49)) 的结构正确解析后续数据。
    - **工具：** USB 分析工具。
    - **固件侧日志：** 固件在 `send_response` ([`device/src/usb/endpoints.rs:74`](device/src/usb/endpoints.rs:74)) 中有 `defmt::info!("固件发送响应原始字节: {:x}", &self.write_buffer[..len]);` 日志。

3. **验证上位机正确接收并解析 `StatusPush` 数据：**
    - **操作：** 检查上位机代码，确保其能够从固件的推送端点接收数据。
    - **验证：** 上位机应能接收到以 `0xC0` 开头的数据包，并能根据 `AllMeasurements` ([`device/src/data_types.rs:49`](device/src/data_types.rs:49)) 的结构正确解析后续数据。
    - **工具：** USB 分析工具。
    - **固件侧日志：** 固件在 `send_status_update` ([`device/src/usb/endpoints.rs:125`](device/src/usb/endpoints.rs:125)) 中有 `defmt::info!("固件发送原始字节: {:x}", &self.write_buffer[..len]);` 日志。

4. **数据结构一致性检查 (`AllMeasurements`)：**
    - **操作：** 仔细比对上位机用于解析 `AllMeasurements` ([`device/src/data_types.rs:49`](device/src/data_types.rs:49)) 的数据结构定义与固件中 `binrw` 序列化/反序列化的行为。
    - **注意：** 确保字段顺序、类型、大小端等均一致。`binrw` 默认使用大端序 (Big Endian)。

5. **USB 端点配置与能力检查：**
    - **操作：** 确认上位机期望的 USB 端点类型（Interrupt, Bulk等）、方向、最大包大小等配置与固件在 `UsbEndpoints::new` ([`device/src/usb/endpoints.rs:37`](device/src/usb/endpoints.rs:37)) 中的配置一致。固件配置的是 Interrupt 端点，最大包大小为 64 字节。

6. **使用 USB 分析工具进行端到端流量分析：**
    - **操作：** 捕获从上位机发送命令到固件响应和推送数据的完整 USB 交互过程。
    - **分析：** 检查是否有 USB 协议层面的错误、数据包是否完整、端点是否按预期工作。

**固件调试增强（可选，如果上述步骤未能定位问题）：**

1. **增加更详细的 USB 事件日志：**
    - 在固件的 `usb_task` ([`device/src/usb/mod.rs:35`](device/src/usb/mod.rs:35)) 和 `UsbEndpoints` ([`device/src/usb/endpoints.rs:27`](device/src/usb/endpoints.rs:27)) 的关键路径（如端点读写前后、状态变更时）添加更详细的 `defmt` 日志，以便更精确地追踪执行流程和数据状态。

**预期成果：**
- 明确上位机无法接收到 USB 数据的根本原因。
- 如果问题在上位机，提供明确的修改建议。
- 如果问题在固件（尽管目前分析可能性较低），定位到具体代码并进行修复。
- 最终目标是使上位机能够成功订阅并持续接收到固件推送的 `AllMeasurements` 数据。

**通信流程示意图：**

```mermaid
sequenceDiagram
    participant 上位机
    participant 固件 (device)

    上位机->>+固件: 发送 SubscribeStatus 命令 (0x00) 到 command_read_ep
    Note over 固件: process_command() -> status_subscription_active = true
    固件->>-上位机: 发送 StatusResponse (0x80 + AllMeasurements) 到 response_write_ep

    loop 周期性数据推送 (当 status_subscription_active 为 true)
        Note over 固件: usb_task 聚合数据
        固件->>上位机: 发送 StatusPush (0xC0 + AllMeasurements) 到 push_write_ep
    end

    Note right of 上位机: 上位机持续监听 push_write_ep 以接收数据

    alt 用户取消订阅
        上位机->>+固件: 发送 UnsubscribeStatus 命令 (0x01) 到 command_read_ep
        Note over 固件: process_command() -> status_subscription_active = false
        固件-->>-上位机: (可选) 发送确认响应
    end
