#!/usr/bin/env python3
"""
独立 WebSocket 服务器 - 用于测试 Unity WebGL 动画接收

用途: 测试 Unity WebGL 是否能接收 WebSocket 消息并播放动画
不需要启动桌面应用、后端或数据库

使用方式:
    1. 运行此脚本启动 WebSocket 服务器
    2. 在浏览器中打开 WebGL (需要本地 HTTP 服务器)
    3. 运行 test_websocket_animation.py 发送测试消息
    4. 观察浏览器 Console 和 Spine 角色动画

依赖: pip install websockets
"""

import asyncio
import websockets
import json
from datetime import datetime

# 全局变量存储所有连接的客户端
connected_clients = set()


async def handler(websocket):
    """处理单个 WebSocket 连接"""
    # 注册新客户端
    connected_clients.add(websocket)
    client_addr = websocket.remote_address
    print(f"[{datetime.now().strftime('%H:%M:%S')}] ✓ 新客户端连接: {client_addr}")
    print(f"    当前连接数: {len(connected_clients)}")
    
    try:
        # 保持连接，接收并广播消息
        async for message in websocket:
            print(f"\n[{datetime.now().strftime('%H:%M:%S')}] 收到消息: {message}")
            
            # 尝试解析 JSON
            try:
                data = json.loads(message)
                print(f"    解析成功: {data}")
            except json.JSONDecodeError:
                print(f"    ⚠ JSON 解析失败")
            
            # 广播给所有连接的客户端
            if connected_clients:
                print(f"    广播给 {len(connected_clients)} 个客户端...")
                websockets.broadcast(connected_clients, message)
                print(f"    ✓ 广播完成")
    
    except websockets.exceptions.ConnectionClosed:
        print(f"\n[{datetime.now().strftime('%H:%M:%S')}] ✗ 客户端断开: {client_addr}")
    
    finally:
        # 移除断开的客户端
        connected_clients.discard(websocket)
        print(f"    当前连接数: {len(connected_clients)}")


async def main():
    """启动 WebSocket 服务器"""
    print("=" * 60)
    print("独立 WebSocket 服务器 - Unity WebGL 动画测试")
    print("=" * 60)
    print(f"监听地址: ws://127.0.0.1:9001")
    print(f"启动时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print()
    print("使用说明:")
    print("  1. 在浏览器中打开 WebGL (http://localhost:8080)")
    print("  2. 运行 test_websocket_animation.py 发送测试消息")
    print("  3. 观察浏览器 Console 和 Spine 角色动画")
    print()
    print("按 Ctrl+C 停止服务器")
    print("-" * 60)
    print()
    
    # 启动服务器
    async with websockets.serve(handler, "127.0.0.1", 9001):
        print("[服务器] WebSocket 服务器已启动")
        print("[服务器] 等待客户端连接...")
        print()
        await asyncio.Future()  # 永远运行


if __name__ == "__main__":
    # 检查依赖
    try:
        import websockets
    except ImportError:
        print("错误: 缺少 websockets 库")
        print("请运行: pip install websockets")
        exit(1)
    
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\n\n[服务器] 服务器已停止")