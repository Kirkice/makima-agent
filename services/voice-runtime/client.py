"""Makima Voice Client - CLI voice client using LiveKit.

This is a simple CLI client that:
1. Connects to LiveKit Cloud
2. Captures microphone audio
3. Sends to LiveKit room
4. Receives and plays agent audio

Usage:
    python client.py [--room ROOM_NAME]
"""

import argparse
import asyncio
import logging
import os
import sys
from pathlib import Path

from dotenv import load_dotenv

# Load environment variables
env_path = Path(__file__).parent.parent.parent / ".env"
load_dotenv(env_path)

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("makima-voice-client")


async def run_client(room_name: str):
    """Run the voice client."""
    try:
        import pyaudio
    except ImportError:
        logger.error("pyaudio not installed. Install with: pip install pyaudio")
        sys.exit(1)
    
    try:
        from livekit import rtc
    except ImportError:
        logger.error("livekit not installed. Install with: pip install livekit")
        sys.exit(1)
    
    # Get LiveKit configuration
    livekit_url = os.getenv("LIVEKIT_URL", "")
    livekit_api_key = os.getenv("LIVEKIT_API_KEY", "")
    livekit_api_secret = os.getenv("LIVEKIT_API_SECRET", "")
    
    if not all([livekit_url, livekit_api_key, livekit_api_secret]):
        logger.error("LiveKit configuration missing in .env file")
        logger.error("Required: LIVEKIT_URL, LIVEKIT_API_KEY, LIVEKIT_API_SECRET")
        sys.exit(1)
    
    # Generate a token for the client
    from livekit import api
    
    token = (
        api.AccessToken(api_key=livekit_api_key, api_secret=livekit_api_secret)
        .with_identity("cli-user")
        .with_name("CLI User")
        .with_grants(
            api.VideoGrants(
                room_join=True,
                room=room_name,
            )
        )
        .to_jwt()
    )
    
    logger.info(f"Connecting to {livekit_url}...")
    logger.info(f"Room: {room_name}")
    
    # Create LiveKit room connection
    room = rtc.Room()
    
    # Audio callback for receiving audio from agent
    def on_track_subscribed(track: rtc.Track, publication: rtc.TrackPublication, participant: rtc.RemoteParticipant):
        if track.kind == rtc.TrackKind.KIND_AUDIO:
            logger.info(f"Audio track subscribed from {participant.identity}")
            # TODO: Play audio through speakers
            # This requires implementing audio playback with pyaudio
    
    room.on("track_subscribed", on_track_subscribed)
    
    # Connect to the room
    await room.connect(livekit_url, token)
    logger.info(f"Connected to room: {room.name}")
    
    # Create audio source for microphone
    audio_source = rtc.AudioSource(48000, 1)  # 48kHz, mono
    track = rtc.LocalAudioTrack.create_audio_track("microphone", audio_source)
    
    # Publish the track
    options = rtc.TrackPublishOptions()
    options.source = rtc.TrackSource.SOURCE_MICROPHONE
    publication = await room.local_participant.publish_track(track, options)
    logger.info(f"Microphone published: {publication.sid}")
    
    # Initialize pyaudio for microphone capture
    pa = pyaudio.PyAudio()
    
    # Open microphone stream
    mic_stream = pa.open(
        format=pyaudio.paInt16,
        channels=1,
        rate=48000,
        input=True,
        frames_per_buffer=480,  # 10ms at 48kHz
    )
    
    logger.info("Microphone active. Press Ctrl+C to stop.")
    print("\n🎤 Listening... (Press Ctrl+C to stop)")
    
    try:
        # Continuously capture and send audio
        while True:
            # Read audio from microphone
            audio_data = mic_stream.read(480, exception_on_overflow=False)
            
            # Convert to LiveKit audio frame
            # Audio data is 16-bit PCM, need to convert to float32
            import numpy as np
            audio_array = np.frombuffer(audio_data, dtype=np.int16).astype(np.float32) / 32768.0
            
            # Create audio frame
            frame = rtc.AudioFrame(
                data=audio_array.tobytes(),
                sample_rate=48000,
                num_channels=1,
                samples_per_channel=480,
            )
            
            # Send to LiveKit
            await audio_source.capture_frame(frame)
            
    except KeyboardInterrupt:
        logger.info("Stopping...")
    finally:
        # Cleanup
        mic_stream.stop_stream()
        mic_stream.close()
        pa.terminate()
        await room.disconnect()
        logger.info("Disconnected")


def main():
    parser = argparse.ArgumentParser(description="Makima Voice Client")
    parser.add_argument(
        "--room",
        default="makima-voice-room",
        help="Room name to join (default: makima-voice-room)",
    )
    args = parser.parse_args()
    
    try:
        asyncio.run(run_client(args.room))
    except KeyboardInterrupt:
        print("\nGoodbye!")


if __name__ == "__main__":
    main()
