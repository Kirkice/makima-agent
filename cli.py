#!/usr/bin/env python3
"""Makima Agent CLI."""

from __future__ import annotations

import argparse
import asyncio
import io
import json
import os
import sys
import threading
import tempfile
import time
import wave
from typing import Optional

import httpx
from prompt_toolkit import PromptSession, prompt as pt_prompt
from prompt_toolkit.formatted_text import HTML
from prompt_toolkit.history import InMemoryHistory
from prompt_toolkit.styles import Style
from rich import box
from rich.align import Align
from rich.console import Console, Group
from rich.live import Live
from rich.markdown import Markdown
from rich.panel import Panel
from rich.spinner import Spinner
from rich.table import Table
from rich.text import Text
from rich.theme import Theme

DEFAULT_SERVER = "http://127.0.0.1:8000"

COLORS = {
    "border": "#b24a56",
    "border_soft": "#5e2630",
    "rose": "#f1a0a7",
    "crimson": "#da6570",
    "gold": "#d7b46a",
    "amber": "#e8c27a",
    "ivory": "#f6f0e8",
    "ink": "#101018",
    "muted": "#9f8e93",
    "shadow": "#09080d",
}

THEME = Theme(
    {
        "info": COLORS["gold"],
        "success": "#c9e4c6",
        "warning": COLORS["amber"],
        "error": f"bold {COLORS['crimson']}",
        "dim": COLORS["muted"],
        "user": f"bold {COLORS['gold']}",
        "agent": f"bold {COLORS['ivory']}",
        "tool": f"bold {COLORS['rose']}",
        "thinking": f"italic {COLORS['rose']}",
        "border": COLORS["border"],
        "border_soft": COLORS["border_soft"],
        "accent": COLORS["rose"],
        "title": f"bold {COLORS['ivory']}",
    }
)

console = Console(theme=THEME, soft_wrap=True, highlight=False)

PROMPT_STYLE = Style.from_dict(
    {
        "prompt": f"bold {COLORS['ivory']}",
        "arrow": f"bold {COLORS['gold']}",
        "label": f"bold {COLORS['rose']}",
        "meta": COLORS["muted"],
    }
)

# 14x15 pixels loaded from LOGO.png. Transparent pixels are treated as empty space.
PIXEL_COLORS = [
    [None, None, None, '#b94d58', '#bc4f5a', '#d05966', '#d15966', '#d15966', '#d05966', '#bc4f5a', '#b94d58', None, None, None],
    [None, None, '#d25864', '#ce5964', '#d15966', '#d25a67', '#d15a67', '#d15a67', '#d25a67', '#d15966', '#ce5964', '#d25864', None, None],
    [None, '#c1515a', '#cf5865', '#d15966', '#d25b67', '#d05966', '#d15b67', '#d15b67', '#d05966', '#d25b67', '#d15966', '#cf5865', '#c1515a', None],
    [None, '#c45862', '#d05863', '#d05964', '#d15663', '#d15966', '#d15a67', '#d15a67', '#d15966', '#d15663', '#d05964', '#d05863', '#c45862', None],
    [None, '#d15965', '#d05965', '#d05966', '#d85f6a', '#a13e4a', '#d25b68', '#d25b68', '#a13e4a', '#d85f6a', '#d05966', '#d05965', '#d15965', None],
    ['#ae4350', '#d15a66', '#a53f4c', '#cf5865', '#ca4d5d', '#a33e4b', '#d25a67', '#d25a67', '#a33e4b', '#ca4d5d', '#cf5865', '#a53f4c', '#d15a66', '#ae4350'],
    ['#b74957', '#b14552', '#a33d4a', '#ae4654', '#ab3b48', '#e0afa3', '#bf4c59', '#bf4c59', '#e0afa3', '#ab3b48', '#ae4654', '#a33d4a', '#b14552', '#b74957'],
    ['#b64856', '#ba4d58', '#8f554e', '#fcebdd', '#fcebdd', '#fcecde', '#fdebde', '#fdebde', '#fcecde', '#fcebdd', '#fcebdd', '#8f554e', '#ba4d58', '#b64856'],
    ['#b64956', '#b44b56', '#4e2a15', '#45210e', '#42200e', '#975d52', '#fceadc', '#fceadc', '#975d52', '#42200e', '#45210e', '#4e2a15', '#b44b56', '#b64956'],
    ['#b74b58', '#b54b56', '#814d43', '#fefbfc', '#f8c819', '#fdf7f0', '#fdeadd', '#fdeadd', '#fdf7f0', '#f8c819', '#fefbfc', '#814d43', '#b54b56', '#b74b58'],
    ['#b54b56', '#b14953', '#b74b58', '#fceade', '#fde9d7', '#fceadd', '#fceadc', '#fceadc', '#fceadd', '#fde9d7', '#fceade', '#b74b58', '#b14953', '#b54b56'],
    [None, '#a7424f', '#a4424e', '#fbeadd', '#fbebdd', '#fdebdd', '#af6f6a', '#af6f6a', '#fdebdd', '#fbebdd', '#fbeadd', '#a4424e', '#a7424f', None],
    [None, '#a7414d', None, '#834e66', '#f8ded4', '#f9e4d9', '#ecc7ba', '#ecc7ba', '#f9e4d9', '#f8ded4', '#834e66', None, '#a7414d', None],
    [None, '#b34858', None, None, '#bcbab4', '#f6f2e8', '#f7f3eb', '#f7f3eb', '#f6f2e8', '#bcbab4', None, None, '#b34858', None],
    [None, '#9b4d62', None, '#969394', '#f8f3ea', '#bbb9b2', '#3e393b', '#3e393b', '#bbb9b2', '#f8f3ea', '#969394', None, '#9b4d62', None],
]


