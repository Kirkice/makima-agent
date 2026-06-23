#!/usr/bin/env python3
"""Makima Agent launcher.

Starts the backend server and then opens the interactive CLI in one window.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
import urllib.request

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
BACKEND_DIR = os.path.join(BASE_DIR, "apps", "backend")
CLI_SCRIPT = os.path.join(BASE_DIR, "cli.py")
SERVER_URL = "http://127.0.0.1:8000"

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


def clear_screen() -> None:
    os.system("cls" if os.name == "nt" else "clear")


def render_avatar():
    from rich.text import Text

    avatar = Text()
    for row in PIXEL_COLORS:
        for cell in row:
            if cell is None:
                avatar.append("  ")
            else:
                avatar.append("  ", style=f"on {cell}")
        avatar.append("\n")
    return avatar


def print_banner() -> None:
    try:
        from rich import box
        from rich.align import Align
        from rich.console import Console, Group
        from rich.panel import Panel
        from rich.text import Text

        console = Console(soft_wrap=True)
        title = Text()
        title.append("Makima", style="bold #f6f0e8")
        title.append("  ", style="dim")
        title.append("Personal Agent", style="bold #d7b46a")

        content = Group(
            Align.center(render_avatar()),
            Text(""),
            Align.center(title),
            Align.center(Text("Precision. Memory. Control.", style="#f1a0a7")),
            Text(""),
            Text("  One-click launcher for the agent backend and CLI.", style="dim"),
            Text("  Private use only. Launches server, then opens chat.", style="dim"),
        )
        console.print(
            Panel(
                content,
                title="Makima",
                subtitle="launcher",
                border_style="#b24a56",
                box=box.DOUBLE,
                padding=(1, 2),
            )
        )
        console.print()
    except Exception:
        print("Makima - Personal Agent")
        print("One-click launcher for the backend and CLI.")
        print()


def check_dependencies() -> None:
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
            check=False,
        )


def cleanup_port() -> None:
    """Kill any existing Makima backend process before starting."""
    try:
        script = r"""
$pids = @()

try {
    $pids += Get-NetTCPConnection -LocalPort 8000 -State Listen -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty OwningProcess
} catch {}

try {
    $pids += Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
        Where-Object {
            $_.CommandLine -match 'uvicorn\s+makima\.app:app' -or
            $_.CommandLine -match 'makima\.app:app' -or
            $_.CommandLine -match '--app-dir\s+apps/backend/src'
        } |
        Select-Object -ExpandProperty ProcessId
} catch {}

foreach ($pid in ($pids | Where-Object { $_ } | Sort-Object -Unique)) {
    try {
        taskkill /F /T /PID $pid | Out-Null
    } catch {}
}
"""
        subprocess.run(
            ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script],
            capture_output=True,
            text=True,
            timeout=15,
            check=False,
        )
        time.sleep(1)
    except Exception:
        pass


def wait_for_server(timeout: int = 30) -> bool:
    start = time.time()
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    while time.time() - start < timeout:
        try:
            import json
            resp = opener.open(f"{SERVER_URL}/health", timeout=2)
            data = json.loads(resp.read().decode())
            # Verify it's actually the makima server, not a stale process
            if data.get("status") == "healthy" and data.get("version"):
                return True
        except Exception:
            time.sleep(1)
    return False


def main() -> None:
    clear_screen()
    print_banner()

    print("  Checking dependencies...")
    check_dependencies()

    # cleanup_port()  # Disabled: was killing its own uvicorn process

    print("  Starting server...")
    kwargs = {}
    if os.name == "nt":
        kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP

    server_proc = subprocess.Popen(
        [
            sys.executable,
            "-m",
            "uvicorn",
            "makima.app:app",
            "--host",
            "0.0.0.0",
            "--port",
            "8000",
            "--reload",
        ],
        cwd=BACKEND_DIR,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        **kwargs,
    )

    print("  Waiting for server to be ready...", end="", flush=True)
    if not wait_for_server(timeout=30):
        print("\n  [ERROR] Server failed to start within 30 seconds")
        print("  Check .env for configuration")
        cleanup_port()
        server_proc.terminate()
        input("\n  Press Enter to exit...")
        sys.exit(1)

    print(f"\r  [OK] Server ready on {SERVER_URL}                    ")
    print()

    try:
        subprocess.run(
            [sys.executable, CLI_SCRIPT, "--server", SERVER_URL],
            cwd=BASE_DIR,
            check=False,
        )
    except KeyboardInterrupt:
        pass

    print()
    print("  Stopping server...")
    try:
        server_proc.terminate()
        server_proc.wait(timeout=5)
    except Exception:
        server_proc.kill()

    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/F", "/IM", "uvicorn.exe"],
            capture_output=True,
            check=False,
        )

    print("  [OK] Server stopped")
    print()


if __name__ == "__main__":
    main()
