# `src/main.rs` 文件拆分计划

为了改善 `src/main.rs` 的代码结构并减少文件行数，将文件内容拆分为以下几个模块：

## 1. `src/data_models.rs`
*   **内容**: 所有测量数据相关的结构体定义 (`Bq25730Measurements`, `Bq76920Measurements`, `Temperatures`, `AllMeasurements`) 和自定义序列化函数 (`serialize_electric_potential`, `serialize_thermodynamic_temperature`, `serialize_voltages`, `serialize_temperatures`)。
*   **目的**: 集中管理数据模型和其序列化逻辑，提高可维护性。

## 2. `src/usb_types.rs`
*   **内容**: USB 相关的枚举 (`UsbData`, `UsbCommand`, `UsbEvent`) 和原始数据结构 (`AdcMeasurementsRaw`, `CellVoltagesRaw`, `CoulombCounterRaw`)。
*   **目的**: 封装 USB 通信中使用的各种数据类型，使其独立于 USB 处理逻辑。

## 3. `src/binrw_impls.rs`
*   **内容**: `AllMeasurements` 的 `BinRead` 和 `BinWrite` 实现。
*   **目的**: 将二进制读写逻辑从数据模型定义中分离，保持数据模型的纯净性。

## 4. `src/usb_handlers.rs`
*   **内容**: USB 连接和管理相关的函数 (`connect_and_subscribe_usb`, `usb_manager_task`, `find_and_open_usb_device`, `send_unsubscribe_command`)。
*   **目的**: 集中处理 USB 设备的连接、数据收发和管理逻辑。

## 5. `src/mqtt_handlers.rs`
*   **内容**: MQTT 连接和发布相关的函数 (`connect_mqtt_and_publish`, `publish_measurements`)。
*   **目的**: 集中处理 MQTT 消息代理的连接和数据发布逻辑。

## 6. `src/main.rs`
*   **内容**: 作为主入口文件，保留 `main` 函数和必要的 `use` 语句，并引入新创建的模块。
*   **目的**: 保持主函数简洁，只负责协调各个模块的启动和运行。

## 模块依赖关系图

```mermaid
graph TD
    main.rs --> data_models.rs
    main.rs --> usb_types.rs
    main.rs --> binrw_impls.rs
    main.rs --> usb_handlers.rs
    main.rs --> mqtt_handlers.rs
    usb_handlers.rs --> usb_types.rs
    usb_handlers.rs --> data_models.rs
    binrw_impls.rs --> data_models.rs
    binrw_impls.rs --> usb_types.rs
    mqtt_handlers.rs --> data_models.rs