def render_pixel_avatar() -> Text:
    art = Text()
    for row in PIXEL_COLORS:
        for cell in row:
            if cell is None:
                art.append("  ")
            else:
                art.append("  ", style=f"on {cell}")
        art.append("\n")
    return art


def render_status_bar(items: list[tuple[str, str]]) -> Panel:
    content = Text()
    for index, (label, value) in enumerate(items):
        if index:
            content.append("  //  ", style="dim")
        content.append(f"{label} ", style="dim")
        content.append(value, style="bold")
    return Panel(
        Align.center(content),
        border_style="border_soft",
        box=box.ROUNDED,
        padding=(0, 1),
    )


def load_env_credentials() -> tuple[str, str]:
    env_file = os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "apps", "backend", ".env"
    )
    username = ""
    password = ""
    if os.path.exists(env_file):
        with open(env_file, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#") or "=" not in line:
                    continue
                key, _, value = line.partition("=")
                key = key.strip()
                value = value.strip()
                if key == "MAKIMA_CLI_USERNAME":
                    username = value
                elif key == "MAKIMA_CLI_PASSWORD":
                    password = value
    return username, password


def _load_env_value(name: str) -> str:
    """Load a value from environment first, then apps/backend/.env."""
    value = os.getenv(name, "")
    if value:
        return value

    env_file = os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "apps", "backend", ".env"
    )
    if os.path.exists(env_file):
        with open(env_file, encoding="utf-8", errors="ignore") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#") or "=" not in line:
                    continue
                k, _, v = line.partition("=")
                if k.strip() == name:
                    return v.strip()
    return ""


def _load_fish_audio_config() -> tuple[str, str]:
    """Load Fish Audio API key and voice reference ID."""
    return (
        _load_env_value("MAKIMA_FISH_AUDIO_KEY"),
        _load_env_value("MAKIMA_FISH_AUDIO_REFERENCE_ID"),
    )


def _update_env_value(name: str, value: str) -> bool:
    """Update or append a variable in apps/backend/.env."""
    env_file = os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "apps", "backend", ".env"
    )
    lines: list[str] = []
    found = False

    if os.path.exists(env_file):
        with open(env_file, encoding="utf-8", errors="ignore") as f:
            lines = f.readlines()

    for idx, line in enumerate(lines):
        if line.startswith(f"{name}="):
            lines[idx] = f"{name}={value}\n"
            found = True
            break

    if not found:
        if lines and not lines[-1].endswith("\n"):
            lines[-1] += "\n"
        lines.append(f"{name}={value}\n")

    with open(env_file, "w", encoding="utf-8", errors="ignore") as f:
        f.writelines(lines)

    os.environ[name] = value
    return True


