# WebRTC 构建问题与解决方案

本文档记录了在 Windows 环境下构建 `desktop-egui` 时遇到的 WebRTC 相关问题及解决方案。

## 问题概述

在集成 LiveKit WebRTC 语音通话功能时，`webrtc-sys` crate 的构建脚本在 Windows 上失败，报以下错误：

```
thread 'main' panicked at webrtc-sys-0.3.35/build.rs:109:45:
called `Result::unwrap()` on an `Err` value: Failed to move extracted WebRTC into place
Caused by: 拒绝访问。 (os error 5)
```

## 根本原因

**Windows MAX_PATH 限制（260 字符）**

WebRTC SDK 包含大量深层嵌套的头文件路径，例如：
```
include/third_party/grpc/src/src/core/ext/upb-gen/envoy/extensions/load_balancing_policies/client_side_weighted_round_robin/v3/client_side_weighted_round_robin.upb_minitable.h
```

当 `webrtc-sys` 构建脚本尝试将解压后的文件移动到默认 scratch 目录时：
```
C:\Users\<user>\AppData\Local\scratch\livekit_webrtc\livekit\win-x64-release-webrtc-51ef663\win-x64-release\
```

完整路径超过了 260 字符限制，导致 Windows API 调用失败。

## 解决方案

### 方案：使用短路径 + LK_CUSTOM_WEBRTC 环境变量

1. **创建短路径目录**
   ```powershell
   New-Item -ItemType Directory -Force -Path "C:\lk-webrtc"
   ```

2. **下载 WebRTC SDK**
   ```powershell
   curl.exe -L -o "C:\lk-webrtc\webrtc.zip" `
     "https://github.com/livekit/rust-sdks/releases/download/webrtc-51ef663/webrtc-win-x64-release.zip"
   ```

3. **解压到短路径**
   ```powershell
   Expand-Archive -Path "C:\lk-webrtc\webrtc.zip" -DestinationPath "C:\lk-webrtc\temp" -Force
   Move-Item "C:\lk-webrtc\temp\win-x64-release" "C:\lk-webrtc\webrtc" -Force
   Remove-Item "C:\lk-webrtc\temp" -Recurse -Force
   Remove-Item "C:\lk-webrtc\webrtc.zip" -Force
   ```

4. **设置环境变量**
   
   临时设置（当前终端）：
   ```powershell
   $env:LK_CUSTOM_WEBRTC="C:\lk-webrtc\webrtc"
   cargo build
   ```
   
   永久设置（系统级）：
   ```powershell
   [Environment]::SetEnvironmentVariable("LK_CUSTOM_WEBRTC", "C:\lk-webrtc\webrtc", "User")
   ```

5. **验证目录结构**
   ```
   C:\lk-webrtc\webrtc\
   ├── include\
   ├── lib\
   ├── args.gn
   ├── desktop_capture.ninja
   ├── LICENSE.md
   └── webrtc.ninja
   ```

## 其他尝试过的方案（不推荐）

| 方案 | 结果 | 原因 |
|------|------|------|
| `cargo clean` 后重试 | 失败 | 问题不在缓存，而在路径长度 |
| 管理员权限运行 | 失败 | 权限不是问题，路径长度才是 |
| 关闭 Windows Defender | 失败 | 与杀毒软件无关 |
| 启用 Windows Long Paths | 未测试 | 需要修改注册表，且部分 API 仍不支持 |

## 编译时注意事项

每次编译 `desktop-egui` 时，确保环境变量已设置：

```powershell
cd apps/desktop-egui
$env:LK_CUSTOM_WEBRTC="C:\lk-webrtc\webrtc"
cargo check  # 或 cargo build
```

如果环境变量未设置，`webrtc-sys` 会尝试重新下载并解压，再次遇到 MAX_PATH 问题。

## 参考链接

- [webrtc-sys build.rs 源码](https://github.com/livekit/rust-sdks/blob/main/webrtc-sys/build.rs)
- [LiveKit Rust SDK Releases](https://github.com/livekit/rust-sdks/releases)
- [Windows MAX_PATH 限制](https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation)