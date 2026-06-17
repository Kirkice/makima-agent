"""Makima Voice Agent - Real-time voice AI using LiveKit Agents SDK.

This is the Voice Agent Worker that:
1. Connects to LiveKit Cloud
2. Listens for incoming voice rooms
3. Runs the voice pipeline: VAD → STT → LLM → TTS
4. Bridges with Makima's backend for LLM responses
"""

import asyncio
import logging
import os
from pathlib import Path

from dotenv import load_dotenv

# Load environment variables from .env file
env_path = Path(__file__).parent.parent.parent.parent / "apps" / "backend" / ".env"
load_dotenv(env_path)

from livekit import rtc
from livekit.agents import (
    AutoSubscribe,
    JobContext,
    JobProcess,
    WorkerOptions,
    cli,
    llm,
)
from livekit.agents.pipeline import VoicePipelineAgent
from livekit.plugins import openai, silero, deepgram

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("makima-voice-agent")


def prewarm(proc: JobProcess):
    """Pre-warm the VAD model for faster startup."""
    proc.userdata["vad"] = silero.VAD.load()


async def entrypoint(ctx: JobContext):
    """Main entry point for the voice agent.
    
    This is called when a new room is created or the agent joins a room.
    """
    logger.info(f"Connecting to room: {ctx.room.name}")
    await ctx.connect(auto_subscribe=AutoSubscribe.AUDIO_ONLY)
    
    # Wait for a participant to join
    participant = await ctx.wait_for_participant()
    logger.info(f"Participant joined: {participant.identity}")
    
    # Get LLM configuration from environment
    llm_api_key = os.getenv("MAKIMA_LLM_API_KEY", "")
    llm_api_base = os.getenv("MAKIMA_LLM_API_BASE", "https://api.deepseek.com")
    llm_model = os.getenv("MAKIMA_LLM_MODEL", "deepseek-v4-flash")
    
    # Get LiveKit configuration
    livekit_url = os.getenv("LIVEKIT_URL", "")
    livekit_api_key = os.getenv("LIVEKIT_API_KEY", "")
    livekit_api_secret = os.getenv("LIVEKIT_API_SECRET", "")
    
    if not llm_api_key:
        logger.error("MAKIMA_LLM_API_KEY not set in environment")
        return
    
    # Initialize the voice pipeline
    # VAD: Silero (local, fast, accurate)
    # STT: Deepgram (cloud, supports Chinese)
    # LLM: OpenAI-compatible API (DeepSeek)
    # TTS: OpenAI-compatible TTS or fallback
    vad = ctx.proc.userdata.get("vad") or silero.VAD.load()
    
    # Configure LLM - use OpenAI plugin with custom base_url for DeepSeek
    llm_client = openai.LLM.with_groq(
        model=llm_model,
        api_key=llm_api_key,
        base_url=llm_api_base + "/v1" if not llm_api_base.endswith("/v1") else llm_api_base,
    )
    
    # Configure STT - use Deepgram for better Chinese support
    # Fallback to Whisper if Deepgram key not available
    deepgram_api_key = os.getenv("DEEPGRAM_API_KEY", "")
    if deepgram_api_key:
        stt = deepgram.STT(
            model="nova-2",
            language="zh",  # Chinese
        )
        logger.info("Using Deepgram STT")
    else:
        # Use OpenAI Whisper as fallback
        stt = openai.STT.with_groq(
            model="whisper-large-v3",
            api_key=llm_api_key,
            base_url=llm_api_base + "/v1" if not llm_api_base.endswith("/v1") else llm_api_base,
        )
        logger.info("Using OpenAI Whisper STT (fallback)")
    
    # Configure TTS - use OpenAI TTS
    tts = openai.TTS.with_groq(
        model="tts-1",
        voice="nova",  # Can be changed to other voices
        api_key=llm_api_key,
        base_url=llm_api_base + "/v1" if not llm_api_base.endswith("/v1") else llm_api_base,
    )
    logger.info("Using OpenAI TTS")
    
    # System prompt for Makima persona
    system_prompt = """你是玛奇玛（Makima），一个冷静、优雅、有控制力的AI助手。

你的核心特质：
- 冷静、优雅、克制
- 说话简洁有力，不啰嗦
- 有自己的判断和观点
- 温和但有边界

回答时：
- 用中文回答（除非用户用其他语言）
- 保持简洁，一般不超过3句话
- 直接给出答案或观点
- 语气平稳，带一点温和的权威感
"""
    
    # Create the voice pipeline agent
    agent = VoicePipelineAgent(
        vad=vad,
        stt=stt,
        llm=llm_client,
        tts=tts,
        chat_ctx=llm.ChatContext().append(
            role="system",
            text=system_prompt,
        ),
    )
    
    # Start the agent with the participant
    agent.start(ctx.room, participant)
    
    # Send initial greeting
    await asyncio.sleep(1)  # Brief delay to ensure connection
    await agent.say("你好，我是玛奇玛。有什么可以帮你的吗？", allow_interruptions=True)
    
    logger.info("Voice agent started and ready")


if __name__ == "__main__":
    # Run the voice agent worker
    cli.run_app(
        WorkerOptions(
            entrypoint_fnc=entrypoint,
            prewarm_fnc=prewarm,
        ),
    )