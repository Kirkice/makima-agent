import asyncio, edge_tts, os, tempfile, subprocess, sys, time

f = os.path.join(tempfile.gettempdir(), "test_tts.mp3")
print(f"Generating TTS to: {f}")
asyncio.run(edge_tts.Communicate("你好，我是玛奇玛。有什么事吗？", voice="zh-CN-XiaoxiaoNeural").save(f))
print(f"File size: {os.path.getsize(f)} bytes")

print("Playing with WMPlayer COM...")
ps = (
    "$p = New-Object -ComObject WMPlayer.OCX; "
    "$p.URL = '" + f + "'; "
    "$p.controls.play(); "
    "while($p.playState -ne 1){ Start-Sleep -Milliseconds 300 }; "
    "$p.close()"
)
r = subprocess.run(["powershell", "-c", ps], capture_output=True, text=True, timeout=30)
print(f"RC: {r.returncode}")
if r.stderr:
    print(f"ERR: {r.stderr[:300]}")
if r.stdout:
    print(f"OUT: {r.stdout[:300]}")

print("Done")
try:
    os.remove(f)
except:
    pass