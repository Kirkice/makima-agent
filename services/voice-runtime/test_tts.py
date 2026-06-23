"""Quick Fish Audio TTS smoke test.

Usage:
    python test_tts.py [text]

Reads API key and reference ID from the repo root .env.
Plays the synthesised audio via pygame (if available) or writes to a temp file.
"""

import asyncio
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from dotenv import load_dotenv

_env_path = Path(__file__).parent.parent.parent / ".env"
load_dotenv(_env_path)

import httpx

DEFAULT_TEXT = "你好，我是玛奇玛。有什么事吗？"


def _load_config() -> tuple[str, str, str]:
    return (
        os.getenv("MAKIMA_FISH_AUDIO_KEY", ""),
        os.getenv("MAKIMA_FISH_AUDIO_REFERENCE_ID", ""),
        os.getenv("MAKIMA_FISH_AUDIO_BASE_URL", "https://api.fish.audio").rstrip("/"),
    )


async def synthesize(text: str, out_path: str) -> bool:
    api_key, ref_id, base = _load_config()

    if not api_key:
        print("ERROR: MAKIMA_FISH_AUDIO_KEY is not set in .env")
        return False
    if not ref_id:
        print("ERROR: MAKIMA_FISH_AUDIO_REFERENCE_ID is not set in .env")
        return False

    print(f"Text:        {text}")
    print(f"Reference:   {ref_id}")
    print(f"Base URL:    {base}")
    print(f"Output:      {out_path}")
    print()

    async with httpx.AsyncClient(timeout=30.0) as client:
        resp = await client.post(
            f"{base}/v1/tts",
            headers={
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json",
            },
            json={
                "text": text,
                "reference_id": ref_id,
                "format": "wav",
                "normalize": True,
            },
        )

        if resp.status_code != 200:
            print(f"ERROR: HTTP {resp.status_code} — {resp.text[:300]}")
            return False

        with open(out_path, "wb") as f:
            f.write(resp.content)

        print(f"OK — {len(resp.content)} bytes written to {out_path}")
        return True


def _play_wav(path: str) -> None:
    """Try to play the WAV file with available players."""
    # Try pygame first
    try:
        import pygame
        if not pygame.mixer.get_init():
            pygame.mixer.init(frequency=24000)
        pygame.mixer.music.load(path)
        pygame.mixer.music.play()
        while pygame.mixer.music.get_busy():
            time.sleep(0.1)
        pygame.mixer.music.unload()
        return
    except ImportError:
        pass

    # Fallback: Windows WMPlayer COM
    if sys.platform == "win32":
        ps = (
            "$p = New-Object -ComObject WMPlayer.OCX; "
            f"$p.URL = '{path}'; "
            "$p.controls.play(); "
            "while($p.playState -ne 1){ Start-Sleep -Milliseconds 300 }; "
            "$p.close()"
        )
        subprocess.run(["powershell", "-c", ps], capture_output=True, text=True, timeout=30)
        return

    # Fallback: ffplay / aplay
    for player in ["ffplay", "aplay", "paplay"]:
        try:
            subprocess.run([player, path], capture_output=True, timeout=30)
            return
        except FileNotFoundError:
            continue

    print("(no audio player available — file saved for manual playback)")


def main():
    text = " ".join(sys.argv[1:]) if len(sys.argv) > 1 else DEFAULT_TEXT
    out = os.path.join(tempfile.gettempdir(), "makima_fish_tts_test.wav")

    print("=" * 50)
    print("Fish Audio TTS — Smoke Test")
    print("=" * 50)

    ok = asyncio.run(synthesize(text, out))
    if ok:
        print(f"\nPlaying: {out}")
        _play_wav(out)

    try:
        if ok:
            os.remove(out)
    except OSError:
        pass


if __name__ == "__main__":
    main()
