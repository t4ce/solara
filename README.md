# Solara

Solara 是一款基于 Rust + QuickJS 的简易浏览器实验项目。

项目目标是用 Rust 负责浏览器外壳、资源加载、文档模型和渲染流程，用 QuickJS 执行页面 JavaScript，并通过一层轻量的宿主 API 将脚本运行时与浏览器环境连接起来。

> 当前状态：项目仍处于早期骨架阶段，浏览器内核、QuickJS 集成和页面渲染能力正在开发中。

## 特性目标

- Rust 实现核心流程，便于控制内存、并发和系统集成。
- QuickJS 作为嵌入式 JavaScript 引擎，提供轻量的 ECMAScript 执行环境。
- 简化版浏览器模型，聚焦 URL 加载、HTML/CSS 解析、DOM 构建、脚本执行和基础渲染。
- 小型、可读、可实验，适合作为学习浏览器工作原理的代码库。

## 非目标

Solara 不是 Chrome、Firefox 或 Safari 的替代品。它不会在早期阶段追求完整 Web 标准兼容，也不会以内置复杂优化和生产级安全沙箱为首要目标。

## 环境要求

- Rust 1.85 或更高版本
- Cargo

项目使用 Rust 2024 edition。

## 快速开始

克隆项目后，在仓库根目录执行：

```bash
cargo build
```

运行当前程序：

```bash
cargo run
```

执行检查：

```bash
cargo check
```

## 项目结构

```text
.
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── README.md
└── src
    └── main.rs
```

## 规划

- 搭建基础应用入口和命令行参数。
- 引入 QuickJS 运行时，并封装脚本执行上下文。
- 实现最小 HTML 解析和 DOM 数据结构。
- 为 JavaScript 暴露精简版 `window` / `document` API。
- 实现资源加载、导航和错误处理。
- 增加基础布局与绘制流程。
- 补充单元测试和示例页面。

## 许可证

Solara 使用 [MIT License](LICENSE)。
