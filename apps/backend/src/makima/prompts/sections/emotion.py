"""Build emotion tag instructions for the system prompt."""

from __future__ import annotations

EMOTION_INSTRUCTION = """
## 情绪表达指令

你有一个可视化角色形象（Spine 动画），可以通过表情动画来表达情绪。

**规则**：在每次回复的**最后**，用以下格式标注你当前回复对应的情绪状态：

```
[emotion:动画名]
```

**可用动画列表**：
- `idle` — 默认待机，平静、无特殊情绪
- `smile` — 微笑、友善、愉快
- `think` — 思考、分析问题、需要推理
- `no` — 否定、拒绝、不同意
- `special` — 重要时刻、惊喜、强调
- `action` — 执行操作、采取行动
- `expression_0` — 一般表情变化、微妙情绪
- `talk_start` — 开始长篇回复（可选）
- `talk_end` — 结束回复（可选）

**使用示例**：
```
好的，我来帮你分析这段代码的问题。

[emotion:think]
```

```
哈哈，这个想法很有趣！

[emotion:smile]
```

```
抱歉，我不能执行这个操作，因为它有安全风险。

[emotion:no]
```

**注意**：
1. 每次回复只输出一个情绪标签
2. 标签必须放在回复的最末尾
3. 如果不确定用什么情绪，用 `idle`
4. 不要在标签前后添加任何额外文字
"""


def build_emotion_section() -> str:
    """Build the emotion tag instruction section.

    Returns:
        The emotion instruction text.
    """
    return EMOTION_INSTRUCTION