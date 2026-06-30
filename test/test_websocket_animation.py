#!/usr/bin/env python3
"""
Part 1: Unity WebGL WebSocket 接收测试脚本

用途: 验证 Unity WebGL 是否能接收 WebSocket 消息并播放动画
使用方式:
    1. 启动桌面应用 (cargo run) 以确保 WebSocket 服务器运行在 9001 端口
    2. 在浏览器中打开 character-webgl/index.html
    3. 运行此脚本发送测试消息
    4. 观察浏览器 Console 和 Spine 角色动画

依赖: pip install websockets
"""

import asyncio
import websockets
import sys
from datetime import datetime


async def send_animation_commands():
    """发送测试动画命令到 WebSocket 服务器"""
    uri = "ws://127.0.0.1:9001"
    
    print("=" * 60)
    print("Unity WebGL WebSocket 动画测试")
    print("=" * 60)
    print(f"连接到: {uri}")
    print(f"时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print()
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✓ WebSocket 连接成功!")
            print()
            
            # 测试动画列表
            test_animations = [
                "idle",
                "smile",
                "think",
                "no",
                "special",
                "action",
                "expression_0",
                "talk_start",
                "talk_end"
            ]
            
            print(f"将发送 {len(test_animations)} 个测试动画命令")
            print("每个动画之间等待 2 秒")
            print()
            print("请观察:")
            print("  1. 浏览器 Console (F12) 是否显示接收消息")
            print("  2. Spine 角色是否播放对应动画")
            print()
            print("-" * 60)
            
            for i, animation in enumerate(test_animations, 1):
                message = f'{{"animation":"{animation}"}}'
                await websocket.send(message)
                print(f"[{i}/{len(test_animations)}] 发送: {message}")
                
                if i < len(test_animations):
                    print(f"      等待 2 秒...")
                    await asyncio.sleep(2)
            
            print()
            print("-" * 60)
            print("✓ 测试完成!")
            print()
            print("如果 Spine 角色播放了动画，说明 Unity WebGL 端正常工作")
            print("如果没有播放动画，请检查:")
            print("  1. 浏览器 Console 是否有错误信息")
            print("  2. WebSocket 连接是否成功")
            print("  3. AgentAnimationBridge 是否正确挂载")
            print("  4. Spine 数据是否包含这些动画")
            
    except ConnectionRefusedError:
        print("✗ 连接失败: WebSocket 服务器未运行")
        print()
        print("请确保:")
        print("  1. 桌面应用已启动 (cargo run)")
        print("  2. WebSocket Bridge 运行在 ws://127.0.0.1:9001")
        sys.exit(1)
        
    except Exception as e:
        print(f"✗ 错误: {e}")
        sys.exit(1)


async def interactive_mode():
    """交互模式: 允许用户手动发送动画命令"""
    uri = "ws://127.0.0.1:9001"
    
    print("=" * 60)
    print("Unity WebGL WebSocket 动画测试 - 交互模式")
    print("=" * 60)
    print(f"连接到: {uri}")
    print()
    print("可用命令:")
    print("  idle, smile, think, no, special, action")
    print("  expression_0, talk_start, talk_end")
    print("  quit - 退出")
    print()
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✓ WebSocket 连接成功!")
            print()
            
            while True:
                animation = input("输入动画名称 > ").strip()
                
                if animation.lower() in ['quit', 'exit', 'q']:
                    print("退出...")
                    break
                
                if animation:
                    message = f'{{"animation":"{animation}"}}'
                    await websocket.send(message)
                    print(f"发送: {message}")
                    print()
                    
    except ConnectionRefusedError:
        print("✗ 连接失败: WebSocket 服务器未运行")
        sys.exit(1)
        
    except KeyboardInterrupt:
        print("\n退出...")


async def main():
    """主函数"""
    if len(sys.argv) > 1 and sys.argv[1] == '--interactive':
        await interactive_mode()
    else:
        await send_animation_commands()


if __name__ == "__main__":
    # 检查依赖
    try:
        import websockets
    except ImportError:
        print("错误: 缺少 websockets 库")
        print("请运行: pip install websockets")
        sys.exit(1)
    
    asyncio.run(main())