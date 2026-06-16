#!/usr/bin/env python3
"""Makima Agent Launcher — starts server + chat CLI in one window."""

import os
import sys
import signal
import subprocess
import time
import urllib.request

# ── Paths ────────────────────────────────────────────────────────────────
BASE_DIR = os.path.dirname(os.path.abspath(__file__))
BACKEND_DIR = os.path.join(BASE_DIR, "apps", "backend")
CLI_SCRIPT = os.path.join(BASE_DIR, "cli.py")
SERVER_URL = "http://localhost:8000"


def clear_screen():
    os.system("cls" if os.name == "nt" else "clear")


def print_banner():
    lines = [
        ("  ███╗   ███╗ █████╗ ██╗  ██╗██╗███╗   ███╗ █████╗ ", "cyan"),
        ("  ████╗ ████║██╔══██╗██║ ██╔╝██║████╗ ████║██╔══██╗", "blue"),
        ("  ██╔████╔██║███████║█████╔╝ ██║██╔████╔██║███████║", "magenta"),
        ("  ██║╚██╔╝██║██╔══██║██╔═██╗ ██║██╔████╔██║██╔══██║", "blue"),
        ("  ██║ ╚═╝ ██║██║  ██║██║  ██╗██║██║ ╚═╝ ██║██║  ██║", "cyan"),
        ("  ╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝╚═╝     ╚═╝╚═╝  ╚═╝", "magenta"),
    ]
    try:
        from rich.console import Console
        from rich.text import Text
        from rich.panel import Panel
        from rich.group import Group

        console = Console()
        title = Text()
        for line, color in lines:
            title.append(line + "\n", style=f"bold {color}")

        content = Group(
            title,
            Text("     AI-Powered Coding Assistant", style="dim italic"),
            Text(""),
            Text("  One-click launcher: server + chat in a single window.", style="dim"),
        )
        console.print(Panel(content, border_style="bright_blue", padding=(1, 2)))
    except ImportError:
        for line, _ in lines:
            print(line)
        print("     AI-Powered Coding Assistant")
        print()


def check_dependencies():
    """Install missing dependencies."""
    deps = ["rich", "prompt_toolkit", "httpx"]
    missing = []
    for dep in deps:
        try:
            __import__(dep)
        except ImportError:
            missing.append(dep)

    if missing:
        print(f"  Installing: {', '.join(missing)}...")
        subprocess.run(
            [sys.executable, "-m", "pip", "install", *missing, "-q"],
            capture_output=True,
        )


def wait_for_server(timeout=30):
    """Wait for the server health check to pass."""
    start = time.time()
    while time.time() - start < timeout:
        try:
            urllib.request.urlopen(f"{SERVER_URL}/health", timeout=2)
            return True
        except Exception:
            time.sleep(1)
    return False


def main():
    clear_screen()
    print_banner()
    print()

    # ── Check dependencies ───────────────────────────────────────────
    print("  Checking dependencies...")
    check_dependencies()

    # ── Start server as background subprocess ────────────────────────
    print("  Starting server...")

    # Use CREATE_NEW_PROCESS_GROUP on Windows so Ctrl+C doesn't kill server
    kwargs = {}
    if os.name == "nt":
        kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP

    server_proc = subprocess.Popen(
        [
            sys.executable, "-m", "uvicorn",
            "makima.app:app",
            "--host", "0.0.0.0",
            "--port", "8000",
            "--reload",
        ],
        cwd=BACKEND_DIR,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        **kwargs,
    )

    # ── Wait for server ready ────────────────────────────────────────
    print("  Waiting for server to be ready...", end="", flush=True)
    if not wait_for_server(timeout=30):
        print("\n  [ERROR] Server failed to start within 30 seconds")
        print("  Check apps/backend/.env for configuration")
        server_proc.terminate()
        input("\n  Press Enter to exit...")
        sys.exit(1)

    print(f"\r  \x1b[32m✓\x1b[0m Server ready on {SERVER_URL}                    ")
    print()
    print("  ─" * 30)
    print()

    # ── Run chat CLI ─────────────────────────────────────────────────
    exit_code = 0
    try:
        result = subprocess.run(
            [sys.executable, CLI_SCRIPT, "--server", SERVER_URL],
            cwd=BASE_DIR,
        )
        exit_code = result.returncode
    except KeyboardInterrupt:
        pass

    # ── Cleanup ──────────────────────────────────────────────────────
    print()
    print("  Stopping server...")
    try:
        server_proc.terminate()
        server_proc.wait(timeout=5)
    except Exception:
        server_proc.kill()

    # Also kill any leftover processes on port 8000
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/F", "/IM", "uvicorn.exe"],
            capture_output=True,
        )

    print("  \x1b[32m✓\x1b[0m Server stopped")
    print()


if __name__ == "__main__":
    main()