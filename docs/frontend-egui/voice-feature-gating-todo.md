# Voice/LiveKit Feature Gating — TODO

> **目标**：将 `livekit` / `livekit-api` / `cpal` 改为可选依赖，默认 `cargo check` 不拉取 `webrtc-sys`，避免国内网络下载超时阻塞 GUI 编译。
>
> **状态**：✅ 已实现并验证通过（2026-06-23）

---

## 实现步骤

- [x] **1. 修改 `Cargo.toml`**  
  - `livekit`、`livekit-api`、`cpal` 改为 `optional = true`  
  - 添加 `[features]`：`voice = ["livekit", "livekit-api", "cpal"]`

- [x] **2. 修改 `src/voice/mod.rs`**  
  - 始终提供统一的 `VoiceManager` 结构体，公有字段始终存在  
  - 私有字段（`room`, `call_task`, `capture_task`, `playback_task`）通过 `#[cfg(feature = "voice")]` 条件编译  
  - `connect`/`disconnect`/`toggle_mute` 提供两份 `impl`：带 `voice` feature 的实现和 stub 实现

- [x] **3. 修改 `src/voice/connection.rs`**  
  - 文件顶部加 `#![cfg(feature = "voice")]`  
  - 从独立 `VoiceManager` 结构体重构为辅助函数 `connect(vm: &mut VoiceManager)`

- [x] **4. 修改 `src/voice/audio_capture.rs`**  
  - 文件顶部加 `#![cfg(feature = "voice")]`

- [x] **5. 修改 `src/voice/audio_playback.rs`**  
  - 文件顶部加 `#![cfg(feature = "voice")]`

- [x] **6. 修改 `src/app.rs`**  
  - `exec_api_command` 中 voice 相关的 3 个 match arm 加 `#[cfg(feature = "voice")]`  
  - 无 voice 构建时静默忽略 voice API 命令  
  - spawn 闭包中的 `match cmd` 分支去掉 `#[cfg(feature = "voice")]` gate（保证穷尽模式匹配）

- [x] **7. 修改 `src/ui/panels/voice.rs`**  
  - 无 `voice` feature 时显示 "Voice feature not compiled. Rebuild with: cargo build --features voice"  
  - 原 UI 逻辑提取到 `draw_voice_ui()` 函数，通过 `#[cfg(feature = "voice")]` gate

- [x] **8. 验证**  
  ```bash
  cd apps/desktop-egui
  cargo clean && cargo check  # ✅ 通过，无 webrtc-sys，编译时间 ~1.6s
  ```
  编译结果：**0 errors, 49 warnings**（均为 unused imports/fields/methods 的 warning）

---

## 补充：其他发现的问题

### 🔴 运行时会 panic（编译不报错）

**嵌套 `Runtime::new().block_on()`** — `src/app.rs`

| 位置 | 代码 |
|------|------|
| 第 96 行 (`exec_login`) | `Runtime::new().unwrap().block_on(sessions_api.list())` |
| 第 531 行 (`bootstrap`) | `Runtime::new().unwrap().block_on(sessions_api.list())` |

这两处都在已有的 tokio 异步上下文（`runtime.spawn(async move { ... })`）中再次创建新 Runtime 并 `block_on`。tokio 多线程调度器下会 panic：

```
Cannot drop a runtime in a context where blocking is not allowed.
```

**修复**：改为直接 `await` 调用，例如：
```rust
// 之前
if let Ok(list) = Runtime::new().unwrap().block_on(sessions_api.list()) { ... }
// 之后
if let Ok(list) = sessions_api.list().await { ... }
```

### 🟡 冗余依赖（Cargo.toml）

| 依赖 | 问题 |
|------|------|
| `thiserror = "2"` | 声明了但代码中零处使用（全部用 `anyhow` 处理错误） |
| `egui_dock = "0.4"` | 声明了但源代码中无任何引用，且引入了 egui v0.21/v0.26 旧版到依赖树，增加编译时间和体积 |
| `copypasta = "0.10"` | 版本偏旧，最新是 0.11，Windows 11 可能有 clipboard 兼容问题 |

**建议**：移除未使用的 `thiserror` 和 `egui_dock`，升级 `copypasta` 到 0.11。

### 🟢 多版本 egui 共存

Cargo.lock 中包含 egui v0.21、v0.26、v0.28 三个大版本（v0.21/v0.26 由 `egui_dock v0.4` 引入）。移除 `egui_dock` 后可消除此问题。

---

## 不需要修改的文件

| 文件 | 原因 |
|------|------|
| `src/state/voice_state.rs` | 纯 UI 状态数据结构，不依赖 livekit |
| `src/state/app_state.rs` | `ApiCommand` 枚举变体保留，无 voice 时在 app.rs 里忽略 |
| `src/ui/shell.rs` | 只做路由分发，不感知 voice |
| `src/api/voice.rs` | HTTP API 调用不依赖 livekit crate |