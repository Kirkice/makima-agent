using System;
using UnityEngine;

/// <summary>
/// Agent 动画桥接层 - 为 Agent/WebGL/IPC 提供统一的动画控制接口
/// 作为 SpineAnimationManager 的薄适配层，处理命令路由和协议转换
/// </summary>
[DisallowMultipleComponent]
[RequireComponent(typeof(SpineAnimationManager))]
public class AgentAnimationBridge : MonoBehaviour
{
    private SpineAnimationManager animationManager;

    /// <summary>
    /// 获取底层的 SpineAnimationManager
    /// </summary>
    public SpineAnimationManager AnimationManager => animationManager;

    private void Awake()
    {
        animationManager = GetComponent<SpineAnimationManager>();
        if (animationManager == null)
        {
            Debug.LogError($"[{nameof(AgentAnimationBridge)}] SpineAnimationManager component not found!");
        }
    }

    #region JSON 命令接口

    /// <summary>
    /// 执行 JSON 格式的命令
    /// 格式: {"type": "play_animation", "name": "smile"}
    /// </summary>
    /// <param name="jsonCommand">JSON 命令字符串</param>
    /// <returns>是否成功执行</returns>
    public bool ExecuteJsonCommand(string jsonCommand)
    {
        if (string.IsNullOrEmpty(jsonCommand))
        {
            Debug.LogWarning($"[{nameof(AgentAnimationBridge)}] JSON command is null or empty");
            return false;
        }

        try
        {
            var command = JsonUtility.FromJson<AnimationCommand>(jsonCommand);
            return ExecuteCommand(command);
        }
        catch (Exception e)
        {
            Debug.LogError($"[{nameof(AgentAnimationBridge)}] Failed to parse JSON command: {e.Message}");
            return false;
        }
    }

    #endregion

    #region 命令对象接口

    /// <summary>
    /// 执行命令对象
    /// </summary>
    /// <param name="command">命令对象</param>
    /// <returns>是否成功执行</returns>
    public bool ExecuteCommand(AnimationCommand command)
    {
        if (command == null)
        {
            Debug.LogWarning($"[{nameof(AgentAnimationBridge)}] Command is null");
            return false;
        }

        if (animationManager == null)
        {
            Debug.LogError($"[{nameof(AgentAnimationBridge)}] AnimationManager not initialized");
            return false;
        }

        switch (command.type?.ToLowerInvariant())
        {
            case "play_animation":
            case "play":
                return animationManager.PlayCommand(command.name);

            case "play_loop":
                return animationManager.PlayAnimationByName(command.name, loop: true, returnToIdle: false);

            case "play_once":
                return animationManager.PlayAnimationByName(command.name, loop: false, returnToIdle: true);

            case "idle":
                animationManager.PlayIdle();
                return true;

            default:
                Debug.LogWarning($"[{nameof(AgentAnimationBridge)}] Unknown command type: {command.type}");
                return false;
        }
    }

    #endregion

    #region 简化接口

    /// <summary>
    /// 简化的动画播放接口（直接传动画名）
    /// </summary>
    /// <param name="animationName">动画名称</param>
    /// <returns>是否成功播放</returns>
    public bool Play(string animationName)
    {
        if (animationManager == null) return false;
        return animationManager.PlayCommand(animationName);
    }

    /// <summary>
    /// 播放动画（带循环控制）
    /// </summary>
    /// <param name="animationName">动画名称</param>
    /// <param name="loop">是否循环</param>
    /// <returns>是否成功播放</returns>
    public bool Play(string animationName, bool loop)
    {
        if (animationManager == null) return false;
        return animationManager.PlayAnimationByName(animationName, loop, returnToIdle: !loop);
    }

    /// <summary>
    /// 回到 idle 状态
    /// </summary>
    public void Idle()
    {
        animationManager?.PlayIdle();
    }

    #endregion

    #region 查询接口（透传）

    /// <summary>
    /// 获取当前动画名称
    /// </summary>
    public string GetCurrentAnimation()
    {
        return animationManager?.CurrentAnimationName ?? string.Empty;
    }

    /// <summary>
    /// 获取所有可用动画
    /// </summary>
    public string[] GetAvailableAnimations()
    {
        return animationManager?.GetAvailableAnimations() ?? Array.Empty<string>();
    }

    /// <summary>
    /// 检查动画是否存在
    /// </summary>
    public bool HasAnimation(string animationName)
    {
        return animationManager?.HasAnimation(animationName) ?? false;
    }

    /// <summary>
    /// 获取动画播放进度（0-1）
    /// </summary>
    public float GetProgress()
    {
        return animationManager?.GetAnimationProgress() ?? 0f;
    }

    /// <summary>
    /// 检查是否正在播放 idle
    /// </summary>
    public bool IsIdle()
    {
        return animationManager?.IsIdle() ?? false;
    }

    #endregion

    #region 事件订阅

    /// <summary>
    /// 订阅动画开始事件
    /// </summary>
    public void SubscribeAnimationStarted(Action<string> callback)
    {
        if (animationManager != null)
        {
            animationManager.OnAnimationStarted += callback;
        }
    }

    /// <summary>
    /// 取消订阅动画开始事件
    /// </summary>
    public void UnsubscribeAnimationStarted(Action<string> callback)
    {
        if (animationManager != null)
        {
            animationManager.OnAnimationStarted -= callback;
        }
    }

    /// <summary>
    /// 订阅动画完成事件
    /// </summary>
    public void SubscribeAnimationCompleted(Action<string> callback)
    {
        if (animationManager != null)
        {
            animationManager.OnAnimationCompleted += callback;
        }
    }

    /// <summary>
    /// 取消订阅动画完成事件
    /// </summary>
    public void UnsubscribeAnimationCompleted(Action<string> callback)
    {
        if (animationManager != null)
        {
            animationManager.OnAnimationCompleted -= callback;
        }
    }

    /// <summary>
    /// 订阅动画中断事件
    /// </summary>
    public void SubscribeAnimationInterrupted(Action<string> callback)
    {
        if (animationManager != null)
        {
            animationManager.OnAnimationInterrupted += callback;
        }
    }

    /// <summary>
    /// 取消订阅动画中断事件
    /// </summary>
    public void UnsubscribeAnimationInterrupted(Action<string> callback)
    {
        if (animationManager != null)
        {
            animationManager.OnAnimationInterrupted -= callback;
        }
    }

    #endregion
}

/// <summary>
/// 动画命令数据结构
/// </summary>
[Serializable]
public class AnimationCommand
{
    /// <summary>
    /// 命令类型: play_animation, play_loop, play_once, idle
    /// </summary>
    public string type;

    /// <summary>
    /// 动画名称
    /// </summary>
    public string name;

    /// <summary>
    /// 额外参数（预留）
    /// </summary>
    public string argument;
}