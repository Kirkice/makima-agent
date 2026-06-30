# 单模型 Spine 动画管理器实现思路

**创建日期**: 2026-06-30  
**适用工程**: `agent-role/`  
**目标**: 仅保留一个 Spine 模型，为其提供统一的 C# 动画管理器，默认 `idle` 循环，其余动画通过清晰的 C# 接口暴露给上层 agent 调用。

---

## 1. 目标

当前需求不是再做多模型切换，也不是做复杂的状态机编辑器，而是收敛成一条简单稳定的链路：

1. 场景里只保留一个 Spine 角色对象
2. 默认状态播放 `idle`，并持续循环
3. 对外暴露明确的动画接口，例如：
   - `PlaySmile()`
   - `PlayThink()`
   - `PlayTalkStart()`
   - `PlayTalkEnd()`
   - `PlaySpecial()`
4. 同时提供一个通用入口，允许 agent 层通过字符串或命令名调用：
   - `PlayCommand("smile")`
   - `PlayAnimationByName("special")`
5. 非 idle 动画播完后，自动回到 `idle`

这套设计的重点是：

- 对 Unity 侧足够简单
- 对 agent 侧足够稳定
- 不把业务逻辑散落在多个脚本里

---

## 2. 当前动画集合

从现有 Spine 资源可见，可用动画包括：

- `action`
- `expression_0`
- `idle`
- `no`
- `smile`
- `special`
- `talk_end`
- `talk_start`
- `think`

其中推荐将它们分为两类：

### 2.1 常驻动画

- `idle`

特点：

- 循环播放
- 作为默认/回退状态

### 2.2 瞬时动作动画

- `action`
- `expression_0`
- `no`
- `smile`
- `special`
- `talk_start`
- `talk_end`
- `think`

特点：

- 默认按非循环处理
- 播放完成后自动回到 `idle`

备注：

- `think` 是否循环，取决于你后续想让它代表“短促思考表情”还是“持续思考状态”
- 第一版建议先按“单次播放后回 idle”处理，最简单

---

## 3. 推荐脚本结构

建议拆成两层，而不是一个脚本把所有事情都做完。

### 3.1 `SpineAnimationManager.cs`

职责：

- 持有 `SkeletonAnimation`
- 负责播放、切换、回 idle
- 对外暴露强类型 C# 方法
- 对外暴露通用字符串命令接口

建议路径：

`agent-role/Assets/Scripts/SpineAnimationManager.cs`

### 3.2 可选：`AgentAnimationBridge.cs`

职责：

- 面向 agent 层做一层更薄的适配
- 把外部命令映射到 `SpineAnimationManager`
- 如果后续 Unity 和 WebGL/宿主之间要做 JSON IPC，这层会更清晰

建议路径：

`agent-role/Assets/Scripts/AgentAnimationBridge.cs`

如果第一版想尽量简单，也可以先不拆第二层，直接在 `SpineAnimationManager` 里实现字符串入口。

---

## 4. 推荐挂载方式

场景中的 Spine 角色对象建议结构如下：

```text
MakimaAvatar
├─ SkeletonAnimation
├─ SpineAnimationManager
└─ （可选）AgentAnimationBridge
```

也就是说：

- `SkeletonAnimation` 继续使用 spine-unity 官方组件
- `SpineAnimationManager` 挂在同一个 GameObject 上
- 管理器通过 `GetComponent<SkeletonAnimation>()` 取得底层动画能力

这样做的好处是：

- 不依赖已经删除的 `SpineModelSetup`
- 不依赖运行时动态创建模型
- 直接围绕“场景里唯一一个角色”组织代码

---

## 5. 管理器的核心职责

`SpineAnimationManager` 需要解决 5 件事。

### 5.1 启动时进入 idle

在 `Awake()` 或 `Start()` 中：

1. 获取 `SkeletonAnimation`
2. 校验 `idle` 动画是否存在
3. 调用 `SetAnimation(0, "idle", true)`

### 5.2 提供强类型接口

直接暴露清晰方法，减少 agent 层误调用：

```csharp
public void PlayIdle();
public void PlaySmile();
public void PlayThink();
public void PlayAction();
public void PlayExpression0();
public void PlayNo();
public void PlaySpecial();
public void PlayTalkStart();
public void PlayTalkEnd();
```

优点：

- Inspector 或其他脚本调用时更直观
- 后续做按钮测试也方便

### 5.3 提供通用入口

除了强类型方法，再提供两类通用接口：

```csharp
public bool PlayAnimationByName(string animationName, bool loop = false, bool returnToIdle = true);
public bool PlayCommand(string command);
```

`PlayCommand` 建议做别名映射，例如：

- `"idle"` → `idle`
- `"smile"` → `smile`
- `"think"` → `think`
- `"talk_start"` → `talk_start`
- `"talk_end"` → `talk_end`
- `"expression"` → `expression_0`

这样 agent 层可以不直接依赖 Spine 原始动画名。

### 5.4 非循环动画播放结束后回 idle

这是这个管理器最关键的逻辑。

推荐做法：

1. 用 `AnimationState.SetAnimation(0, target, false)` 播放动作
2. 记录“当前动作播完后是否要回 idle”
3. 监听当前 `TrackEntry` 的完成事件
4. 完成后切回 `idle`

逻辑上要避免两个坑：

- 动画被中途打断时，不要错误地回 idle
- 当前动作已经不是当初那条 `TrackEntry` 时，不要处理旧回调

### 5.5 查询能力

建议额外提供：

```csharp
public string CurrentAnimationName { get; }
public string[] GetAvailableAnimations();
public bool HasAnimation(string animationName);
```

这些接口对调试和 agent 自检都很有用。

---

## 6. 推荐的动画行为策略

第一版建议用非常保守的策略：

| 动画 | 默认循环 | 结束后回 Idle | 用途建议 |
|---|---|---|---|
| `idle` | 是 | 否 | 默认待机 |
| `smile` | 否 | 是 | 短促表情 |
| `think` | 否 | 是 | 思考表情 |
| `no` | 否 | 是 | 否定反馈 |
| `special` | 否 | 是 | 特殊动作 |
| `action` | 否 | 是 | 一般动作 |
| `expression_0` | 否 | 是 | 预设表情 |
| `talk_start` | 否 | 是 | 开口起始 |
| `talk_end` | 否 | 是 | 说话结束 |

后续如果要支持更细的语音联动，可以升级为：

- `talk_start`：播放一次
- `idle`：说话中作为底层循环
- `talk_end`：说完时播放一次

或者增加真正的 `talk_loop` 动画时，再单独接入。

---

## 7. 对 agent 层暴露的推荐协议

为了让 AI 更容易调用，建议不要让 agent 直接操作过多 Spine 细节，而是暴露一组稳定命令。

### 7.1 最小命令集

```json
{ "type": "play_animation", "name": "smile" }
{ "type": "play_animation", "name": "think" }
{ "type": "play_animation", "name": "talk_start" }
{ "type": "play_animation", "name": "talk_end" }
{ "type": "play_animation", "name": "idle" }
```

### 7.2 Unity 侧入口建议

```csharp
public bool ExecuteAgentCommand(string command)
public bool ExecuteAgentCommand(string command, string argument)
```

或者统一一个：

```csharp
public bool ExecuteCommand(string name)
```

如果未来需要通过 WebGL / JS / Rust IPC 传递，只要把宿主消息最后路由到这个入口即可。

---

## 8. 推荐实现草图

下面是建议的骨架，不是最终代码，只是说明结构。

