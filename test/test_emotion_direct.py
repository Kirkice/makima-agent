#!/usr/bin/env python3
"""
直接测试情绪标签解析和 SSE 事件生成
不需要认证，直接测试核心功能
"""

import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'apps', 'backend', 'src'))

from makima.orchestrator.emotion_parser import extract_emotion

def test_emotion_parser():
    """测试情绪解析器"""
    print("=" * 60)
    print("测试 1: 情绪标签解析器")
    print("=" * 60)
    
    test_cases = [
        ("你好！很高兴见到你 [emotion:smile]", "smile"),
        ("让我想想这个问题... [emotion:think]", "think"),
        ("不，我不这么认为 [emotion:no]", "no"),
        ("太棒了！[emotion:special]", "special"),
        ("开始说话 [emotion:talk_start]", "talk_start"),
        ("说完了 [emotion:talk_end]", "talk_end"),
        ("没有情绪标签的普通消息", None),
        ("无效情绪 [emotion:invalid]", "idle"),  # 应该回退到 idle
    ]
    
    passed = 0
    failed = 0
    
    for text, expected in test_cases:
        cleaned, animation = extract_emotion(text)
        
        if animation == expected:
            status = "✓"
            passed += 1
        else:
            status = "✗"
            failed += 1
        
        print(f"\n{status} 输入: {text}")
        print(f"  清理后: {cleaned}")
        print(f"  动画: {animation}")
        print(f"  期望: {expected}")
    
    print(f"\n{'=' * 60}")
    print(f"结果: {passed} 通过, {failed} 失败")
    print(f"{'=' * 60}")
    
    return failed == 0

def test_sse_event_structure():
    """测试 SSE 事件结构"""
    print("\n" + "=" * 60)
    print("测试 2: SSE 事件结构")
    print("=" * 60)
    
    from makima_schemas.events import AgentEvent, AgentEventType
    
    # 测试 ANIMATION 事件类型是否存在
    if hasattr(AgentEventType, 'ANIMATION'):
        print("✓ AgentEventType.ANIMATION 存在")
    else:
        print("✗ AgentEventType.ANIMATION 不存在")
        return False
    
    # 测试创建 ANIMATION 事件
    try:
        event = AgentEvent(
            type=AgentEventType.ANIMATION,
            data={"animation": "smile"},
            timestamp=1234567890.0,
            step=5,
        )
        print(f"✓ 成功创建 ANIMATION 事件")
        print(f"  类型: {event.type}")
        print(f"  数据: {event.data}")
        
        # 测试序列化
        json_str = event.model_dump_json()
        print(f"  JSON: {json_str}")
        
        return True
    except Exception as e:
        print(f"✗ 创建 ANIMATION 事件失败: {e}")
        return False

def test_system_prompt():
    """测试系统提示词是否包含情绪指令"""
    print("\n" + "=" * 60)
    print("测试 3: 系统提示词检查")
    print("=" * 60)
    
    try:
        from makima.prompts.engine import PromptEngine
        from makima.modes import ModeRegistry
        
        engine = PromptEngine()
        mode = ModeRegistry.get_default()
        
        # 构建系统提示词
        system_prompt = engine.build(mode=mode)
        
        # 检查是否包含情绪相关指令
        emotion_keywords = ["emotion", "情绪", "表情", "动画"]
        found_keywords = [kw for kw in emotion_keywords if kw.lower() in system_prompt.lower()]
        
        if found_keywords:
            print(f"✓ 系统提示词包含情绪相关关键词: {found_keywords}")
            return True
        else:
            print("✗ 系统提示词未包含情绪相关关键词")
            print(f"  期望找到: {emotion_keywords}")
            return False
            
    except Exception as e:
        print(f"✗ 检查系统提示词失败: {e}")
        import traceback
        traceback.print_exc()
        return False

if __name__ == "__main__":
    print("\n" + "=" * 60)
    print("Agent 端情绪标签功能直接测试")
    print("=" * 60)
    
    results = []
    
    results.append(("情绪解析器", test_emotion_parser()))
    results.append(("SSE 事件结构", test_sse_event_structure()))
    results.append(("系统提示词", test_system_prompt()))
    
    print("\n" + "=" * 60)
    print("总结")
    print("=" * 60)
    
    for name, passed in results:
        status = "✓ 通过" if passed else "✗ 失败"
        print(f"{status}: {name}")
    
    all_passed = all(passed for _, passed in results)
    
    if all_passed:
        print("\n✓ 所有测试通过！Agent 端功能正常")
        print("\n下一步:")
        print("  1. 启动后端服务")
        print("  2. 在对话中测试是否生成情绪标签")
        print("  3. 检查 Rust 桌面应用是否转发到 WebSocket")
    else:
        print("\n✗ 部分测试失败，需要修复")
        sys.exit(1)