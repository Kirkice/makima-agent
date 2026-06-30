#!/usr/bin/env python3
"""简化的情绪标签端到端测试"""

import requests
import json
import sseclient
import sys
import time

# 配置
BACKEND_URL = "http://127.0.0.1:8000"
TOKEN_FILE = "../token.txt"

def get_token():
    """从文件读取 token"""
    try:
        with open(TOKEN_FILE, 'r') as f:
            token = f.read().strip()
            if token == 'FAILED':
                print("❌ Token 获取失败")
                return None
            return token
    except Exception as e:
        print(f"❌ 读取 token 失败: {e}")
        return None

def create_session(headers):
    """创建一个新的 session"""
    try:
        response = requests.post(
            f"{BACKEND_URL}/sessions",
            headers=headers,
            json={"title": "Emotion Test Session"},
            timeout=10
        )
        
        # Accept both 200 (OK) and 201 (Created)
        if response.status_code in (200, 201):
            data = response.json()
            session_id = data.get('id')
            print(f"✓ 创建 session: {session_id}")
            return session_id
        else:
            print(f"❌ 创建 session 失败: {response.status_code}")
            print(f"   响应: {response.text[:200]}")
            return None
    except Exception as e:
        print(f"❌ 创建 session 异常: {e}")
        return None

def test_emotion_tags():
    """测试情绪标签生成"""
    print("=" * 60)
    print("情绪标签端到端测试")
    print("=" * 60)
    
    token = get_token()
    if not token:
        return False
    
    print(f"✓ 获取到 token: {token[:20]}...")
    
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json"
    }
    
    # 创建 session
    session_id = create_session(headers)
    if not session_id:
        return False
    
    # 测试消息 - 这些应该触发不同的情绪
    test_messages = [
        ("你好！今天过得怎么样？", "应该触发友好情绪"),
        ("我不明白这个问题", "应该触发困惑情绪"),
        ("太棒了！你做得很好！", "应该触发开心情绪"),
        ("这不对，请重新检查", "应该触发纠正情绪"),
    ]
    
    all_passed = True
    
    for i, (message, expected) in enumerate(test_messages, 1):
        print(f"\n[{i}/{len(test_messages)}] 测试: {message}")
        print(f"    预期: {expected}")
        
        try:
            # 发送任务请求（流式）
            response = requests.post(
                f"{BACKEND_URL}/tasks",
                headers=headers,
                json={
                    "session_id": session_id,
                    "input_text": message,
                    "mode_slug": "code"
                },
                stream=True,
                timeout=30
            )
            
            if response.status_code != 200:
                print(f"    ❌ 请求失败: {response.status_code}")
                print(f"       响应: {response.text[:200]}")
                all_passed = False
                continue
            
            # 解析 SSE 流
            client = sseclient.SSEClient(response)
            events = []
            
            for event in client.events():
                try:
                    data = json.loads(event.data)
                    event_type = data.get('type')
                    
                    # 提取内层 data
                    inner_data = data.get('data', {})
                    
                    if event_type == 'message':
                        content = inner_data.get('content', '')
                        print(f"    📝 收到消息: {content[:80]}...")
                        
                        # 检查是否包含情绪标签
                        if '[emotion:' in content:
                            # 提取情绪标签
                            import re
                            match = re.search(r'\[emotion:(\w+)\]', content)
                            if match:
                                emotion = match.group(1)
                                print(f"    ✅ 发现情绪标签: {emotion}")
                                events.append(('emotion', emotion))
                    
                    elif event_type == 'animation':
                        animation = inner_data.get('animation', 'unknown')
                        print(f"    🎭 动画事件: {animation}")
                        events.append(('animation', animation))
                    
                    elif event_type == 'done':
                        print(f"    ✓ 流结束")
                    
                    elif event_type == 'error':
                        error = data.get('error', 'unknown')
                        print(f"    ❌ 错误: {error}")
                
                except json.JSONDecodeError:
                    print(f"    ⚠️  无法解析: {event.data[:50]}")
            
            # 检查结果 - 检查 animation 事件（后端发送的独立动画事件）
            animation_events = [e for e in events if e[0] == 'animation']
            emotion_events = [e for e in events if e[0] in ('emotion', 'emotion_event')]
            
            # 合并所有情绪相关的事件
            all_emotion_events = animation_events + emotion_events
            
            if all_emotion_events:
                print(f"    ✅ 测试通过: 生成了 {len(all_emotion_events)} 个情绪/动画事件")
                for evt_type, emotion in all_emotion_events:
                    print(f"       - {evt_type}: {emotion}")
            else:
                print(f"    ❌ 测试失败: 没有生成情绪标签")
                print(f"       这可能是因为 LLM 没有遵循情绪指令")
                all_passed = False
        
        except requests.exceptions.Timeout:
            print(f"    ❌ 请求超时")
            all_passed = False
        except Exception as e:
            print(f"    ❌ 测试失败: {e}")
            import traceback
            traceback.print_exc()
            all_passed = False
    
    print("\n" + "=" * 60)
    if all_passed:
        print("✅ 所有测试通过！情绪标签功能正常工作")
        print("   后端能够正确生成情绪标签")
    else:
        print("❌ 部分或全部测试失败")
        print("   可能原因：")
        print("   1. LLM 没有遵循情绪指令生成 [emotion:xxx] 标签")
        print("   2. 后端 emotion_parser.py 没有正确提取标签")
        print("   3. SSE 流中没有发送 emotion 事件")
        print("   4. API 认证失败或其他后端错误")
    print("=" * 60)
    
    return all_passed

if __name__ == "__main__":
    try:
        test_emotion_tags()
    except KeyboardInterrupt:
        print("\n\n测试被用户中断")
        sys.exit(1)