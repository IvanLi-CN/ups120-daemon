# BQ76920 状态信息上位机显示与 MQTT 发布方案

## 方案概述

为了在上位机程序中解析并显示 BQ76920 的全面状态信息（包括充电放电 MOS 管状态及其他系统状态），并将这些信息发布到 MQTT，我们将根据固件已实现的接口进行以下修改：

1.  **扩展 `Bq76920Measurements` 结构体：** 在 `src/data_models.rs` 中，将 `SystemStatus` 和 MOS 管状态（`charge_on`, `discharge_on`）添加到 `Bq76920Measurements` 结构体中。
2.  **修改 `AllMeasurements` 的 `BinRead` 和 `BinWrite` 实现：** 在 `src/binrw_impls.rs` 中，更新 `AllMeasurements` 的 `BinRead` 和 `BinWrite` 实现，以包含新的 BQ76920 状态数据。
3.  **MQTT 数据发布更新：** 确保 `src/mqtt_handlers.rs` 中的 MQTT 发布逻辑能够正确地将包含新状态信息的 `AllMeasurements` 结构体以 JSON 格式发布。

## 详细计划

以下是实现此功能的详细步骤：

**步骤 1：修改 `src/data_models.rs`**

*   **目标：** 在上位机程序的数据模型中，定义 BQ76920 的系统状态和 MOS 管状态，并将其集成到 `Bq76920Measurements` 结构体中。
*   **具体操作：**
    *   定义一个新的 `SystemStatus` 结构体，用于表示 BQ76920 的 `SysStat` 寄存器中的位字段。例如，可以使用 `bitflags` crate 来方便地定义位字段。
    *   定义一个新的 `MosStatus` 枚举或结构体，用于表示充电/放电 MOS 管的状态（`CHG_ON` 和 `DSG_ON`）。
    *   在 `Bq76920Measurements` 结构体中添加 `system_status: SystemStatus` 和 `mos_status: MosStatus` 字段。
    *   确保为 `SystemStatus` 和 `MosStatus` 以及 `Bq76920Measurements` 结构体添加 `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]` 宏，以便进行调试、克隆、比较以及 JSON 序列化/反序列化。

**步骤 2：修改 `src/binrw_impls.rs`**

*   **目标：** 更新 `AllMeasurements` 结构体的二进制读写实现，使其能够正确地处理 `Bq76920Measurements` 中新增的状态字段。
*   **具体操作：**
    *   修改 `impl BinRead for AllMeasurements<N>` 和 `impl BinWrite for AllMeasurements<N>` 的实现。
    *   在读取和写入 `Bq76920Measurements` 部分时，需要根据 `SystemStatus` 和 `MosStatus` 的具体定义，调整二进制数据的解析和写入逻辑。例如，如果 `SystemStatus` 是一个 `u8`，则需要读取一个字节；如果 `MosStatus` 是一个布尔值，则可能需要从一个字节中解析出对应的位。

**步骤 3：修改 `src/mqtt_handlers.rs`**

*   **目标：** 确保包含新状态信息的 `AllMeasurements` 结构体能够正确地序列化为 JSON 并发布到 MQTT。
*   **具体操作：**
    *   检查 `publish_measurements` 函数。由于在 `src/data_models.rs` 中已经为相关结构体添加了 `#[derive(Serialize)]` 宏，`serde_json` 应该能够自动处理新的字段。
    *   如果需要，可以添加日志输出，以验证发布到 MQTT 的 JSON 数据是否包含了 `system_status` 和 `mos_status` 字段。

## Mermaid 图

```mermaid
graph TD
    A[用户需求：上位机显示BQ76920状态并发布到MQTT] --> B{分析现有上位机代码结构}
    B --> C1[src/data_models.rs]
    B --> C2[src/binrw_impls.rs]
    B --> C3[src/mqtt_handlers.rs]

    C1 --> D[识别Bq76920Measurements结构体]
    C2 --> E[识别AllMeasurements的BinRead/BinWrite实现]
    C3 --> F[识别publish_measurements函数]

    D & E & F --> G[设计上位机实现方案]

    G --> H[修改src/data_models.rs: 扩展Bq76920Measurements，定义SystemStatus和MosStatus]
    G --> I[修改src/binrw_impls.rs: 更新AllMeasurements的BinRead/BinWrite以包含新状态]
    G --> J[修改src/mqtt_handlers.rs: 确保新状态信息正确发布到MQTT]

    H --> K[实现]
    I --> K
    J --> K

    K --> L[完成]