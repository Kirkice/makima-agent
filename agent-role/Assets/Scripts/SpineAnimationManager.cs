using System;
using System.Collections.Generic;
using System.Linq;
using Spine;
using Spine.Unity;
using UnityEngine;

/// <summary>
/// Spine 动画管理器 - 管理单个 Spine 角色的动画播放
/// 提供强类型方法和通用命令入口，支持自动回 idle 机制
/// </summary>
[DisallowMultipleComponent]
[RequireComponent(typeof(SkeletonAnimation))]
public class SpineAnimationManager : MonoBehaviour
{
    [Header("基础配置")]
    [SerializeField] private string idleAnimation = "idle";
    [SerializeField] private int trackIndex = 0;
    
    [Header("调试")]
    [SerializeField] private bool logAnimationChanges = true;

    private SkeletonAnimation skeletonAnimation;
    private Spine.AnimationState animationState;
    private TrackEntry activeEntry;
    private string currentAnimationName = string.Empty;

    // 回调事件
    public event Action<string> OnAnimationStarted;
    public event Action<string> OnAnimationCompleted;
    public event Action<string> OnAnimationInterrupted;

    /// <summary>
    /// 当前播放的动画名称
    /// </summary>
    public string CurrentAnimationName => currentAnimationName;

    /// <summary>
    /// 获取底层 SkeletonAnimation 组件
    /// </summary>
    public SkeletonAnimation SkeletonAnimationComponent => skeletonAnimation;

    private void Awake()
    {
        skeletonAnimation = GetComponent<SkeletonAnimation>();
        if (skeletonAnimation == null)
        {
            Debug.LogError($"[{nameof(SpineAnimationManager)}] SkeletonAnimation component not found!");
            return;
        }

        animationState = skeletonAnimation.AnimationState;
        if (animationState == null)
        {
            Debug.LogError($"[{nameof(SpineAnimationManager)}] Failed to get AnimationState!");
        }
    }

    private void Start()
    {
        PlayIdle();
    }

    #region 强类型动画接口

    /// <summary>
    /// 播放 idle 动画（循环）
    /// </summary>
    public void PlayIdle()
    {
        PlayAnimationByName(idleAnimation, loop: true, returnToIdle: false);
    }

    /// <summary>
    /// 播放微笑动画
    /// </summary>
    public bool PlaySmile() => PlayAnimationByName("smile", loop: false, returnToIdle: true);

    /// <summary>
    /// 播放思考动画
    /// </summary>
    public bool PlayThink() => PlayAnimationByName("think", loop: false, returnToIdle: true);

    /// <summary>
    /// 播放动作动画
    /// </summary>
    public bool PlayAction() => PlayAnimationByName("action", loop: false, returnToIdle: true);

    /// <summary>
    /// 播放表情动画
    /// </summary>
    public bool PlayExpression0() => PlayAnimationByName("expression_0", loop: false, returnToIdle: true);

    /// <summary>
    /// 播放否定动画
    /// </summary>
    public bool PlayNo() => PlayAnimationByName("no", loop: false, returnToIdle: true);

    /// <summary>
    /// 播放特殊动画
    /// </summary>
    public bool PlaySpecial() => PlayAnimationByName("special", loop: false, returnToIdle: true);

    /// <summary>
    /// 播放开始说话动画
    /// </summary>
    public bool PlayTalkStart() => PlayAnimationByName("talk_start", loop: false, returnToIdle: true);

    /// <summary>
    /// 播放结束说话动画
    /// </summary>
    public bool PlayTalkEnd() => PlayAnimationByName("talk_end", loop: false, returnToIdle: true);

    #endregion

    #region 通用命令接口

    /// <summary>
    /// 通过命令字符串播放动画（支持别名映射）
    /// </summary>
    /// <param name="command">动画命令名称</param>
    /// <returns>是否成功播放</returns>
    public bool PlayCommand(string command)
    {
        if (string.IsNullOrEmpty(command))
        {
            Debug.LogWarning($"[{nameof(SpineAnimationManager)}] PlayCommand: command is null or empty");
            return false;
        }

        // 命令别名映射
        switch (command.ToLowerInvariant())
        {
            case "idle":
                PlayIdle();
                return true;
            case "smile":
                return PlaySmile();
            case "think":
                return PlayThink();
            case "action":
                return PlayAction();
            case "expression":
            case "expression_0":
                return PlayExpression0();
            case "no":
                return PlayNo();
            case "special":
                return PlaySpecial();
            case "talk_start":
                return PlayTalkStart();
            case "talk_end":
                return PlayTalkEnd();
            default:
                // 尝试直接作为动画名称播放
                return PlayAnimationByName(command, loop: false, returnToIdle: true);
        }
    }

