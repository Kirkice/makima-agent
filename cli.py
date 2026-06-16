#!/usr/bin/env python3
"""Makima Agent CLI - 命令行交互客户端"""

import sys
import json
import httpx
import argparse
from typing import Optional

# 默认服务器地址
DEFAULT_SERVER = "http://localhost:8000"

class MakimaCLI:
    def __init__(self, server_url: str = DEFAULT_SERVER):
        self.server_url = server_url.rstrip('/')
        self.client = httpx.Client(timeout=60.0)
        self.token: Optional[str] = None
        self.user_id: Optional[str] = None
        self.session_id: Optional[str] = None

    def login(self, username: str, password: str) -> bool:
        """登录或注册"""
        # 尝试登录
        resp = self.client.post(
            f"{self.server_url}/auth/login",
            json={"username": username, "password": password}
        )
        
        if resp.status_code == 200:
            data = resp.json()
            self.token = data["access_token"]
            self.user_id = data["user_id"]
            print(f"✅ 登录成功: {username}")
            return True
        
        # 登录失败，尝试注册
        resp = self.client.post(
            f"{self.server_url}/auth/register",
            json={"username": username, "email": f"{username}@local", "password": password}
        )
        
        if resp.status_code == 200:
            data = resp.json()
            self.token = data["access_token"]
            self.user_id = data["user_id"]
            print(f"✅ 注册并登录成功: {username}")
            return True
        
        print(f"❌ 认证失败: {resp.text}")
        return False

    def create_session(self, title: str = "CLI Chat") -> bool:
        """创建会话"""
        headers = {"Authorization": f"Bearer {self.token}"}
        resp = self.client.post(
            f"{self.server_url}/sessions",
            json={"title": title},
            headers=headers
        )
        
        if resp.status_code in (200, 201):
            data = resp.json()
            self.session_id = data["id"]
            print(f"✅ 会话已创建: {title} (ID: {self.session_id[:8]}...)")
            return True
        
        print(f"❌ 创建会话失败: {resp.text}")
        return False

    def send_message(self, message: str):
        """发送消息并流式接收响应"""
        headers = {"Authorization": f"Bearer {self.token}"}
        
        try:
            with self.client.stream(
                "POST",
                f"{self.server_url}/tasks",
                json={
                    "session_id": self.session_id,
                    "input_text": message
                },
                headers=headers
            ) as resp:
                if resp.status_code != 200:
                    print(f"❌ 请求失败: {resp.status_code}")
                    return
                
                print("\n🤖 Agent: ", end="", flush=True)
                
                for line in resp.iter_lines():
                    if not line:
                        continue
                    
                    # 解析 SSE 事件
                    if line.startswith("event:"):
                        event_type = line[6:].strip()
                    elif line.startswith("data:"):
                        data_str = line[5:].strip()
                        try:
                            data = json.loads(data_str)
                            
                            if event_type == "thinking":
                                phase = data.get('data', {}).get('phase', '')
                                print(f"\n💭 思考: {phase}" if phase else "\n💭 思考中...", flush=True)
                            elif event_type == "tool_call":
                                tool_name = data.get('data', {}).get('tool', 'unknown')
                                print(f"\n🔧 工具调用: {tool_name}", flush=True)
                            elif event_type == "tool_result":
                                tool_name = data.get('data', {}).get('tool', 'unknown')
                                output = data.get('data', {}).get('output', '')
                                print(f"\n📝 {tool_name} 结果: {output[:100]}..." if len(output) > 100 else f"\n📝 {tool_name} 结果: {output}", flush=True)
                            elif event_type == "message":
                                content = data.get('data', {}).get('content', '')
                                if content:
                                    print(content, end="", flush=True)
                            elif event_type == "done":
                                print("\n", flush=True)
                            elif event_type == "error":
                                error_msg = data.get('data', {}).get('error', '')
                                print(f"\n❌ 错误: {error_msg}", flush=True)
                        except json.JSONDecodeError:
                            pass
        
        except Exception as e:
            print(f"\n❌ 通信错误: {e}")

    def run(self):
        """运行交互式 CLI"""
        print("=" * 60)
        print("🤖 Makima Agent CLI")
        print("=" * 60)
        print()
        
        # 获取用户信息
        username = input("👤 用户名: ").strip()
        if not username:
            username = "cli_user"
        
        password = input("🔑 密码: ").strip()
        if not password:
            password = "cli_pass"
        
        print()
        
        # 登录
        if not self.login(username, password):
            sys.exit(1)
        
        # 创建会话
        session_title = input("💬 会话标题 (回车使用默认): ").strip()
        if not session_title:
            session_title = f"CLI Chat - {username}"
        
        if not self.create_session(session_title):
            sys.exit(1)
        
        print()
        print("=" * 60)
        print("开始对话 (输入 'exit' 或 'quit' 退出)")
        print("=" * 60)
        print()
        
        # 交互式循环
        while True:
            try:
                message = input("🧑 You: ").strip()
                
                if not message:
                    continue
                
                if message.lower() in ['exit', 'quit', '退出']:
                    print("\n👋 再见！")
                    break
                
                self.send_message(message)
                print()
                
            except KeyboardInterrupt:
                print("\n\n👋 再见！")
                break
            except EOFError:
                print("\n\n👋 再见！")
                break

def main():
    parser = argparse.ArgumentParser(description="Makima Agent CLI - 命令行交互客户端")
    parser.add_argument(
        "--server",
        default=DEFAULT_SERVER,
        help=f"服务器地址 (默认: {DEFAULT_SERVER})"
    )
    
    args = parser.parse_args()
    
    cli = MakimaCLI(args.server)
    cli.run()

if __name__ == "__main__":
    main()