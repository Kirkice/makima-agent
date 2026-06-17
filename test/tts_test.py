"""Quick TTS + playback test."""
import asyncio
import os
import sys
import time
import tempfile

print("=== TTS Playback Test ===\n")

# Step 1: Generate TTS
print("[1] Generating speech with edge-tts...")
import edge_tts

tmp = os.path.join(tempfile.gettempdir(), "makima_test.mp3")
try:
    asyncio.run(
        edge_tts.Communicate(
            "你好，我是玛奇玛。有什么事吗？",
            voice="zh-CN-XiaoxiaoNeural",
            rate="-5%",
            pitch="-2Hz",
        ).save(tmp)
    )
    size = os.path.getsize(tmp)
    print(f"    OK - File: {tmp} ({size} bytes)")
except Exception as e:
    print(f"    FAIL: {e}")
    sys.exit(1)

# Step 2: Play with pygame
print("\n[2] Playing with pygame...")
try:
    import pygame
    if not pygame.mixer.get_init():
        pygame.mixer.init(frequency=24000)
    pygame.mixer.music.load(tmp)
    pygame.mixer.music.play()
    print(f"    Playing... (waiting for finish)")
    while pygame.mixer.music.get_busy():
        time.sleep(0.1)
    print(f"    OK - Playback finished")
    pygame.mixer.music.unload()
except Exception as e:
    print(f"    FAIL: {e}")
    import traceback
    traceback.print_exc()

# Step 3: Cleanup
try:
    os.remove(tmp)
except:
    pass

print("\n=== Test Complete ===")