    /// <summary>
    /// 通过动画名称播放动画
    /// </summary>
    /// <param name="animationName">动画名称</param>
    /// <param name="loop">是否循环播放</param>
    /// <param name="returnToIdle">播放完成后是否回到 idle</param>
    /// <returns>是否成功播放</returns>
    public bool PlayAnimationByName(string animationName, bool loop, bool returnToIdle)
    {
        if (skeletonAnimation == null || animationState == null)
        {
            Debug.LogError($"[{nameof(SpineAnimationManager)}] Component not initialized!");
            return false;
        }

        if (string.IsNullOrEmpty(animationName))
        {
            Debug.LogWarning($"[{nameof(SpineAnimationManager)}] Animation name is null or empty");
            return false;
        }

        // 查找动画
        var data = skeletonAnimation.Skeleton.Data;
        var animation = data.FindAnimation(animationName);
        if (animation == null)
        {
            Debug.LogWarning($"[{nameof(SpineAnimationManager)}] Animation '{animationName}' not found in skeleton data");
            return false;
        }

        // 如果当前正在播放同一个动画且是循环的，则不重复播放
        if (currentAnimationName == animationName && loop && activeEntry != null && !activeEntry.IsComplete)
        {
            if (logAnimationChanges)
                Debug.Log($"[{nameof(SpineAnimationManager)}] Animation '{animationName}' already playing, skipping");
            return true;
        }

        // 设置动画
        var entry = animationState.SetAnimation(trackIndex, animation, loop);
        
        // 更新当前动画名称
        string previousAnimation = currentAnimationName;
        currentAnimationName = animationName;
        activeEntry = entry;

        if (logAnimationChanges)
            Debug.Log($"[{nameof(SpineAnimationManager)}] Playing '{animationName}' (loop={loop}, returnToIdle={returnToIdle})");

        // 触发开始事件
        OnAnimationStarted?.Invoke(animationName);

        // 设置完成回调
        if (!loop && returnToIdle)
        {
            entry.Complete += OnOneShotAnimationComplete;
        }

        // 设置中断回调
        entry.Interrupt += (TrackEntry interruptedEntry) =>
        {
            if (interruptedEntry == activeEntry)
            {
                OnAnimationInterrupted?.Invoke(animationName);
            }
        };

        return true;
    }

    #endregion

    #region 查询接口

    /// <summary>
    /// 获取所有可用的动画名称
    /// </summary>
    /// <returns>动画名称数组</returns>
    public string[] GetAvailableAnimations()
    {
        if (skeletonAnimation == null || skeletonAnimation.Skeleton == null)
            return Array.Empty<string>();

        var data = skeletonAnimation.Skeleton.Data;
        return data.Animations.Items
            .Where(a => a != null)
            .Select(a => a.Name)
            .ToArray();
    }

    /// <summary>
    /// 检查是否存在指定名称的动画
    /// </summary>
    /// <param name="animationName">动画名称</param>
    /// <returns>是否存在</returns>
    public bool HasAnimation(string animationName)
    {
        if (skeletonAnimation == null || skeletonAnimation.Skeleton == null)
            return false;

        var data = skeletonAnimation.Skeleton.Data;
        return data.FindAnimation(animationName) != null;
    }

    /// <summary>
    /// 获取当前动画的播放进度（0-1）
    /// </summary>
    /// <returns>播放进度</returns>
    public float GetAnimationProgress()
    {
        if (activeEntry == null || activeEntry.IsComplete)
            return 1f;

        return activeEntry.TrackTime / activeEntry.AnimationEnd;
    }

    /// <summary>
    /// 检查当前是否正在播放 idle
    /// </summary>
    public bool IsIdle()
    {
        return currentAnimationName == idleAnimation;
    }

    #endregion

    #region 内部方法

    /// <summary>
    /// 单次动画播放完成时的回调
    /// </summary>
    private void OnOneShotAnimationComplete(TrackEntry trackEntry)
    {
        // 检查是否是当前活动的动画条目（防止被中断后的旧回调触发）
        if (trackEntry != activeEntry)
        {
            if (logAnimationChanges)
                Debug.Log($"[{nameof(SpineAnimationManager)}] Ignoring stale complete event for '{trackEntry.Animation.Name}'");
            return;
        }

        string completedAnimation = currentAnimationName;

        if (logAnimationChanges)
            Debug.Log($"[{nameof(SpineAnimationManager)}] Animation '{completedAnimation}' completed, returning to idle");

        // 触发完成事件
        OnAnimationCompleted?.Invoke(completedAnimation);

        // 移除回调（避免重复触发）
        trackEntry.Complete -= OnOneShotAnimationComplete;

        // 回到 idle
        PlayIdle();
    }

    #endregion

    #region Agent 命令执行接口

    /// <summary>
    /// 执行来自 Agent 层的命令（统一入口）
    /// </summary>
    /// <param name="command">命令名称</param>
    /// <returns>是否成功执行</returns>
    public bool ExecuteCommand(string command)
    {
        return PlayCommand(command);
    }

    /// <summary>
    /// 执行来自 Agent 层的命令（带参数版本）
    /// </summary>
    /// <param name="command">命令名称</param>
    /// <param name="argument">命令参数（预留）</param>
    /// <returns>是否成功执行</returns>
    public bool ExecuteCommand(string command, string argument)
    {
        // 当前版本忽略 argument，未来可扩展
        return PlayCommand(command);
    }

    #endregion

    #region 调试和验证

    /// <summary>
    /// 验证所有预定义的动画是否存在
    /// </summary>
    /// <returns>缺失的动画列表</returns>
    public List<string> ValidatePredefinedAnimations()
    {
        var predefined = new[] { "idle", "smile", "think", "action", "expression_0", "no", "special", "talk_start", "talk_end" };
        var missing = new List<string>();

        foreach (var animName in predefined)
        {
            if (!HasAnimation(animName))
            {
                missing.Add(animName);
            }
        }

        if (missing.Count > 0)
        {
            Debug.LogWarning($"[{nameof(SpineAnimationManager)}] Missing predefined animations: {string.Join(", ", missing)}");
        }
        else
        {
            Debug.Log($"[{nameof(SpineAnimationManager)}] All predefined animations found");
        }

        return missing;
    }

    #endregion
}