def _speak_text(text: str) -> None:
    """Use Fish Audio API or edge-tts to speak text aloud. Runs synchronously."""
    tmp_path = os.path.join(tempfile.gettempdir(), "makima_tts.mp3")
    success = False

    # ── Try Fish Audio API first (requires MAKIMA_FISH_AUDIO_KEY) ──
    try:
        import httpx
        import pygame

        api_key, reference_id = _load_fish_audio_config()
        api_url = "https://api.fish.audio/v1/tts"

        if api_key and reference_id:
            with httpx.Client(timeout=30.0) as client:
                resp = client.post(
                    api_url,
                    headers={
                        "Authorization": f"Bearer {api_key}",
                        "Content-Type": "application/json",
                    },
                    json={
                        "text": text,
                        "reference_id": reference_id,
                    },
                )
                if resp.status_code == 200:
                    with open(tmp_path, "wb") as f:
                        f.write(resp.content)

                    if os.path.exists(tmp_path):
                        if not pygame.mixer.get_init():
                            pygame.mixer.init(frequency=24000)
                        pygame.mixer.music.load(tmp_path)
                        pygame.mixer.music.play()
                        while pygame.mixer.music.get_busy():
                            time.sleep(0.1)
                        pygame.mixer.music.unload()
                        success = True
                else:
                    console.print(
                        f"  [dim]Fish Audio TTS failed (HTTP {resp.status_code}): "
                        f"{resp.text[:200]}. Falling back to Edge TTS.[/dim]"
                    )
        elif api_key and not reference_id:
            console.print(
                "  [dim]Fish Audio skipped: MAKIMA_FISH_AUDIO_REFERENCE_ID is not set. "
                "Falling back to Edge TTS.[/dim]"
            )
    except Exception as exc:
        console.print(
            f"  [dim]Fish Audio TTS exception: {exc}. Falling back to Edge TTS.[/dim]"
        )

    # ── Fallback to edge-tts (free, no API key needed) ──
    if not success:
        try:
            import edge_tts
            import pygame

            voice = "zh-CN-XiaoxiaoNeural"
            rate = "-5%"
            pitch = "-2Hz"

            async def _generate():
                communicate = edge_tts.Communicate(
                    text=text, voice=voice, rate=rate, pitch=pitch
                )
                await communicate.save(tmp_path)

            asyncio.run(_generate())

            if os.path.exists(tmp_path):
                if not pygame.mixer.get_init():
                    pygame.mixer.init(frequency=24000)
                pygame.mixer.music.load(tmp_path)
                pygame.mixer.music.play()
                while pygame.mixer.music.get_busy():
                    time.sleep(0.1)
                pygame.mixer.music.unload()
        except Exception:
            pass

    # Cleanup temp file
    try:
        if os.path.exists(tmp_path):
            os.remove(tmp_path)
    except OSError:
        pass


