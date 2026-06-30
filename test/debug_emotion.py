#!/usr/bin/env python3
"""调试情绪标签 - 打印完整的 LLM 响应"""

import requests
import json
import sseclient
import sys

BACKEND_URL = "http://127.0.0.1:8000"

def get_token():
    with open("../token.txt", 'r') as f:
        return f.read().strip()

def create_session(headers):
    response = requests.post(
        f"{BACKEND_URL}/sessions",
        headers=headers,
        json={"title": "Debug Session"},
        timeout=10
    )
    if response.status_code in (200, 201):
        return response.json().get('id')
    return None

def debug_emotion():
    print("=" * 60)
    print("情绪标签调试 - 完整响应")
    print("=" * 60)
    
    token = get_token()
    print(f"Token: {token[:20]}...")
    
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json"
    }
    
    session_id = create_session(headers)
    if not session_id:
        print("❌ 创建 session 失败")
        return
    
    print(f"Session: {session_id}")
    
    message = "你好！今天过得怎么样？"
    print(f"\n发送消息: {message}")
    print("-" * 60)
    
    response = requests.post(
        f"{BACKEND_URL}/tasks",
        headers=headers,
        json={
            "session_id": session_id,
            "input_text": message,
            "mode_slug": "code"
        },
        stream=True,
        timeout=60
    )
    
    if response.status_code != 200:
        print(f"❌ 请求失败: {response.status_code}")
        print(response.text)
        return
    
    print("\n=== 所有 SSE 事件 ===")
    client = sseclient.SSEClient(response)
    
    for event in client.events():
        try:
            data = json.loads(event.data)
            event_type = data.get('type', 'unknown')
            
            print(f"\n📦 事件类型: {event_type}")
            print(f"   数据: {json.dumps(data, ensure_ascii=False, indent=2)}")
            
            # 提取内层 data
            inner_data = data.get('data', {})
            
            # 特别检查 message 事件
            if event_type == 'message':
                content = inner_data.get('content', '')
                print(f"\n   🔍 完整内容 (长度: {len(content)}):")
                print(f"   {repr(content)}")
                
                # 检查是否包含 emotion 标签
                if '[emotion:' in content:
                    print(f"\n   ✅ 发现 [emotion:] 标签！")
                else:
                    print(f"\n   ❌ 没有发现 [emotion:] 标签")
            
            # 检查 animation 事件
            elif event_type == 'animation':
                animation = inner_data.get('animation', '')
                print(f"\n   🎭 动画事件: {animation}")
        
        except json.JSONDecodeError as e:
            print(f"   ⚠️ JSON 解析失败: {e}")
            print(f"   原始数据: {event.data}")
    
    print("\n" + "=" * 60)
    print("调试完成")
    print("=" * 60)

if __name__ == "__main__":
    try:
        debug_emotion()
    except KeyboardInterrupt:
        print("\n中断")