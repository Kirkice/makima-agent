#!/usr/bin/env python3
"""Makima Agent CLI — Beautiful terminal interface.

A rich-powered CLI client with Markdown rendering, syntax highlighting,
spinner animations, and prompt_toolkit-powered input with history.
"""

from __future__ import annotations

import json
import os
import sys
import argparse
from typing import Optional

import httpx
from rich.console import Console, Group
from rich.markdown import Markdown
from rich.panel import Panel
from rich.spinner import Spinner
from rich.syntax import Syntax
from rich.table import Table
from rich.text import Text
from rich.theme import Theme
from rich.live import Live
from rich.layout import Layout
from rich.align import Align
from rich.rule import Rule
from prompt_toolkit import PromptSession
from prompt_toolkit.history import InMemoryHistory
from prompt_toolkit.formatted_text import HTML
from prompt_toolkit.styles import Style

# ── Theme ────────────────────────────────────────────────────────────────

MAKIMA_THEME = Theme({
    "info": "cyan",
    "success": "green",
    "warning": "yellow",
    "error": "red bold",
    "dim": "dim",
    "user": "bold magenta",
    "agent": "bold cyan",
    "tool": "bold yellow",
    "thinking": "dim italic",
    "border": "bright_blue",
})

console = Console(theme=MAKIMA_THEME, soft_wrap=True)

# ── Prompt style ─────────────────────────────────────────────────────────

PROMPT_STYLE = Style.from_dict({
    'prompt': 'bold cyan',
    'arrow': 'bold blue',
})

# ── Default server ───────────────────────────────────────────────────────

DEFAULT_SERVER = "http://localhost:8000"