```csharp
using System;
using System.Collections.Generic;
using Spine;
using Spine.Unity;
using UnityEngine;

[DisallowMultipleComponent]
[RequireComponent(typeof(SkeletonAnimation))]
public class SpineAnimationManager : MonoBehaviour
{
    [SerializeField] private string idleAnimation = "idle";
    [SerializeField] private int trackIndex = 0;

    private SkeletonAnimation skeletonAnimation;
    private AnimationState animationState;
    private TrackEntry activeEntry;

    public string CurrentAnimationName =>
        animationState?.GetCurrent(trackIndex)?.Animation?.Name ?? string.Empty;

    private void Awake()
    {
        skeletonAnimation = GetComponent<SkeletonAnimation>();
        animationState = skeletonAnimation.AnimationState;
    }

    private void Start()
    {
        PlayIdle();
    }

    public void PlayIdle()
    {
        PlayAnimationByName(idleAnimation, true, false);
    }

    public bool PlaySmile() => PlayAnimationByName("smile", false, true);
    public bool PlayThink() => PlayAnimationByName("think", false, true);
    public bool PlayAction() => PlayAnimationByName("action", false, true);
    public bool PlayExpression0() => PlayAnimationByName("expression_0", false, true);
    public bool PlayNo() => PlayAnimationByName("no", false, true);
    public bool PlaySpecial() => PlayAnimationByName("special", false, true);
    public bool PlayTalkStart() => PlayAnimationByName("talk_start", false, true);
    public bool PlayTalkEnd() => PlayAnimationByName("talk_end", false, true);

    public bool PlayCommand(string command)
    {
        switch (command)
        {
            case "idle": return PlayAnimationByName("idle", true, false);
            case "smile": return PlaySmile();
            case "think": return PlayThink();
            case "action": return PlayAction();
            case "expression":
            case "expression_0": return PlayExpression0();
            case "no": return PlayNo();
            case "special": return PlaySpecial();
            case "talk_start": return PlayTalkStart();
            case "talk_end": return PlayTalkEnd();
            default: return false;
        }
    }

    public bool PlayAnimationByName(string animationName, bool loop, bool returnToIdle)
    {
        var data = skeletonAnimation.Skeleton.Data;
        var animation = data.FindAnimation(animationName);
        if (animation == null) return false;

        var entry = animationState.SetAnimation(trackIndex, animationName, loop);
        activeEntry = entry;

        if (!loop && returnToIdle)
        {
            entry.Complete += OnOneShotAnimationComplete;
        }

        return true;
    }

    private void OnOneShotAnimationComplete(TrackEntry entry)
    {
        if (entry != activeEntry) return;
        PlayIdle();
    }
}
```

---

## 9. 第二阶段可扩展点

如果第一版跑稳，后面可以继续增强：

### 9.1 动画优先级

例如：

- `special` 可以打断 `smile`
- `talk_end` 不应打断 `special`

可加入简单优先级表。

### 9.2 冷却时间

避免 agent 在极短时间内连续触发动画导致抖动：

- 表情动画最短间隔 `0.3s`
- 特殊动作最短间隔 `1.0s`

### 9.3 队列机制

有些命令可以排队，而不是立即打断：

- `talk_start` -> `talk_end`
- `smile` -> `idle`

### 9.4 回调事件

给上层反馈动画状态：

```csharp
public event Action<string> OnAnimationStarted;
public event Action<string> OnAnimationCompleted;
```

这样 agent 层或宿主层可以知道动作何时结束。

---

## 10. 建议落地顺序

### 阶段 1

- 在场景唯一 Spine 模型上挂 `SkeletonAnimation`
- 新建 `SpineAnimationManager.cs`
- 实现 `idle` 默认循环
- 实现 `PlaySmile / PlayThink / PlayTalkStart / PlayTalkEnd`
- 实现 `PlayCommand(string)`

### 阶段 2

- 增加动画完成回调
- 增加命令别名映射
- 增加 `GetAvailableAnimations()`

### 阶段 3

- 接宿主层 / agent 层 IPC
- 让 AI 通过统一命令调用动画

---

## 11. 结论

这次最合适的方案不是复杂状态机，而是：

- 一个场景内唯一 Spine 模型
- 一个 `SpineAnimationManager`
- 一个默认 `idle`
- 一组强类型 C# 方法
- 一个给 agent 层使用的字符串命令入口

这能最快把“角色动画能力”稳定交给上层 AI 使用，同时保留后续做 IPC、优先级和语音联动的扩展空间。

