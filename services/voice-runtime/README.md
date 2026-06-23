# Makima Voice Runtime

实时语音对话接口，使用 LiveKit RTC + Fish Audio 实现与后端 Chat Agent 的语音交互。

## 架构

```
用户麦克风 → LiveKit RTC → RMS 静音检测 → Fish Audio ASR
                                              ↓
用户扬声器 ← LiveKit RTC ← Fish Audio TTS ← 后端 /tasks API (SSE)
                                             ↑
                                     完整 Agent 能力
                                     (PromptEngine, Persona, Memory, Knowledge, Tools)
```

**核心原则**: Voice Runtime 不是独立 Agent，它是后端 Chat 系统的实时语音接口。所有 LLM 推理、记忆、知识库、工具调用都由后端统一处理。

## 功能特性

- **音频传输**: LiveKit RTC (WebRTC)
- **语音识别 (STT)**: Fish Audio ASR
- **语音合成 (TTS)**: Fish Audio TTS
- **静音检测**: RMS 阈值检测（简单可靠）
- **Agent 推理**: 后端 `/tasks` API（复用完整 Agent 能力）

## 文件结构

| 文件 | 用途 |
|------|------|
| `agent.py` | 语音循环主程序 |
| `fish_audio.py` | Fish Audio ASR/TTS 异步封装 |
| `client.py` | LiveKit 客户端（已废弃，保留参考） |
| `test_voice.py` | 完整测试套件 |
| `test_tts.py` | TTS 快速测试 |
| `pyproject.toml` | 依赖声明 |

## 环境配置

在根目录 `.env` 中添加：

```bash
# LiveKit Cloud
LIVEKIT_URL=wss://your-project.livekit.cloud
LIVEKIT_API_KEY=your-api-key
LIVEKIT_API_SECRET=your-api-secret

# Fish Audio
MAKIMA_FISH_AUDIO_KEY=your-fish-audio-api-key
MAKIMA_FISH_AUDIO_REFERENCE_ID=your-voice-model-id
# MAKIMA_FISH_AUDIO_BASE_URL=https://api.fish.audio  # 可选

# 后端认证（Voice Agent 用）
MAKIMA_CLI_USERNAME=makima-voice
MAKIMA_CLI_PASSWORD=makima-voice-password

# 后端地址（默认 http://127.0.0.1:8000）
# MAKIMA_BACKEND_URL=http://127.0.0.1:8000
```

## 快速开始

### 1. 安装依赖

```bash
cd services/voice-runtime
pip install -e .
```

依赖极简：
- `livekit>=0.17.0` (仅 RTC SDK)
- `httpx>=0.27.0`
- `numpy>=1.24.0`
- `python-dotenv>=1.0.0`

### 2. 启动后端

```bash
cd apps/backend
python -m makima.app
```

### 3. 启动 Voice Agent

```bash
cd services/voice-runtime
python agent.py
```

Agent 会：
1. 连接 LiveKit Cloud
2. 创建房间 `makima-voice-room`
3. 等待用户加入
4. 播放欢迎语
5. 进入语音循环（听 → 识别 → 后端推理 → 合成 → 播放）

## 工作原理

### 语音循环 (Voice Loop)

```python
while True:
    # 1. 监听音频帧
    audio_frame = await receive_audio()
    
    # 2. RMS 静音检测
    if is_speaking(audio_frame):
        buffer_audio(audio_frame)
    elif has_silence(buffer):
        # 3. 发送给 Fish Audio ASR
        text = await fish_audio.transcribe(buffer)
        
        # 4. 调用后端 Agent
        async for chunk in backend.stream("/tasks", text):
            sentence_buffer += chunk.content
            if sentence_boundary(sentence_buffer):
                # 5. TTS 合成并播放
                audio = await fish_audio.synthesize(sentence_buffer)
                await play_audio(audio)
```

### 静音检测

使用简单的 RMS 阈值：
- **阈值**: 500 (int16 音频)
- **静音时长**: 1.5 秒触发
- **最短语音**: 0.3 秒

### 后端集成

Voice Agent 通过 HTTP 调用后端：
1. **认证**: `POST /auth/login` 或 `/auth/register`
2. **创建会话**: `POST /sessions`
3. **流式推理**: `POST /tasks` (SSE)
   - 监听 `message` 事件
   - 检测句子边界（`。！？!?`）
   - 边收边合成播放

## 测试

### 完整测试套件

```bash
python test_voice.py
```

测试项：
1. 依赖检查
2. 环境变量
3. LiveKit 连接
4. 后端连接
5. Fish Audio TTS

### TTS 快速测试

```bash
python test_tts.py "你好，我是玛奇玛"
```

## 自定义

### 调整静音检测

编辑 `agent.py` 中的参数：

```python
self.silence_threshold = 500      # RMS 阈值（越小越灵敏）
self.silence_duration = 1.5       # 静音时长（秒）
self.min_speech_duration = 0.3    # 最短语音（秒）
```

### 更换语音

编辑 `.env`：

```bash
MAKIMA_FISH_AUDIO_REFERENCE_ID=new-voice-model-id
```

## 故障排除

### "Cannot connect to backend"

确保后端已启动：
```bash
cd apps/backend && python -m makima.app
```

### "LiveKit connection failed"

检查 `LIVEKIT_URL`、`LIVEKIT_API_KEY`、`LIVEKIT_API_SECRET` 是否正确。

### "Fish Audio TTS failed"

检查 `MAKIMA_FISH_AUDIO_KEY` 和 `MAKIMA_FISH_AUDIO_REFERENCE_ID` 是否有效。

### 语音识别不准确

- 提高麦克风音量
- 降低环境噪音
- 调整 `silence_threshold` 参数

## 与 CLI 语音的区别

| 特性 | CLI (`/speak`) | Voice Runtime |
|------|----------------|---------------|
| 传输 | 本地麦克风 | LiveKit WebRTC |
| 延迟 | 高（录完再识别） | 低（实时流） |
| 打断 | 不支持 | 支持（TODO） |
| 多用户 | 不支持 | 支持（LiveKit 房间） |
| 适用场景 | 个人使用 | 远程/多人对话 |
