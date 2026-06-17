"""Test script for Makima Voice Runtime.

Tests:
1. Dependencies check
2. LiveKit connection
3. VAD model loading
4. Microphone capture
5. Agent worker startup
"""

import asyncio
import sys
import os
from pathlib import Path

# Add parent paths to sys.path
sys.path.insert(0, str(Path(__file__).parent.parent / "apps" / "backend" / "src"))
sys.path.insert(0, str(Path(__file__).parent))

# Load environment
from dotenv import load_dotenv
env_path = Path(__file__).parent.parent.parent / "apps" / "backend" / ".env"
load_dotenv(env_path)


def test_dependencies():
    """Test 1: Check if all dependencies are installed."""
    print("\n" + "="*60)
    print("TEST 1: Dependencies Check")
    print("="*60)
    
    dependencies = {
        "livekit": "LiveKit SDK",
        "livekit.agents": "LiveKit Agents",
        "livekit.plugins.openai": "LiveKit OpenAI Plugin",
        "livekit.plugins.silero": "LiveKit Silero Plugin",
        "pyaudio": "PyAudio (microphone)",
        "numpy": "NumPy (audio processing)",
    }
    
    all_ok = True
    for module, name in dependencies.items():
        try:
            __import__(module)
            print(f"✓ {name}: OK")
        except ImportError as e:
            print(f"✗ {name}: MISSING - {e}")
            all_ok = False
    
    if not all_ok:
        print("\n❌ Some dependencies missing. Install with:")
        print("   cd services/voice-runtime")
        print("   pip install -e .")
        return False
    
    print("\n✓ All dependencies OK")
    return True


def test_environment():
    """Test 2: Check environment variables."""
    print("\n" + "="*60)
    print("TEST 2: Environment Variables")
    print("="*60)
    
    required = {
        "LIVEKIT_URL": "LiveKit Cloud URL",
        "LIVEKIT_API_KEY": "LiveKit API Key",
        "LIVEKIT_API_SECRET": "LiveKit API Secret",
        "MAKIMA_LLM_API_KEY": "LLM API Key",
    }
    
    all_ok = True
    for var, name in required.items():
        value = os.getenv(var)
        if value:
            # Mask sensitive values
            masked = value[:8] + "..." if len(value) > 8 else "***"
            print(f"✓ {name}: {masked}")
        else:
            print(f"✗ {name}: NOT SET")
            all_ok = False
    
    if not all_ok:
        print("\n❌ Some environment variables missing.")
        print("   Check apps/backend/.env file")
        return False
    
    print("\n✓ All environment variables OK")
    return True


async def test_livekit_connection():
    """Test 3: Test LiveKit Cloud connection."""
    print("\n" + "="*60)
    print("TEST 3: LiveKit Connection")
    print("="*60)
    
    try:
        from livekit import api, rtc
        
        livekit_url = os.getenv("LIVEKIT_URL")
        livekit_api_key = os.getenv("LIVEKIT_API_KEY")
        livekit_api_secret = os.getenv("LIVEKIT_API_SECRET")
        
        print(f"Connecting to: {livekit_url}")
        
        # Generate a test token
        token = (
            api.AccessToken(api_key=livekit_api_key, api_secret=livekit_api_secret)
            .with_identity("test-user")
            .with_name("Test User")
            .with_grants(
                api.VideoGrants(
                    room_join=True,
                    room="test-room",
                )
            )
            .to_jwt()
        )
        
        print(f"✓ Token generated")
        
        # Try to connect
        room = rtc.Room()
        await room.connect(livekit_url, token)
        print(f"✓ Connected to room: {room.name}")
        
        # Disconnect
        await room.disconnect()
        print(f"✓ Disconnected")
        
        print("\n✓ LiveKit connection OK")
        return True
        
    except Exception as e:
        print(f"\n✗ Connection failed: {e}")
        return False


def test_vad_model():
    """Test 4: Test VAD model loading."""
    print("\n" + "="*60)
    print("TEST 4: VAD Model (Silero)")
    print("="*60)
    
    try:
        from livekit.plugins import silero
        
        print("Loading Silero VAD model...")
        vad = silero.VAD.load()
        print(f"✓ VAD model loaded: {type(vad).__name__}")
        
        print("\n✓ VAD model OK")
        return True
        
    except Exception as e:
        print(f"\n✗ VAD model failed: {e}")
        return False


def test_microphone():
    """Test 5: Test microphone capture."""
    print("\n" + "="*60)
    print("TEST 5: Microphone Capture")
    print("="*60)
    
    try:
        import pyaudio
        
        pa = pyaudio.PyAudio()
        
        # List audio devices
        print("Available audio devices:")
        for i in range(pa.get_device_count()):
            info = pa.get_device_info_by_index(i)
            if info['maxInputChannels'] > 0:
                print(f"  [{i}] {info['name']} (inputs: {info['maxInputChannels']})")
        
        # Try to open default input
        stream = pa.open(
            format=pyaudio.paInt16,
            channels=1,
            rate=48000,
            input=True,
            frames_per_buffer=480,
        )
        
        print(f"✓ Microphone opened (48kHz, mono)")
        
        # Read a small chunk to verify
        data = stream.read(480, exception_on_overflow=False)
        print(f"✓ Captured {len(data)} bytes")
        
        stream.stop_stream()
        stream.close()
        pa.terminate()
        
        print("\n✓ Microphone OK")
        return True
        
    except Exception as e:
        print(f"\n✗ Microphone failed: {e}")
        print("\nTroubleshooting:")
        print("  - Check if microphone is connected")
        print("  - Check system audio settings")
        print("  - On Windows, install Visual C++ Redistributable")
        return False


async def run_all_tests():
    """Run all tests."""
    print("\n" + "="*60)
    print("MAKIMA VOICE RUNTIME - TEST SUITE")
    print("="*60)
    
    results = {}
    
    # Test 1: Dependencies
    results["dependencies"] = test_dependencies()
    if not results["dependencies"]:
        print("\n⚠️  Stopping tests due to missing dependencies")
        return results
    
    # Test 2: Environment
    results["environment"] = test_environment()
    
    # Test 3: LiveKit Connection
    results["livekit"] = await test_livekit_connection()
    
    # Test 4: VAD Model
    results["vad"] = test_vad_model()
    
    # Test 5: Microphone
    results["microphone"] = test_microphone()
    
    # Summary
    print("\n" + "="*60)
    print("TEST SUMMARY")
    print("="*60)
    
    for test_name, passed in results.items():
        status = "✓ PASS" if passed else "✗ FAIL"
        print(f"{status}: {test_name}")
    
    all_passed = all(results.values())
    
    if all_passed:
        print("\n🎉 All tests passed! Voice system is ready.")
        print("\nNext steps:")
        print("  1. Start the agent worker:")
        print("     cd services/voice-runtime")
        print("     python agent.py dev")
        print("\n  2. In another terminal, start the client:")
        print("     python client.py")
    else:
        print("\n⚠️  Some tests failed. Check the output above.")
    
    return results


def main():
    """Main entry point."""
    try:
        asyncio.run(run_all_tests())
    except KeyboardInterrupt:
        print("\n\nTest interrupted by user")
        sys.exit(1)


if __name__ == "__main__":
    main()