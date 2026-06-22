"""Test script for Makima Voice Runtime (simple voice loop edition).

Tests:
1. Dependencies check
2. Environment variables
3. LiveKit connection
4. Backend connection
5. Fish Audio TTS quick check
"""

import asyncio
import sys
import os
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from dotenv import load_dotenv

_env_path = Path(__file__).parent.parent.parent / "apps" / "backend" / ".env"
load_dotenv(_env_path)


def test_dependencies():
    """Test 1: Check core dependencies."""
    print("\n" + "=" * 60)
    print("TEST 1: Dependencies Check")
    print("=" * 60)

    deps = {
        "livekit": "LiveKit SDK (RTC)",
        "httpx": "httpx (HTTP client)",
        "numpy": "NumPy (audio processing)",
    }

    ok = True
    for mod, name in deps.items():
        try:
            __import__(mod)
            print(f"✓ {name}: OK")
        except ImportError as e:
            print(f"✗ {name}: MISSING — {e}")
            ok = False

    if not ok:
        print("\n❌ Some dependencies missing. Install with:")
        print("   cd services/voice-runtime && pip install -e .")
        return False
    print("\n✓ All dependencies OK")
    return True


def test_environment():
    """Test 2: Check environment variables."""
    print("\n" + "=" * 60)
    print("TEST 2: Environment Variables")
    print("=" * 60)

    required = {
        "LIVEKIT_URL": "LiveKit Cloud URL",
        "LIVEKIT_API_KEY": "LiveKit API Key",
        "LIVEKIT_API_SECRET": "LiveKit API Secret",
        "MAKIMA_FISH_AUDIO_KEY": "Fish Audio API Key",
        "MAKIMA_FISH_AUDIO_REFERENCE_ID": "Fish Audio Voice Reference ID",
        "MAKIMA_CLI_USERNAME": "Backend CLI Username",
        "MAKIMA_CLI_PASSWORD": "Backend CLI Password",
    }

    ok = True
    for var, name in required.items():
        val = os.getenv(var)
        if val:
            masked = val[:8] + "..." if len(val) > 8 else "***"
            print(f"✓ {name}: {masked}")
        else:
            print(f"✗ {name}: NOT SET")
            ok = False

    if not ok:
        print("\n❌ Some variables missing. Check apps/backend/.env")
        return False
    print("\n✓ All environment variables OK")
    return True


async def test_livekit_connection():
    """Test 3: LiveKit Cloud connection."""
    print("\n" + "=" * 60)
    print("TEST 3: LiveKit Connection")
    print("=" * 60)

    try:
        from livekit import api, rtc

        url = os.getenv("LIVEKIT_URL", "")
        key = os.getenv("LIVEKIT_API_KEY", "")
        secret = os.getenv("LIVEKIT_API_SECRET", "")

        print(f"Connecting to: {url}")
        token = (
            api.AccessToken(api_key=key, api_secret=secret)
            .with_identity("test-user")
            .with_name("Test User")
            .with_grants(api.VideoGrants(room_join=True, room="test-room"))
            .to_jwt()
        )
        print("✓ Token generated")

        room = rtc.Room()
        await room.connect(url, token)
        print(f"✓ Connected to room: {room.name}")
        await room.disconnect()
        print("✓ Disconnected")

        print("\n✓ LiveKit connection OK")
        return True
    except Exception as e:
        print(f"\n✗ Connection failed: {e}")
        return False


async def test_backend_connection():
    """Test 4: Backend authentication."""
    print("\n" + "=" * 60)
    print("TEST 4: Backend Connection")
    print("=" * 60)

    try:
        import httpx

        backend_url = os.getenv("MAKIMA_BACKEND_URL", "http://127.0.0.1:8000")
        username = os.getenv("MAKIMA_CLI_USERNAME", "makima-voice")
        password = os.getenv("MAKIMA_CLI_PASSWORD", "makima-voice")

        print(f"Backend: {backend_url}")
        print(f"Username: {username}")

        async with httpx.AsyncClient(timeout=10.0) as client:
            resp = await client.post(
                f"{backend_url}/auth/login",
                json={"username": username, "password": password},
            )

            if resp.status_code in (401, 404):
                # Try register
                resp = await client.post(
                    f"{backend_url}/auth/register",
                    json={
                        "username": username,
                        "email": f"{username}@voice.local",
                        "password": password,
                    },
                )

            if resp.status_code in (200, 201):
                token = resp.json()["access_token"]
                print(f"✓ Authenticated (token: {token[:20]}...)")

                # Test session creation
                resp2 = await client.post(
                    f"{backend_url}/sessions",
                    json={"title": "Test Session"},
                    headers={"Authorization": f"Bearer {token}"},
                )
                if resp2.status_code in (200, 201):
                    session_id = resp2.json()["id"]
                    print(f"✓ Session created: {session_id[:12]}...")
                    print("\n✓ Backend connection OK")
                    return True
                else:
                    print(f"✗ Session creation failed: {resp2.status_code}")
                    return False
            else:
                print(f"✗ Auth failed: HTTP {resp.status_code}")
                return False

    except httpx.ConnectError:
        print("✗ Cannot connect to backend — is it running?")
        return False
    except Exception as e:
        print(f"✗ Backend error: {e}")
        return False


async def test_fish_tts():
    """Test 5: Fish Audio TTS quick check."""
    print("\n" + "=" * 60)
    print("TEST 5: Fish Audio TTS")
    print("=" * 60)

    try:
        from fish_audio import synthesize

        print("Synthesising test phrase...")
        result = await synthesize("测试语音合成。")
        if result is not None:
            pcm, sr = result
            duration = len(pcm) / sr
            print(f"✓ TTS OK — {duration:.2f}s audio at {sr} Hz")
            return True
        else:
            print("✗ TTS returned None")
            return False
    except Exception as e:
        print(f"✗ TTS exception: {e}")
        return False


async def run_all_tests():
    print("\n" + "=" * 60)
    print("MAKIMA VOICE RUNTIME — TEST SUITE (Simple Loop)")
    print("=" * 60)

    results = {}
    results["dependencies"] = test_dependencies()
    if not results["dependencies"]:
        print("\n⚠️  Stopping due to missing dependencies")
        return results

    results["environment"] = test_environment()
    results["livekit"] = await test_livekit_connection()
    results["backend"] = await test_backend_connection()
    results["fish_tts"] = await test_fish_tts()

    print("\n" + "=" * 60)
    print("TEST SUMMARY")
    print("=" * 60)
    for name, passed in results.items():
        print(f"{'✓ PASS' if passed else '✗ FAIL'}: {name}")

    if all(results.values()):
        print("\n🎉 All tests passed! Voice system is ready.")
        print("\nNext steps:")
        print("  1. Start the backend:")
        print("     cd apps/backend && python -m makima.app")
        print("\n  2. Start the voice agent:")
        print("     cd services/voice-runtime && python agent.py")
    else:
        print("\n⚠️  Some tests failed. Check output above.")

    return results


def main():
    try:
        asyncio.run(run_all_tests())
    except KeyboardInterrupt:
        print("\n\nTest interrupted by user")
        sys.exit(1)


if __name__ == "__main__":
    main()