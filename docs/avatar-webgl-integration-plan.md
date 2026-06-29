# Avatar 面板接入 Unity WebGL 实现计划

**状态**: 待实施  
**创建日期**: 2026-06-30  
**相关模块**: `apps/desktop-egui/src/ui/panels/avatar.rs`, `character-webgl/`, `Cargo.toml`

---

## 1. 概述

当前 Avatar 面板 (`avatar.rs`) 是一个占位实现，仅展示硬编码的像素画角色。本计划旨在将其升级为真实的 Unity WebGL 3D 角色渲染视图。

### 技术栈

| 层 | 技术 |
|---|------|
| 桌面框架 | egui/eframe (Rust) + glow backend |
| WebView | [wry](https://github.com/nicedoc/wry) v0.40 |
| 3D 内容 | Unity WebGL build 产物 |
| Feature flag | `avatar = ["wry"]` (已定义于 Cargo.toml) |

---

## 2. 前置条件

### 2.1 Unity 侧需提供的 Build 产物

Unity 构建目标为 **WebGL**，输出以下文件到 `character-webgl/Build/`：

```
character-webgl/
├── Build/
│   ├── character-webgl.loader.js        # Unity loader 脚本
│   ├── character-webgl.framework.js.br  # 压缩的框架代码 (Brotli)
│   ├── character-webgl.wasm.br          # 压缩的 WASM (Brotli)
│   ├── character-webgl.data.br          # 压缩的资源数据 (Brotli)
│   └── StreamingAssets/                 # (可选) 流式资源目录
├── TemplateData/
│   └── *.png, style.css, favicon.ico
└── index.html                           # 入口页面 (已存在)
```

### 2.2 Unity Build Settings 建议

- **Compression Format**: Brotli（当前 index.html 引用 `.br` 文件）
- **Code Optimization**: Size / Speed 权衡（推荐 Size）
- **Enable Exceptions**: None（减小 wasm 体积）
- **WebGL Template**: Default（或自定义最小模板）
- 建议移除 index.html 中的 canvas 尺寸硬编码，改为 CSS 响应式（WebView 嵌入时更灵活）

---

## 3. 实施步骤

### 步骤 1: 补齐 `character-webgl/Build/` 目录

**文件**: `character-webgl/Build/*`

从 Unity 编辑器执行 WebGL Build，确保产物包含：

| 文件 | 用途 | 大小预估 |
|------|------|---------|
| `character-webgl.loader.js` | Unity WebGL Loader | ~200 KB |
| `character-webgl.framework.js.br` | 引擎框架代码 | 5-15 MB |
| `character-webgl.wasm.br` | WASM 二进制 | 10-30 MB |
| `character-webgl.data.br` | 资源数据 | 5-50 MB |

> **注意**: 如果 Unity Build 产物使用 `.gz` 而非 `.br`，需相应修改 `index.html` 中的 loader 配置和 `avatar_webview.rs` 中的 Content-Encoding 处理。

---

### 步骤 2: 新增 `avatar_webview.rs` - WebView 面板实现

**文件**: `apps/desktop-egui/src/ui/panels/avatar_webview.rs`（新建）

#### 2.1 模块职责

- 在编译时通过 `include_dir!` 或 `rust_embed` 将 `character-webgl/` 内容嵌入二进制
- 启动本地 HTTP server（`tiny_http` 或自建），将 WebGL 文件 serve 到 `localhost`
- 使用 `wry` 创建原生 WebView 子窗口，加载 `http://localhost:{PORT}/index.html`
- 将 WebView 嵌入到 egui dock 的 Avatar Tab 区域
- 处理 resize、focus、生命周期事件

#### 2.2 核心数据结构

```rust
/// Avatar WebView 管理器（仅在 avatar feature 启用时编译）
pub struct AvatarWebView {
    /// wry WebView 实例
    webview: wry::WebView,
    /// 当前 WebView 在屏幕上的位置和大小
    bounds: egui::Rect,
    /// 本地 HTTP server 的端口号
    port: u16,
    /// 是否已完成首次加载
    loaded: bool,
}
```

#### 2.3 核心流程

```
1. 应用启动 / 首次切换到 Avatar Tab
   ↓
2. 初始化本地 HTTP server (随机端口)
   ↓
3. 注册路由:
   GET /                  → index.html
   GET /Build/*.js        → application/javascript
   GET /Build/*.wasm.br   → application/wasm + Content-Encoding: br
   GET /Build/*.data.br   → application/octet-stream + Content-Encoding: br
   GET /Build/*.js.br     → application/javascript + Content-Encoding: br
   GET /TemplateData/*    → 静态资源 (image/png, text/css)
   ↓
4. 创建 wry WebView，绑定到 eframe 原生窗口
   ↓
5. egui update() 循环中同步 WebView bounds
   ↓
6. 窗口关闭 / 切换到 Chat Tab 时隐藏 WebView
   ↓
7. 应用退出时销毁 WebView 和 HTTP server
```

#### 2.4 文件嵌入方案（推荐 `rust-embed`）

```toml
# Cargo.toml 中在 [dependencies] 和 [features] 中添加
rust-embed = { version = "8", optional = true }
tiny_http = { version = "0.12", optional = true }

[features]
avatar = ["wry", "rust-embed", "tiny_http"]
```

```rust
// avatar_webview.rs
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../character-webgl/"]
struct WebglAssets;

fn start_asset_server() -> u16 {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let path = request.url().trim_start_matches('/');
            let path = if path.is_empty() { "index.html" } else { path };
            
            if let Some(file) = WebglAssets::get(path) {
                let mime = mime_type(path);
                let response = tiny_http::Response::new(
                    200.into(),
                    vec![
                        tiny_http::Header::from_str("Content-Type", &mime).unwrap(),
                    ],
                    file.data.as_ref(),
                    Some(file.data.len()),
                    None,
                );
                let _ = request.respond(response);
            } else {
                let _ = request.respond(tiny_http::Response::new_empty(404.into()));
            }
        }
    });
    port
}
```

**mime_type 映射:**

| 扩展名 | Content-Type | Content-Encoding |
|--------|-------------|------------------|
| `.html` | `text/html; charset=utf-8` | - |
| `.js` | `application/javascript` | - |
| `.wasm` | `application/wasm` | - |
| `.css` | `text/css` | - |
| `.png` | `image/png` | - |
| `.ico` | `image/x-icon` | - |
| `.br` | (由父文件类型决定) | **`br`** (必须设置) |

---

### 步骤 3: 修改模块结构 - Feature Gate

#### 3.1 修改 `apps/desktop-egui/src/ui/panels/mod.rs`

```rust
pub mod settings;
pub mod login;
pub mod persona;

// Avatar panel: feature-gated
#[cfg(feature = "avatar")]
pub mod avatar_webview;     // ← 新增：WebView 实现

#[cfg(not(feature = "avatar"))]
pub mod avatar;             // ← 现有像素画占位实现（重命名区分）

// 提供统一的 draw 入口
#[cfg(feature = "avatar")]
pub use avatar_webview as avatar_impl;

#[cfg(not(feature = "avatar"))]
pub use avatar as avatar_impl;
```

#### 3.2 修改 `apps/desktop-egui/src/ui/dock.rs`

在 `AppTabViewer::ui()` 中，Avatar tab 的渲染需要支持 WebView 初始化：

```rust
AppDockTab::Avatar => {
    // 现有调用保持不变，draw 函数内部根据 feature 走不同实现
    crate::ui::panels::avatar_impl::draw(ui, self.state);
}
```

> 注意：WebView 的创建和 bounds 更新需要在 `MakimaApp::update()` 中处理，
> 而不是在 `draw()` 中，因为 wry 需要原生窗口句柄（在 `update()` 中可通过 `ctx` 获取）。

#### 3.3 修改 `apps/desktop-egui/src/app.rs`

在 `MakimaApp` 结构体中添加：

```rust
#[cfg(feature = "avatar")]
pub avatar_webview: Option<crate::ui::panels::avatar_webview::AvatarWebView>,
```

在 `update()` 方法中：

```rust
#[cfg(feature = "avatar")]
{
    // 当切换到 Avatar 模式时创建 WebView
    // 当离开 Avatar 模式时隐藏 WebView
    // 在每帧更新 WebView 的 bounds 到 dock 中 Avatar Tab 的位置
}
```

---

### 步骤 4: WebView 窗口嵌入

#### 4.1 挑战

`wry` 创建的原生 WebView 是独立的平台窗口（Windows: HWND, macOS: NSView, Linux: GTK）。需要将其**嵌入**到 eframe 主窗口中，并定位到 Avatar Tab 对应的屏幕区域。

#### 4.2 方案 A: `build_as_child` + 手动 bounds 同步（推荐）

```rust
use wry::WebViewBuilder;
use raw_window_handle::{HasWindowHandle, HasDisplayHandle};

// 在 MakimaApp::update() 中
fn create_or_update_webview(
    &mut self,
    ctx: &egui::Context,
    avatar_rect: egui::Rect,      // Avatar Tab 在屏幕上的像素坐标
) {
    #[cfg(feature = "avatar")]
    {
        let native_handle = ctx.viewport_id();  // egui 0.33 新增 API
        
        if self.avatar_webview.is_none() {
            if let Some(viewport) = ctx.viewport_id() {
                // 创建子窗口 WebView
                let webview = WebViewBuilder::new_as_child(/* parent hwnd */)
                    .with_url(&format!("http://127.0.0.1:{}/index.html", self.avatar_port))
                    .with_bounds(wry::Rect {
                        position: wry::dpi::Position::Logical(
                            wry::dpi::LogicalPosition::new(
                                avatar_rect.min.x as f64,
                                avatar_rect.min.y as f64,
                            )
                        ),
                        size: wry::dpi::Size::Logical(
                            wry::dpi::LogicalSize::new(
                                avatar_rect.width() as f64,
                                avatar_rect.height() as f64,
                            )
                        ),
                    })
                    .with_transparent(false)
                    .build()
                    .expect("Failed to create avatar WebView");
                    
                self.avatar_webview = Some(AvatarWebView {
                    webview,
                    bounds: avatar_rect,
                    port: self.avatar_port,
                    loaded: false,
                });
            }
        } else if let Some(ref webview_state) = self.avatar_webview {
            // 同步 bounds
            if webview_state.bounds != avatar_rect {
                use wry::Rect;
                let _ = webview_state.webview.set_bounds(Rect {
                    position: wry::dpi::Position::Logical(/* ... */),
                    size: wry::dpi::Size::Logical(/* ... */),
                });
            }
        }
    }
}
```

#### 4.3 方案 B: 使用 egui-winit + egui-wgpu（备选）

如果 egui glow backend 暴露原生句柄困难，可考虑迁移到 egui-wgpu backend，通过 winit 更容易获取 `WindowHandle`。

当前项目使用 `eframe` 的 `glow` feature：
```toml
eframe = { version = "0.33", default-features = false, features = ["glow"] }
```

**评估**: 
- `glow` backend 通过 `eframe::Frame` 可以获取 glow context
- egui 0.33 新增了 `viewport_id()` API，可以获取原生窗口句柄
- **先验证 glow backend 是否能拿到 HWND/NSView**，如果不行再考虑切换 backend

#### 4.4 关键 API

| 操作 | wry API | 说明 |
|------|---------|------|
| 创建子窗口 | `WebViewBuilder::new_as_child(hwnd)` | 需要父窗口原生句柄 |
| 设置位置大小 | `webview.set_bounds(Rect)` | dock resize 时调用 |
| 显示/隐藏 | `webview.set_visible(bool)` | 切换 Tab 时使用 |
| 销毁 | `drop(webview)` | 退出时自动清理 |

#### 4.5 获取父窗口句柄

Windows 平台下，eframe glow backend 可以通过 `eframe::glow::Context` 获取原生窗口句柄：

```rust
// 在 eframe::App::update() 中
fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
    #[cfg(feature = "avatar")]
    #[cfg(target_os = "windows")]
    {
        use raw_window_handle::HasWindowHandle;
        if let Some(window_handle) = frame.window_handle() {
            let hwnd = /* 从 RawWindowHandle 提取 HWND */;
        }
    }
}
```

> **注意**: 需要在 `Cargo.toml` 中添加 `raw-window-handle = "0.6"` 依赖。

---

### 步骤 5: Unity ↔ Rust 双向通信

#### 5.1 需求场景

| 方向 | 场景 | 优先级 |
|------|------|--------|
| Rust → Unity | 切换角色表情/动画 | P1 |
| Rust → Unity | 语音口型同步（lip-sync） | P2 |
| Rust → Unity | 切换服装/外观 | P2 |
| Unity → Rust | 点击角色触发交互 | P3 |
| Unity → Rust | 动画播放完成事件 | P3 |

#### 5.2 实现方案: wry IPC + Unity jslib

**Rust → Unity（JavaScript 注入）:**

```rust
// avatar_webview.rs
impl AvatarWebView {
    pub fn send_command(&self, command: &AvatarCommand) {
        let js = match command {
            AvatarCommand::SetExpression(expr) => {
                format!("window.unityInstance.SendMessage('AvatarController', 'SetExpression', '{}');", expr)
            }
            AvatarCommand::PlayAnimation(anim) => {
                format!("window.unityInstance.SendMessage('AvatarController', 'PlayAnimation', '{}');", anim)
            }
            AvatarCommand::LipSync(blendshapes) => {
                let json = serde_json::to_string(blendshapes).unwrap();
                format!("window.unityInstance.SendMessage('AvatarController', 'UpdateLipSync', '{}');", json)
            }
        };
        let _ = self.webview.evaluate_script(&js);
    }
}

pub enum AvatarCommand {
    SetExpression(String),    // happy, sad, angry, neutral
    PlayAnimation(String),    // idle, wave, dance
    LipSync(HashMap<String, f32>), // blendshape name → weight
}
```

**Unity → Rust（自定义协议 / JavaScript 回调）:**

方案：在 Unity 中通过 jslib 插件调用 `window.ipc.postMessage(msg)`：

```javascript
// Unity 侧: Assets/Plugins/WebGL/RustBridge.jslib
mergeInto(LibraryManager.library, {
    SendToHost: function(msgPtr) {
        var msg = UTF8ToString(msgPtr);
        window.ipc.postMessage(msg);
    },
    OnHostMessage: function() {
        // Unity 侧注册的回调会被存在这里
        return window._hostMessageCallback || '';
    }
});
```

```rust
// Rust 侧: 通过 wry 的 IPC 机制接收消息
use wry::application::event_loop::EventLoopProxy;

// 注册自定义协议
webview_builder = webview_builder.with_ipc_handler(move |msg| {
    let msg: String = msg.body().into();
    // 解析 Unity 发来的消息
    if let Ok(event) = serde_json::from_str::<UnityEvent>(&msg) {
        match event {
            UnityEvent::CharacterClicked => {
                // 触发 Rust 主线程响应
            }
            UnityEvent::AnimationComplete { name } => {
                // ...
            }
        }
    }
});
```

#### 5.3 消息协议（JSON）

```rust
// Unity → Host
{
    "type": "character_clicked",
    "bone": "head",
    "position": { "x": 0.5, "y": 0.7 }
}

{
    "type": "animation_complete",
    "name": "wave",
    "duration": 2.3
}

// Host → Unity
{
    "type": "set_expression",
    "expression": "happy"
}

{
    "type": "play_animation",
    "name": "wave",
    "loop": false
}

{
    "type": "update_lipsync",
    "blendshapes": {
        "jawOpen": 0.5,
        "mouthClose": 0.2,
        "lipCorner": 0.1
    }
}
```

---

## 4. 文件变更清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `character-webgl/Build/*` | **导入** | Unity WebGL build 产物 |
| `Cargo.toml` | **修改** | 添加 `rust-embed`, `tiny_http`, `raw-window-handle` 可选依赖；更新 `avatar` feature |
| `src/ui/panels/avatar_webview.rs` | **新建** | WebView 完整实现 |
| `src/ui/panels/mod.rs` | **修改** | Feature gate avatar 模块 |
| `src/ui/panels/avatar.rs` | **保留** | 作为无 avatar feature 时的降级占位 |
| `src/ui/dock.rs` | **修改** | Avatar tab 的 `ui()` 回调适配 |
| `src/app.rs` | **修改** | 添加 `AvatarWebView` 状态、创建/销毁/同步逻辑 |
| `src/main.rs` | **轻微修改** | 如有必要 |

---

## 5. 编译与构建

```bash
# 不带 Avatar（默认，使用像素画占位）
cargo build --release

# 带 Avatar WebView（完整 Unity WebGL 3D 渲染）
cargo build --release --features avatar

# 同时启用 Voice + Avatar
cargo build --release --features "voice,avatar"
```

### 5.1 平台注意事项

| 平台 | WebView 后端 | 备注 |
|------|-------------|------|
| Windows | WebView2 (Edge Chromium) | 需要安装 Edge 或 WebView2 Runtime（Win10+ 自带） |
| macOS | WKWebView | 系统自带 |
| Linux | WebKitGTK | 需要安装 `libwebkit2gtk-4.1-dev` |

### 5.2 Windows Developer Mode（可选）

如果在 Windows 上遇到 WebView2 加载 `localhost` 被阻止的问题，可能需要以 `--allow-loopback-in-local-network` 运行，或使用 `127.0.0.1` 代替 `localhost`（已在方案中使用 `127.0.0.1`）。

---

## 6. 风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| glow backend 无法获取原生句柄 | 中 | 调研后可能需要切换到 winit/WGPU backend；先写 PoC |
| WebView 并非完美嵌入，有 z-order 问题 | 中 | WebView 是独立窗口，egui dock 内容在 WebView 上方时会遮挡；可把 Avatar tab 设计为独占区域 |
| Unity WebGL 大文件(>20MB) 嵌入二进制导致体积膨胀 | 低 | 采用运行时从文件系统加载的备选方案，而非 `rust-embed` |
| Brotli 解压需要浏览器支持 | 低 | WebView 基于 Chromium/WebKit，原生支持 br；如不用压缩可关闭 Unity 的 Compression |
| 跨平台一致性 | 中 | 主要在 Windows 开发；Linux/macOS 需单独测试 |

---

## 7. 里程碑

| 阶段 | 内容 | 预估工时 |
|------|------|---------|
| M1 - PoC | 静态 WebView 嵌入 + 加载 Unity WebGL | 2-3 天 |
| M2 - 嵌入完善 | bounds 同步、切换 Tab、resize | 1-2 天 |
| M3 - 双向通信 | IPC 协议 + Unity jslib 插件 | 2-3 天 |
| M4 - 语音联动 | 语音驱动口型/表情 | 2-3 天 |
| M5 - 打磨 | 性能优化、错误处理、跨平台测试 | 1-2 天 |

---

## 8. 参考资料

- [wry 文档](https://docs.rs/wry/latest/wry/)
- [egui/eframe 文档](https://docs.rs/eframe/latest/eframe/)
- [Unity WebGL 开发文档](https://docs.unity3d.com/Manual/webgl-developing.html)
- [Unity WebGL jslib 插件](https://docs.unity3d.com/Manual/webgl-interactingwithbrowserscripting.html)
- [raw-window-handle](https://docs.rs/raw-window-handle/latest/raw_window_handle/)