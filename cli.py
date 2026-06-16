#!/usr/bin/env python3
"""Makima Agent CLI."""

from __future__ import annotations

import argparse
import json
import os
import sys
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

DEFAULT_SERVER = "http://localhost:8000"

COLORS = {
    "border": "#b24a56",
    "border_soft": "#6d2b35",
    "rose": "#f1a0a7",
    "crimson": "#da6570",
    "gold": "#d7b46a",
    "amber": "#e8c27a",
    "ivory": "#f6f0e8",
    "ink": "#101018",
    "muted": "#8c7f83",
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
    }
)

# Exact 12x12 pixels sampled from t.png. Near-black background is treated as transparent.
PIXEL_COLORS = [
    [None, None, None, None, None, None, None, None, None, None, None, None],
    [None, None, None, "#a43249", "#c75265", "#c95268", "#cb5166", "#cc5165", "#9d3b4a", None, None, None],
    [None, None, "#824250", "#ce5065", "#cf5166", "#cc5364", "#cd5362", "#cd5362", "#cd5362", "#a73946", None, None],
    [None, "#952f44", "#e2727e", "#da7682", "#e37786", "#cd5266", "#cf5264", "#ce5163", "#e07682", "#dd7e86", "#9a343f", None],
    [None, "#c75167", "#d15466", "#ce5568", "#cc5163", "#a43142", "#d15466", "#aa3545", "#d05166", "#cc5366", "#cc5364", "#933c4c"],
    [None, "#ce526a", "#c2495c", "#c44d60", "#cd5868", "#a33947", "#ae384e", "#9c3f47", "#c85366", "#a43b49", "#ce5065", "#9e3947"],
    ["#a3384a", "#a43845", "#c2495c", "#912f3c", "#973244", "#fcddd8", "#a73d51", "#a23344", "#8a2d38", "#a33a48", "#be4558", "#ce5368"],
    ["#a8374b", "#842e3b", "#bc4c5a", "#fde1d5", "#f6e3d2", "#f5e7da", "#9d4754", "#fbe8d9", "#fee7d9", "#9e3845", "#a93f4d", "#cd4f65"],
    ["#943847", "#8c3140", "#b55058", "#fdfaf5", "#ffefcb", "#f5e6e1", "#faebe4", "#fef9f6", "#ad7a29", "#6e3a3c", "#d99c9b", "#c94e62"],
    [None, "#872f3d", "#bb4f5c", "#f4ede5", "#f8edd7", "#f8e9e2", "#f5e6df", "#f3e8e2", "#ffebca", "#83313f", "#ae4853", None],
    [None, "#882a3a", "#b35b67", "#f9ece6", "#fdebdf", "#f9eddf", "#f7e5e1", "#f9eae5", "#f7eee7", None, "#712b36", None],
    [None, None, "#8b3a4b", None, None, None, None, None, None, None, "#5e192b", None],
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


class MakimaCLI:
    def __init__(self, server_url: str = DEFAULT_SERVER):
        self.server_url = server_url.rstrip("/")
        self.client = httpx.Client(timeout=120.0)
        self.token: Optional[str] = None
        self.user_id: Optional[str] = None
        self.session_id: Optional[str] = None
        self.session_title = "CLI Chat"
        self.history = InMemoryHistory()
        self.prompt_session: Optional[PromptSession] = None
        self._title_generated = False

    def print_banner(self) -> None:
        title = Text()
        title.append("Makima", style=f"bold {COLORS['ivory']}")
        title.append("  ", style="dim")
        title.append("Personal Agent", style=f"bold {COLORS['gold']}")

        content = Group(
            Align.center(render_pixel_avatar()),
            Text(""),
            Align.center(title),
            Align.center(Text("Precision. Memory. Control.", style="accent")),
            Text(""),
            Text("  Private assistant for chat, code, tools, and context.", style="dim"),
            Text("  Type /help for commands.", style="dim"),
            Text("  Press Ctrl+C or type /exit to quit.", style="dim"),
        )

        console.print(
            Panel(
                content,
                title="[title]Makima[/title]",
                subtitle="[dim]personal assistant cockpit[/dim]",
                border_style="border",
                box=box.DOUBLE,
                padding=(1, 2),
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
        console.print(Panel.fit("", border_style="border_soft", box=box.SQUARE))

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
                    title_align="left",
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
                                    title_align="left",
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
                                    title_align="left",
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
                                    title_align="left",
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
                line.append("• ", style="tool")
                line.append(tool_name, style="bold")
                if tool_input:
                    line.append("  ", style="dim")
                    input_str = json.dumps(tool_input, ensure_ascii=False)
                    if len(input_str) > 100:
                        input_str = input_str[:100] + "..."
                    line.append(input_str, style="dim")
                panel_parts.append(line)

        if tool_results:
            for tool_name, output in tool_results:
                line = Text()
                line.append("• ", style="dim")
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
                title_align="left",
                padding=(1, 2),
            )
        )
        console.print()
        return agent_content

    def print_help(self) -> None:
        table = Table(
            show_header=True,
            header_style=f"bold {COLORS['gold']}",
            border_style="border_soft",
            padding=(0, 2),
        )
        table.add_column("Command", style="bold", width=20)
        table.add_column("Description", style="dim")
        for cmd, desc in [
            ("/help", "Show this help message"),
            ("/clear", "Clear the screen"),
            ("/session", "Show current session info"),
            ("/exit, /quit", "Exit the CLI"),
        ]:
            table.add_row(cmd, desc)

        console.print()
        console.print(
            Panel(
                table,
                title="[title]Commands[/title]",
                border_style="border_soft",
                box=box.SQUARE,
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
                username = pt_prompt(HTML("  <b>Username:</b> ")).strip() or "cli_user"
                password = (
                    pt_prompt(HTML("  <b>Password:</b> "), is_password=True).strip()
                    or "cli_pass"
                )
            except (KeyboardInterrupt, EOFError):
                console.print("\n  [dim]Aborted.[/dim]")
                sys.exit(0)

        console.print()
        if not self.login(username, password):
            sys.exit(1)
        if not self.create_session("New Chat"):
            sys.exit(1)

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
                message = self.prompt_session.prompt(HTML("  <b>></b> ")).strip()
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
                    self.print_divider()
                    console.print()
                    continue
                if message == "/session":
                    console.print()
                    console.print(f"  [bold]User:[/bold]    {username}")
                    console.print(f"  [bold]Session:[/bold] {self.session_title}")
                    console.print(f"  [bold]ID:[/bold]      {self.session_id}")
                    console.print()
                    continue

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