class MakimaCLI:
    def __init__(self, server_url: str = DEFAULT_SERVER):
        self.server_url = server_url.rstrip("/")
        self.client = httpx.Client(timeout=120.0, trust_env=False)
        self.token: Optional[str] = None
        self.user_id: Optional[str] = None
        self.session_id: Optional[str] = None
        self.session_title = "CLI Chat"
        self.history = InMemoryHistory()
        self.prompt_session: Optional[PromptSession] = None
        self._title_generated = False
        self._tts_enabled = True
        self._tts_thread: Optional[threading.Thread] = None

    def print_banner(self) -> None:
        eyebrow = Text("MAKIMA CONTROL CONSOLE", style=f"bold {COLORS['rose']}")
        title = Text("Personal Devil of the Terminal", style=f"bold {COLORS['ivory']}")
        subtitle = Text(
            "Precision. Memory. Obedience.",
            style="accent",
        )
        notes = Text()
        notes.append("Private assistant for chat, code, tools, and context.", style="dim")
        tips = Text()
        tips.append("/help", style=f"bold {COLORS['gold']}")
        tips.append(" for commands", style="dim")
        tips.append("   ", style="dim")
        tips.append("/exit", style=f"bold {COLORS['gold']}")
        tips.append(" to disengage", style="dim")

        content = Table.grid(expand=True, padding=(0, 2))
        content.add_column(width=32)
        content.add_column(ratio=1)
        content.add_row(
            render_pixel_avatar(),
            Group(
                Text(""),
                eyebrow,
                Text(""),
                title,
                subtitle,
                Text(""),
                notes,
                Text(""),
                tips,
            ),
        )

        console.print(
            Panel(
                content,
                border_style="border",
                box=box.DOUBLE,
                title="[title]Makima[/title]",
                subtitle="[dim]control node[/dim]",
                padding=(1, 2),
            )
        )
        console.print(
            render_status_bar(
                [
                    ("mode", "dominion"),
                    ("transport", "local api"),
                    ("persona", "makima"),
                ]
            )
        )
        console.print()

    def print_status(self, status: str, detail: str = "") -> None:
        text = Text("  ")
        text.append(status, style="success")
        if detail:
            text.append(f"  {detail}", style="dim")
        console.print(text)

    def print_error(self, message: str) -> None:
        console.print(f"  [error]ERROR[/error] {message}")

    def print_divider(self) -> None:
        console.rule("[dim]red channel open[/dim]", style=COLORS["border_soft"])

    def print_session_header(self, username: str) -> None:
        info = Table.grid(expand=True)
        info.add_column(ratio=1)
        info.add_column(ratio=1)
        info.add_column(ratio=1)
        info.add_row(
            f"[dim]user[/dim] [bold]{username}[/bold]",
            f"[dim]session[/dim] [bold]{self.session_title}[/bold]",
            f"[dim]server[/dim] [bold]{self.server_url}[/bold]",
        )
        console.print(
            Panel(
                info,
                border_style="border_soft",
                box=box.ROUNDED,
                title="[title]Console Ready[/title]",
                padding=(0, 1),
            )
        )

    def print_session_details(self, username: str) -> None:
        details = Table.grid(expand=True, padding=(0, 1))
        details.add_column(ratio=1)
        details.add_column(ratio=1)
        details.add_column(ratio=1)
        details.add_row(
            f"[dim]user[/dim] [bold]{username}[/bold]",
            f"[dim]session[/dim] [bold]{self.session_title}[/bold]",
            f"[dim]id[/dim] [bold]{self.session_id}[/bold]",
        )
        details.add_row(
            f"[dim]server[/dim] [bold]{self.server_url}[/bold]",
            f"[dim]title lock[/dim] [bold]{'armed' if self._title_generated else 'idle'}[/bold]",
            f"[dim]status[/dim] [bold]{'online' if self.token else 'offline'}[/bold]",
        )
        console.print(
            Panel(
                details,
                title="[title]Session Telemetry[/title]",
                subtitle="[dim]live console state[/dim]",
                border_style="border",
                box=box.DOUBLE,
                padding=(1, 2),
            )
        )

    def list_fish_voices(self, limit: int = 8) -> bool:
        api_key, current_reference_id = _load_fish_audio_config()
        if not api_key:
            self.print_error("MAKIMA_FISH_AUDIO_KEY is not configured")
            return False

        try:
            response = httpx.get(
                "https://api.fish.audio/model",
                params={"type": "tts", "page_size": limit},
                headers={"Authorization": f"Bearer {api_key}"},
                timeout=30.0,
            )
        except Exception as exc:
            self.print_error(f"Failed to load Fish voices: {exc}")
            return False

        if response.status_code != 200:
            self.print_error(f"Fish voices request failed: HTTP {response.status_code}")
            return False

        items = response.json().get("items", [])
        if not items:
            console.print("  [dim]No Fish voices returned.[/dim]")
            return False

        table = Table(
            show_header=True,
            header_style=f"bold {COLORS['gold']}",
            border_style="border_soft",
            box=box.SIMPLE_HEAVY,
            padding=(0, 1),
        )
        table.add_column("Active", width=8)
        table.add_column("Voice ID", style="bold")
        table.add_column("Title")
        table.add_column("Tags", style="dim")

        for item in items:
            voice_id = str(item.get("_id", ""))
            tags = ", ".join(item.get("tags", [])[:4])
            active = "CURRENT" if voice_id == current_reference_id else ""
            table.add_row(active, voice_id, str(item.get("title", "")), tags)

        console.print()
        console.print(
            Panel(
                Group(
                    table,
                    Text(""),
                    Text(
                        "Use /fishvoice <voice_id> to set the active Fish voice.",
                        style="dim",
                    ),
                ),
                title="[title]Fish Voices[/title]",
                border_style="border_soft",
                box=box.ROUNDED,
                padding=(0, 1),
            )
        )
        console.print()
        return True

    def set_fish_voice(self, voice_id: str) -> bool:
        voice_id = voice_id.strip()
        if not voice_id:
            self.print_error("Usage: /fishvoice <voice_id>")
            return False

        try:
            _update_env_value("MAKIMA_FISH_AUDIO_REFERENCE_ID", voice_id)
        except Exception as exc:
            self.print_error(f"Failed to update .env: {exc}")
            return False

        console.print(f"  [dim]Fish voice set to:[/dim] [bold]{voice_id}[/bold]")
        console.print()
        return True

    def show_fish_voice(self) -> None:
        _, reference_id = _load_fish_audio_config()
        if reference_id:
            console.print(f"  [dim]Current Fish voice:[/dim] [bold]{reference_id}[/bold]")
        else:
            console.print("  [dim]Current Fish voice: not configured[/dim]")
        console.print()

    def login(self, username: str, password: str) -> bool:
        try:
            resp = self.client.post(
                f"{self.server_url}/auth/login",
                json={"username": username, "password": password},
            )
        except Exception as exc:
            self.print_error(f"Unable to connect to server: {exc}")
            return False

        if resp.status_code == 200:
            data = resp.json()
            self.token = data["access_token"]
            self.user_id = data["user_id"]
            self.print_status("OK", f"Logged in as [bold]{username}[/bold]")
            return True

        try:
            resp = self.client.post(
                f"{self.server_url}/auth/register",
                json={
                    "username": username,
                    "email": f"{username}@local",
                    "password": password,
                },
            )
        except Exception as exc:
            self.print_error(f"Registration failed: {exc}")
            return False

        if resp.status_code in (200, 201):
            data = resp.json()
            self.token = data["access_token"]
            self.user_id = data["user_id"]
            self.print_status("OK", f"Registered and logged in as [bold]{username}[/bold]")
            return True

        self.print_error(f"Authentication failed: {resp.text}")
        return False

    def create_session(self, title: str = "CLI Chat") -> bool:
        headers = {"Authorization": f"Bearer {self.token}"}
        try:
            resp = self.client.post(
                f"{self.server_url}/sessions",
                json={"title": title},
                headers=headers,
            )
        except Exception as exc:
            self.print_error(f"Create session failed: {exc}")
            return False

        if resp.status_code in (200, 201):
            data = resp.json()
            self.session_id = data["id"]
            self.session_title = title
            self.print_status(
                "OK",
                f"Session [bold]{title}[/bold]  [dim]({self.session_id[:8]}...)[/dim]",
            )
            return True

        self.print_error(f"Create session failed: {resp.text}")
        return False

    def update_session_title(self, title: str) -> bool:
        headers = {"Authorization": f"Bearer {self.token}"}
        try:
            resp = self.client.patch(
                f"{self.server_url}/sessions/{self.session_id}",
                json={"title": title},
                headers=headers,
            )
        except Exception:
            return False

        if resp.status_code == 200:
            self.session_title = title
            return True
        return False

    def generate_title(self, user_msg: str, agent_msg: str) -> str:
        prompt = (
            "Based on the following conversation, generate a concise title (5-10 words, "
            "no quotes, no punctuation at the end). Respond with ONLY the title.\n\n"
            f"User: {user_msg[:200]}\n"
            f"Assistant: {agent_msg[:200]}\n\n"
            "Title:"
        )

        try:
            env_file = os.path.join(
                os.path.dirname(os.path.abspath(__file__)), "apps", "backend", ".env"
            )
            api_key = ""
            api_base = "https://api.deepseek.com"
            model = "deepseek-v4-flash"
            if os.path.exists(env_file):
                with open(env_file, encoding="utf-8") as f:
                    for line in f:
                        line = line.strip()
                        if not line or line.startswith("#") or "=" not in line:
                            continue
                        key, _, value = line.partition("=")
                        key = key.strip()
                        value = value.strip()
                        if key == "MAKIMA_LLM_API_KEY":
                            api_key = value
                        elif key == "MAKIMA_LLM_API_BASE":
                            api_base = value
                        elif key == "MAKIMA_LLM_MODEL":
                            model = value

            if not api_key:
                return user_msg[:30]

            resp = self.client.post(
                f"{api_base}/chat/completions",
                json={
                    "model": model,
                    "messages": [{"role": "user", "content": prompt}],
                    "max_tokens": 50,
                    "temperature": 0.5,
                },
                headers={
                    "Authorization": f"Bearer {api_key}",
                    "Content-Type": "application/json",
                },
                timeout=15.0,
            )
            if resp.status_code == 200:
                title = resp.json()["choices"][0]["message"]["content"].strip()
                title = title.strip('"').strip("'").strip()
                if title.endswith("."):
                    title = title[:-1]
                return title[:50]
        except Exception:
            pass

        return user_msg[:30]

    def voice_input(self) -> str:
        """Record audio from microphone and transcribe using Whisper API.
        
        Returns:
            Transcribed text string, or empty string on failure.
        """
        try:
            import speech_recognition as sr
        except ImportError:
            self.print_error("SpeechRecognition not installed. Run: pip install SpeechRecognition")
            return ""
        
        recognizer = sr.Recognizer()
        
        console.print("  [dim]🎙️ Listening... (speak now, silence to stop)[/dim]")
        
        try:
            with sr.Microphone() as source:
                # Adjust for ambient noise
                recognizer.adjust_for_ambient_noise(source, duration=0.5)
                # Listen for speech (timeout after 10 seconds of silence)
                audio = recognizer.listen(source, timeout=10, phrase_time_limit=30)
        except sr.WaitTimeoutError:
            console.print("  [dim]⏱️ No speech detected, timeout.[/dim]")
            return ""
        except Exception as exc:
            self.print_error(f"Microphone error: {exc}")
            return ""
        
        console.print("  [dim]🔄 Transcribing...[/dim]")
        
        # Get audio data as WAV bytes
        wav_data = audio.get_wav_data()
        
        # Read API config from .env
        env_file = os.path.join(
            os.path.dirname(os.path.abspath(__file__)), "apps", "backend", ".env"
        )
        api_key = ""
        api_base = "https://api.openai.com/v1"
        if os.path.exists(env_file):
            with open(env_file, encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#") or "=" not in line:
                        continue
                    key, _, value = line.partition("=")
                    key = key.strip()
                    value = value.strip()
                    if key == "MAKIMA_LLM_API_KEY":
                        api_key = value
                    elif key == "MAKIMA_LLM_API_BASE":
                        api_base = value
        
        if not api_key:
            # Fallback to Google free API
            try:
                text = recognizer.recognize_google(audio, language="zh-CN")
                return text
            except Exception:
                self.print_error("No API key configured and Google STT failed")
                return ""
        
        # Use Whisper API via HTTP
        try:
            whisper_url = api_base.rstrip("/") + "/audio/transcriptions"
            response = self.client.post(
                whisper_url,
                files={"file": ("audio.wav", wav_data, "audio/wav")},
                data={
                    "model": "whisper-1",
                    "language": "zh",
                    "response_format": "json",
                },
                headers={"Authorization": f"Bearer {api_key}"},
                timeout=30.0,
            )
            
            if response.status_code == 200:
                result = response.json()
                text = result.get("text", "")
                if text:
                    console.print(f"  [dim]📝 Heard:[/dim] [bold]{text}[/bold]")
                    return text
                else:
                    console.print("  [dim]⚠️ Could not transcribe audio.[/dim]")
                    return ""
            else:
                # Fallback to Google
                try:
                    text = recognizer.recognize_google(audio, language="zh-CN")
                    console.print(f"  [dim]📝 Heard:[/dim] [bold]{text}[/bold]")
                    return text
                except Exception:
                    self.print_error(f"Whisper API error: HTTP {response.status_code}")
                    return ""
        except Exception as exc:
            # Fallback to Google
            try:
                text = recognizer.recognize_google(audio, language="zh-CN")
                console.print(f"  [dim]📝 Heard:[/dim] [bold]{text}[/bold]")
                return text
            except Exception:
                self.print_error(f"Transcription failed: {exc}")
                return ""

    def send_message(self, message: str) -> str:
        headers = {"Authorization": f"Bearer {self.token}"}
        agent_content = ""
        tool_calls: list[tuple[str, dict[str, object]]] = []
        tool_results: list[tuple[str, str]] = []
        error_msg: str | None = None

        try:
            with Live(
                Panel(
                    Spinner("dots", text="Thinking...", style="thinking"),
                    border_style="border",
                    box=box.ROUNDED,
                    title="[agent]Makima[/agent]",
                    subtitle="[dim]processing[/dim]",
                    padding=(0, 1),
                ),
                console=console,
                refresh_per_second=10,
                transient=True,
            ) as live:
                with self.client.stream(
                    "POST",
                    f"{self.server_url}/tasks",
                    json={"session_id": self.session_id, "input_text": message},
                    headers=headers,
                ) as resp:
                    if resp.status_code != 200:
                        self.print_error(f"Request failed: HTTP {resp.status_code}")
                        return ""

                    event_type = ""
                    for line in resp.iter_lines():
                        if not line:
                            continue

                        if line.startswith("event:"):
                            event_type = line[6:].strip()
                            continue

                        if not line.startswith("data:"):
                            continue

                        try:
                            payload = json.loads(line[5:].strip())
                        except json.JSONDecodeError:
                            continue

                        data = payload.get("data", {})
                        if event_type == "thinking":
                            phase = data.get("phase", "")
                            if phase == "memory_recall":
                                status_text = "Recalling memories..."
                            elif phase == "knowledge_retrieval":
                                status_text = "Searching knowledge base..."
                            else:
                                status_text = "Thinking..."
                            live.update(
                                Panel(
                                    Spinner("dots", text=status_text, style="thinking"),
                                    border_style="border",
                                    box=box.ROUNDED,
                                    title="[agent]Makima[/agent]",
                                    subtitle="[dim]thinking[/dim]",
                                    padding=(0, 1),
                                )
                            )
                        elif event_type == "tool_call":
                            tool_name = str(data.get("tool", "unknown"))
                            tool_input = dict(data.get("input", {}) or {})
                            tool_calls.append((tool_name, tool_input))
                            live.update(
                                Panel(
                                    Group(
                                        Text(f"Calling {tool_name}...", style="tool"),
                                        Text(
                                            f"   Input: {json.dumps(tool_input, ensure_ascii=False)[:80]}",
                                            style="dim",
                                        ),
                                    ),
                                    border_style="tool",
                                    box=box.ROUNDED,
                                    title="[agent]Makima[/agent]",
                                    subtitle="[dim]tool dispatch[/dim]",
                                    padding=(0, 1),
                                )
                            )
                        elif event_type == "tool_result":
                            tool_name = str(data.get("tool", "unknown"))
                            output = str(data.get("output", ""))
                            tool_results.append((tool_name, output))
                            live.update(
                                Panel(
                                    Group(
                                        Text(f"{tool_name} returned:", style="tool"),
                                        Text(f"   {output[:120]}", style="dim"),
                                    ),
                                    border_style="tool",
                                    box=box.ROUNDED,
                                    title="[agent]Makima[/agent]",
                                    subtitle="[dim]tool result[/dim]",
                                    padding=(0, 1),
                                )
                            )
                        elif event_type == "message":
                            content = str(data.get("content", ""))
                            if content:
                                agent_content += content
                        elif event_type == "error":
                            error_msg = str(data.get("error", "Unknown error"))
        except httpx.TimeoutException:
            self.print_error("Request timed out")
            return ""
        except Exception as exc:
            self.print_error(f"Communication error: {exc}")
            return ""

        console.print()
        panel_parts: list[object] = []

        if tool_calls:
            for tool_name, tool_input in tool_calls:
                line = Text()
                line.append("COMMAND ", style="tool")
                line.append(tool_name, style="bold")
                if tool_input:
                    line.append("  ", style="dim")
                    input_str = json.dumps(tool_input, ensure_ascii=False)
                    if len(input_str) > 100:
                        input_str = input_str[:100] + "..."
                    line.append(f"  {input_str}", style="dim")
                panel_parts.append(line)

        if tool_results:
            for tool_name, output in tool_results:
                line = Text()
                line.append("RESULT ", style="dim")
                line.append(tool_name, style="dim italic")
                panel_parts.append(line)
                if len(output) > 200:
                    output = output[:200] + "..."
                panel_parts.append(Text(f"    {output}", style="dim"))

        if panel_parts:
            panel_parts.append(Text(""))

        if error_msg:
            panel_parts.append(Text(f"ERROR: {error_msg}", style="error"))

        if agent_content:
            panel_parts.append(Markdown(agent_content))

        if not panel_parts:
            panel_parts.append(Text("(empty response)", style="dim italic"))

        console.print(
            Panel(
                Group(*panel_parts),
                border_style="border",
                box=box.ROUNDED,
                title="[agent]Makima[/agent]",
                subtitle="[dim]response[/dim]",
                padding=(1, 2),
            )
        )
        console.print()

        # TTS: speak the response in a background thread
        if self._tts_enabled and agent_content:
            # Strip markdown/code blocks for cleaner speech
            import re
            speak_text = re.sub(r"```[\s\S]*?```", "", agent_content)
            speak_text = re.sub(r"`[^`]+`", "", speak_text)
            speak_text = re.sub(r"[#*_\[\]()>|~-]", "", speak_text)
            speak_text = speak_text.strip()
            if speak_text:
                self._tts_thread = threading.Thread(
                    target=_speak_text, args=(speak_text,), daemon=True
                )
                self._tts_thread.start()

        return agent_content

    def print_help(self) -> None:
        table = Table(
            show_header=True,
            header_style=f"bold {COLORS['gold']}",
            border_style="border_soft",
            box=box.SIMPLE_HEAVY,
            padding=(0, 2),
        )
        table.add_column("Command", style="bold", width=20)
        table.add_column("Description", style="dim")
        tts_status = "[bold green]ON[/bold green]" if self._tts_enabled else "[bold red]OFF[/bold red]"
        for cmd, desc in [
            ("/help", "Show this help message"),
            ("/clear", "Clear the screen"),
            ("/session", "Show current session info"),
            ("/speak", "🎙️ Speak to Makima (voice input)"),
            (f"/voice  ({tts_status})", "Toggle voice output (TTS)"),
            ("/fishvoices", "List available Fish Audio voices"),
            ("/fishvoice [id]", "Show or set the active Fish voice"),
            ("/exit, /quit", "Exit the CLI"),
        ]:
            table.add_row(cmd, desc)

        console.print()
        console.print(
            Panel(
                Group(
                    table,
                    Text(""),
                    Text("Tip: use /session to inspect the active chat context.", style="dim"),
                ),
                title="[title]Command Index[/title]",
                border_style="border_soft",
                box=box.ROUNDED,
                padding=(0, 1),
            )
        )
        console.print()

    def run(self) -> None:
        console.clear()
        self.print_banner()

        env_user, env_pass = load_env_credentials()
        console.print()
        if env_user and env_pass:
            username = env_user
            password = env_pass
            console.print("  [dim]Using credentials from .env[/dim]")
        else:
            try:
                username = pt_prompt(HTML("  <b>Username:</b> ")).strip()
                password = pt_prompt(HTML("  <b>Password:</b> "), is_password=True).strip()
                if not username or not password:
                    console.print("\n  [error]Username and password are required.[/error]\n")
                    sys.exit(1)
            except (KeyboardInterrupt, EOFError):
                console.print("\n  [dim]Aborted.[/dim]")
                sys.exit(0)

        console.print()
        if not self.login(username, password):
            sys.exit(1)
        if not self.create_session("New Chat"):
            sys.exit(1)

        self.print_session_header(username)
        self.print_divider()
        console.print()

        self.prompt_session = PromptSession(
            history=self.history,
            style=PROMPT_STYLE,
            multiline=False,
            enable_history_search=True,
        )

        while True:
            try:
                message = self.prompt_session.prompt(
                    HTML(
                        "  <label>makima</label> <meta>binds</meta> "
                        f"<prompt>{username}</prompt> <arrow>&gt;</arrow> "
                    )
                ).strip()
                if not message:
                    continue

                if message in ("/exit", "/quit", "/q"):
                    console.print("\n  [dim]Goodbye.[/dim]\n")
                    break
                if message == "/help":
                    self.print_help()
                    continue
                if message == "/clear":
                    console.clear()
                    self.print_banner()
                    self.print_session_header(username)
                    self.print_divider()
                    console.print()
                    continue
                if message == "/session":
                    console.print()
                    self.print_session_details(username)
                    console.print()
                    continue
                if message == "/voice":
                    self._tts_enabled = not self._tts_enabled
                    status = "[bold green]ON[/bold green]" if self._tts_enabled else "[bold red]OFF[/bold red]"
                    console.print(f"  [dim]Voice output (TTS):[/dim] {status}")
                    console.print()
                    continue
                if message == "/fishvoices":
                    self.list_fish_voices()
                    continue
                if message == "/fishvoice":
                    self.show_fish_voice()
                    continue
                if message.startswith("/fishvoice "):
                    self.set_fish_voice(message.partition(" ")[2])
                    continue
                if message == "/speak":
                    # Voice input mode
                    voice_text = self.voice_input()
                    if voice_text:
                        # Wait for previous TTS to finish
                        if self._tts_thread and self._tts_thread.is_alive():
                            self._tts_thread.join(timeout=30)
                        # Send the transcribed text as a message
                        agent_reply = self.send_message(voice_text)
                        if not self._title_generated and agent_reply:
                            self._title_generated = True
                            new_title = self.generate_title(voice_text, agent_reply)
                            if self.update_session_title(new_title):
                                console.print(f"  [dim]Title updated: {new_title}[/dim]")
                                console.print()
                    continue

                # Wait for previous TTS to finish before sending new message
                if self._tts_thread and self._tts_thread.is_alive():
                    self._tts_thread.join(timeout=30)

                agent_reply = self.send_message(message)
                if not self._title_generated and agent_reply:
                    self._title_generated = True
                    new_title = self.generate_title(message, agent_reply)
                    if self.update_session_title(new_title):
                        console.print(f"  [dim]Title updated: {new_title}[/dim]")
                        console.print()
            except KeyboardInterrupt:
                console.print("\n  [dim]Use /exit or Ctrl+C again to quit.[/dim]")
                try:
                    self.prompt_session.prompt(HTML("  <b>></b> "), default="")
                except KeyboardInterrupt:
                    console.print("\n  [dim]Goodbye.[/dim]\n")
                    break
            except EOFError:
                console.print("\n  [dim]Goodbye.[/dim]\n")
                break


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Makima Agent CLI - AI-powered coding assistant",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--server",
        default=DEFAULT_SERVER,
        help=f"Server URL (default: {DEFAULT_SERVER})",
    )
    args = parser.parse_args()
    MakimaCLI(args.server).run()


if __name__ == "__main__":
    main()
