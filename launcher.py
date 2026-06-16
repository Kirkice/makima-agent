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
SERVER_URL = "http://localhost:8000"

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


def wait_for_server(timeout: int = 30) -> bool:
    start = time.time()
    while time.time() - start < timeout:
        try:
            urllib.request.urlopen(f"{SERVER_URL}/health", timeout=2)
            return True
        except Exception:
            time.sleep(1)
    return False


def main() -> None:
    clear_screen()
    print_banner()

    print("  Checking dependencies...")
    check_dependencies()

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
        print("  Check apps/backend/.env for configuration")
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