def load_env_credentials() -> tuple[str, str]:
    """Load CLI credentials from .env file (apps/backend/.env)."""
    env_file = os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "apps", "backend", ".env"
    )
    username = ""
    password = ""
    if os.path.exists(env_file):
        with open(env_file, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line.startswith("#") or "=" not in line:
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
        self.server_url = server_url.rstrip('/')
        self.client = httpx.Client(timeout=120.0)
        self.token: Optional[str] = None
        self.user_id: Optional[str] = None
        self.session_id: Optional[str] = None
        self.session_title: str = "CLI Chat"
        self.history = InMemoryHistory()
        self.prompt_session: Optional[PromptSession] = None

    # ── Banner ───────────────────────────────────────────────────────

    def print_banner(self):
        """Print the welcome banner."""
        # ASCII art title
        title_text = Text()
        title_text.append("  ███╗   ███╗ █████╗ ██╗  ██╗██╗███╗   ███╗ █████╗ \n", style="bold cyan")
        title_text.append("  ████╗ ████║██╔══██╗██║ ██╔╝██║████╗ ████║██╔══██╗\n", style="bold blue")
        title_text.append("  ██╔████╔██║███████║█████╔╝ ██║██╔████╔██║███████║\n", style="bold magenta")
        title_text.append("  ██║╚██╔╝██║██╔══██║██╔═██╗ ██║██║╚██╔╝██║██╔══██║\n", style="bold blue")
        title_text.append("  ██║ ╚═╝ ██║██║  ██║██║  ██╗██║██║ ╚═╝ ██║██║  ██║\n", style="bold cyan")
        title_text.append("  ╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝╚═╝     ╚═╝╚═╝  ╚═╝\n", style="bold magenta")

        subtitle = Text("     AI-Powered Coding Assistant", style="dim italic")

        content = Group(
            title_text,
            subtitle,
            Text(""),
            Text("  Type your message to start chatting. Use /help for commands.", style="dim"),
            Text("  Press Ctrl+C or type /exit to quit.", style="dim"),
        )

        panel = Panel(
            content,
            border_style="bright_blue",
            padding=(1, 2),
        )
        console.print(panel)
        console.print()

    def print_status(self, status: str, detail: str = ""):
        """Print a status line."""
        text = Text("  ")
        text.append(status, style="success")
        if detail:
            text.append(f"  {detail}", style="dim")
        console.print(text)

    def print_error(self, message: str):
        """Print an error message."""
        console.print(f"  [error]✗ {message}[/error]")

    def print_divider(self):
        """Print a thin divider."""
        console.print(Rule(style="dim"))

    # ── Auth ─────────────────────────────────────────────────────────

    def login(self, username: str, password: str) -> bool:
        """Login or register."""
        # Try login first
        try:
            resp = self.client.post(
                f"{self.server_url}/auth/login",
                json={"username": username, "password": password}
            )
        except Exception as e:
            self.print_error(f"无法连接服务器: {e}")
            return False

        if resp.status_code == 200:
            data = resp.json()
            self.token = data["access_token"]
            self.user_id = data["user_id"]
            self.print_status("✓", f"Logged in as [bold]{username}[/bold]")
            return True

        # Try register
        try:
            resp = self.client.post(
                f"{self.server_url}/auth/register",
                json={"username": username, "email": f"{username}@local", "password": password}
            )
        except Exception as e:
            self.print_error(f"注册失败: {e}")
            return False

        if resp.status_code in (200, 201):
            data = resp.json()
            self.token = data["access_token"]
            self.user_id = data["user_id"]
            self.print_status("✓", f"Registered and logged in as [bold]{username}[/bold]")
            return True

        self.print_error(f"认证失败: {resp.text}")
        return False

    def create_session(self, title: str = "CLI Chat") -> bool:
        """Create a chat session."""
        headers = {"Authorization": f"Bearer {self.token}"}
        try:
            resp = self.client.post(
                f"{self.server_url}/sessions",
                json={"title": title},
                headers=headers
            )
        except Exception as e:
            self.print_error(f"创建会话失败: {e}")
            return False

        if resp.status_code in (200, 201):
            data = resp.json()
            self.session_id = data["id"]
            self.session_title = title
            self.print_status("✓", f"Session [bold]{title}[/bold]  [dim]({self.session_id[:8]}...)[/dim]")
            return True

        self.print_error(f"创建会话失败: {resp.text}")
        return False

    def update_session_title(self, title: str) -> bool:
        """Update the session title via PATCH API."""
        headers = {"Authorization": f"Bearer {self.token}"}
        try:
            resp = self.client.patch(
                f"{self.server_url}/sessions/{self.session_id}",
                json={"title": title},
                headers=headers
            )
            if resp.status_code == 200:
                self.session_title = title
                return True
        except Exception:
            pass
        return False

    def generate_title(self, user_msg: str, agent_msg: str) -> str:
        """Generate a concise title using LLM based on the first exchange."""
        prompt = (
            "Based on the following conversation, generate a concise title (5-10 words, "
            "no quotes, no punctuation at the end). "
            "The title should capture the main topic. "
            "Respond with ONLY the title, nothing else.\n\n"
            f"User: {user_msg[:200]}\n"
            f"Assistant: {agent_msg[:200]}\n\n"
            "Title:"
        )
        try:
            # Read LLM config from .env
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
                        if line.startswith("#") or "=" not in line:
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
                data = resp.json()
                title = data["choices"][0]["message"]["content"].strip()
                # Clean up
                title = title.strip('"').strip("'").strip()
                if title.endswith("."):
                    title = title[:-1]
                return title[:50]
        except Exception:
            pass
        # Fallback: use first 30 chars of user message
        return user_msg[:30]

    # ── SSE Streaming ────────────────────────────────────────────────

    def send_message(self, message: str) -> str:
        """Send a message and stream the response with rich rendering.
        
        Returns the agent's text content for title generation.
        """
        headers = {"Authorization": f"Bearer {self.token}"}

        # Collect response parts
        agent_content = ""
        tool_calls = []
        tool_results = []
        error_msg = None
        is_thinking = True

        try:
            # Show spinner while thinking
            with Live(
                Panel(
                    Group(
                        Spinner("dots", text="Thinking...", style="thinking"),
                    ),
                    border_style="bright_blue",
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
                    json={
                        "session_id": self.session_id,
                        "input_text": message
                    },
                    headers=headers,
                ) as resp:
                    if resp.status_code != 200:
                        self.print_error(f"请求失败: HTTP {resp.status_code}")
                        return

                    event_type = ""
                    for line in resp.iter_lines():
                        if not line:
                            continue

                        if line.startswith("event:"):
                            event_type = line[6:].strip()
                        elif line.startswith("data:"):
                            data_str = line[5:].strip()
                            try:
                                data = json.loads(data_str)
                                event_data = data.get("data", {})

                                if event_type == "thinking":
                                    phase = event_data.get("phase", "")
                                    if phase == "memory_recall":
                                        live.update(Panel(
                                            Spinner("dots", text=f"Recalling memories...", style="thinking"),
                                            border_style="bright_blue",
                                            title="[agent]Makima[/agent]",
                                            title_align="left",
                                            padding=(0, 1),
                                        ))
                                    elif phase == "knowledge_retrieval":
                                        live.update(Panel(
                                            Spinner("dots", text=f"Searching knowledge base...", style="thinking"),
                                            border_style="bright_blue",
                                            title="[agent]Makima[/agent]",
                                            title_align="left",
                                            padding=(0, 1),
                                        ))
                                    else:
                                        live.update(Panel(
                                            Spinner("dots", text="Thinking...", style="thinking"),
                                            border_style="bright_blue",
                                            title="[agent]Makima[/agent]",
                                            title_align="left",
                                            padding=(0, 1),
                                        ))

                                elif event_type == "tool_call":
                                    tool_name = event_data.get("tool", "unknown")
                                    tool_input = event_data.get("input", {})
                                    is_thinking = False
                                    tool_calls.append((tool_name, tool_input))
                                    live.update(Panel(
                                        Group(
                                            Text(f"🔧 Calling {tool_name}...", style="tool"),
                                            Text(f"   Input: {json.dumps(tool_input, ensure_ascii=False)[:80]}", style="dim"),
                                        ),
                                        border_style="yellow",
                                        title="[agent]Makima[/agent]",
                                        title_align="left",
                                        padding=(0, 1),
                                    ))

                                elif event_type == "tool_result":
                                    tool_name = event_data.get("tool", "unknown")
                                    output = event_data.get("output", "")
                                    tool_results.append((tool_name, output))
                                    live.update(Panel(
                                        Group(
                                            Text(f"📋 {tool_name} returned:", style="tool"),
                                            Text(f"   {str(output)[:120]}", style="dim"),
                                        ),
                                        border_style="yellow",
                                        title="[agent]Makima[/agent]",
                                        title_align="left",
                                        padding=(0, 1),
                                    ))

                                elif event_type == "message":
                                    content = event_data.get("content", "")
                                    if content:
                                        agent_content += content

                                elif event_type == "error":
                                    error_msg = event_data.get("error", "Unknown error")

                                elif event_type == "done":
                                    pass

                            except json.JSONDecodeError:
                                pass

        except httpx.TimeoutException:
            self.print_error("请求超时")
            return
        except Exception as e:
            self.print_error(f"通信错误: {e}")
            return

        # ── Render the response ──────────────────────────────────────

        console.print()

        # Build panel content
        panel_parts = []

        # Tool calls section
        if tool_calls:
            for tool_name, tool_input in tool_calls:
                tool_text = Text()
                tool_text.append("🔧 ", style="tool")
                tool_text.append(tool_name, style="bold yellow")
                if tool_input:
                    tool_text.append(f"  ", style="dim")
                    input_str = json.dumps(tool_input, ensure_ascii=False)
                    if len(input_str) > 100:
                        input_str = input_str[:100] + "..."
                    tool_text.append(input_str, style="dim")
                panel_parts.append(tool_text)

        # Tool results section
        if tool_results:
            for tool_name, output in tool_results:
                result_text = Text()
                result_text.append("📋 ", style="dim")
                result_text.append(f"{tool_name}", style="dim italic")
                panel_parts.append(result_text)
                # Show output in a code block if it looks like code
                output_str = str(output)
                if len(output_str) > 200:
                    output_str = output_str[:200] + "..."
                panel_parts.append(Text(f"    {output_str}", style="dim"))

        if panel_parts:
            panel_parts.append(Text(""))

        # Error
        if error_msg:
            panel_parts.append(Text(f"✗ Error: {error_msg}", style="error"))

        # Agent message (rendered as Markdown)
        if agent_content:
            panel_parts.append(Markdown(agent_content))

        if not panel_parts:
            panel_parts.append(Text("(empty response)", style="dim italic"))

        # Create the panel
        panel = Panel(
            Group(*panel_parts),
            border_style="bright_blue",
            title="[agent]Makima[/agent]",
            title_align="left",
            padding=(1, 2),
        )
        console.print(panel)
        console.print()
        return agent_content

    # ── Help ─────────────────────────────────────────────────────────

    def print_help(self):
        """Print help information."""
        table = Table(
            show_header=True,
            header_style="bold cyan",
            border_style="dim",
            padding=(0, 2),
        )
        table.add_column("Command", style="bold", width=20)
        table.add_column("Description", style="dim")

        commands = [
            ("/help", "Show this help message"),
            ("/clear", "Clear the screen"),
            ("/session", "Show current session info"),
            ("/exit, /quit", "Exit the CLI"),
        ]
        for cmd, desc in commands:
            table.add_row(cmd, desc)

        console.print()
        console.print(Panel(table, title="Commands", border_style="dim", padding=(0, 1)))
        console.print()

    # ── Main loop ────────────────────────────────────────────────────

    def run(self):
        """Run the interactive CLI."""
        console.clear()
        self.print_banner()

        # ── Login ────────────────────────────────────────────────────
        from prompt_toolkit import prompt as pt_prompt

        # Try loading credentials from .env
        env_user, env_pass = load_env_credentials()

        console.print()

        if env_user and env_pass:
            # Auto-login with .env credentials
            username = env_user
            password = env_pass
            console.print(f"  [dim]Using credentials from .env[/dim]")
        else:
            # Interactive login
            try:
                username = pt_prompt(HTML("  <b>Username:</b> ")).strip()
                if not username:
                    username = "cli_user"

                password = pt_prompt(HTML("  <b>Password:</b> "), is_password=True).strip()
                if not password:
                    password = "cli_pass"
            except (KeyboardInterrupt, EOFError):
                console.print("\n  [dim]Aborted.[/dim]")
                sys.exit(0)

        console.print()

        if not self.login(username, password):
            sys.exit(1)

        # ── Session ──────────────────────────────────────────────────
        # Create session with default title, will be updated after first message
        default_title = "New Chat"
        if not self.create_session(default_title):
            sys.exit(1)
        
        self._title_generated = False  # Track if title has been generated

        console.print()
        self.print_divider()
        console.print()

        # ── Prompt session ───────────────────────────────────────────
        self.prompt_session = PromptSession(
            history=self.history,
            style=PROMPT_STYLE,
            multiline=False,
            enable_history_search=True,
        )

        # ── Chat loop ────────────────────────────────────────────────
        while True:
            try:
                message = self.prompt_session.prompt(
                    HTML("  <b>❯</b> ")
                ).strip()

                if not message:
                    continue

                # Commands
                if message in ("/exit", "/quit", "/q"):
                    console.print("\n  [dim]Goodbye! 👋[/dim]\n")
                    break
                elif message == "/help":
                    self.print_help()
                    continue
                elif message == "/clear":
                    console.clear()
                    self.print_banner()
                    console.print()
                    self.print_divider()
                    console.print()
                    continue
                elif message == "/session":
                    console.print()
                    console.print(f"  [bold]User:[/bold]    {username}")
                    console.print(f"  [bold]Session:[/bold] {self.session_title}")
                    console.print(f"  [bold]ID:[/bold]      {self.session_id}")
                    console.print()
                    continue

                # Send message
                agent_reply = self.send_message(message)
                
                # Generate title after first message
                if not self._title_generated and agent_reply:
                    self._title_generated = True
                    new_title = self.generate_title(message, agent_reply)
                    if self.update_session_title(new_title):
                        console.print(f'  [dim]Title updated: {new_title}[/dim]')
                        console.print()

            except KeyboardInterrupt:
                console.print("\n  [dim]Use /exit or Ctrl+C again to quit.[/dim]")
                try:
                    # Wait for second Ctrl+C
                    self.prompt_session.prompt(HTML("  <b>❯</b> "), default="")
                except KeyboardInterrupt:
                    console.print("\n  [dim]Goodbye! 👋[/dim]\n")
                    break
            except EOFError:
                console.print("\n  [dim]Goodbye! 👋[/dim]\n")
                break


def main():
    parser = argparse.ArgumentParser(
        description="Makima Agent CLI — AI-powered coding assistant",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--server",
        default=DEFAULT_SERVER,
        help=f"Server URL (default: {DEFAULT_SERVER})",
    )

    args = parser.parse_args()

    cli = MakimaCLI(args.server)
    cli.run()


if __name__ == "__main__":
    main()