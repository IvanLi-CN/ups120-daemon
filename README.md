# ups120-daemon

这是一个用于 UPS120 项目的上位机程序。

## 项目简介
该项目旨在提供一个命令行界面 (CLI) 工具，用于与 UPS120 设备进行交互和控制。它利用 Rust 语言的强大功能和 Tokio 异步运行时，以实现高效和响应式的操作。

## 如何运行

1.  **克隆仓库**
    ```bash
    git clone [此仓库的URL]
    cd ups120-daemon
    ```
2.  **初始化子模块**
    ```bash
    git submodule update --init --recursive
    ```
3.  **构建项目**
    ```bash
    cargo build
    ```
4.  **运行项目**
    ```bash
    cargo run
    ```

## 子模块
本项目包含以下 Git 子模块：

*   `device`: [git@github.com:IvanLi-CN/ups120.git](git@github.com:IvanLi-CN/ups120.git)
    该子模块包含了 UPS120 设备的底层驱动和相关代码。