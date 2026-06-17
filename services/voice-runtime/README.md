# Makima Voice Runtime

Real-time voice AI agent using LiveKit Agents SDK.

## Features

- **VAD (Voice Activity Detection)**: Silero VAD for accurate speech detection
- **STT (Speech-to-Text)**: Deepgram Nova-2 (Chinese support) or OpenAI Whisper fallback
- **LLM**: OpenAI-compatible API (DeepSeek, Groq, etc.)
- **TTS (Text-to-Speech)**: OpenAI TTS with multiple voice options
- **Transport**: LiveKit WebRTC for low-latency audio streaming

## Setup

### 1. Install Dependencies

```bash
cd services/voice-runtime
pip install -e .
```

### 2. Configure Environment

Add to `apps/backend/.env`:

```env
# LiveKit Cloud credentials
LIVEKIT_URL=wss://your-project.livekit.cloud
LIVEKIT_API_KEY=your-api-key
LIVEKIT_API_SECRET=your-api-secret

# Optional: Deepgram for better Chinese STT
DEEPGRAM_API_KEY=your-deepgram-key
```

### 3. Start Voice Agent Worker

```bash
cd services/voice-runtime
python agent.py dev
```

The agent will connect to LiveKit Cloud and wait for participants.

### 4. Start Voice Client (CLI)

In another terminal:

```bash
cd services/voice-runtime
python client.py --room makima-voice-room
```

The client will:
1. Connect to the same LiveKit room
2. Capture your microphone audio
3. Send to the agent
4. Receive and play agent responses

## Architecture

```
┌─────────────────────────────────┐
│  CLI Voice Client (client.py)   │
│  - Microphone capture (pyaudio) │
│  - WebRTC to LiveKit            │
│  - Audio playback               │
└──────────────┬──────────────────┘
               │ WebRTC
               ▼
┌─────────────────────────────────┐
│  LiveKit Cloud                  │
│  - Room management              │
│  - Audio routing                │
│  - WebRTC signaling             │
└──────────────┬──────────────────┘
               │ WebRTC
               ▼
┌─────────────────────────────────┐
│  Voice Agent Worker (agent.py)  │
│  - VAD: Silero                  │
│  - STT: Deepgram / Whisper      │
│  - LLM: DeepSeek / OpenAI       │
│  - TTS: OpenAI TTS              │
│  - Makima persona               │
└─────────────────────────────────┘
```

## Customization

### Change TTS Voice

Edit `agent.py` and change the `voice` parameter:

```python
tts = openai.TTS(
    model="tts-1",
    voice="nova",  # Options: alloy, echo, fable, onyx, nova, shimmer
    ...
)
```

### Change System Prompt

Edit the `system_prompt` variable in `agent.py`.

### Use Different LLM

Update `MAKIMA_LLM_API_BASE` and `MAKIMA_LLM_MODEL` in `.env`.

## Troubleshooting

### "pyaudio not installed"

```bash
# Windows
pip install pyaudio

# macOS
brew install portaudio
pip install pyaudio

# Linux
sudo apt-get install portaudio19-dev
pip install pyaudio
```

### "Microphone not detected"

Check your system's microphone settings and ensure it's not muted.

### "Connection failed"

Verify LiveKit credentials in `.env` and ensure the URL is correct.

## Future Enhancements

- [ ] Support for local Whisper model (no API key needed)
- [ ] Edge TTS integration (free, high-quality Chinese)
- [ ] Voice cloning with Fish Speech
- [ ] Multi-language support
- [ ] Emotion detection and adaptive responses