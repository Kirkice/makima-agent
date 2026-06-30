using UnityEngine;
using System.Collections.Generic;

/// <summary>
/// 测试脚本 - 演示 SpineAnimationManager 和 AgentAnimationBridge 的使用方法
/// 提供键盘快捷键测试所有动画
/// </summary>
public class SpineAnimationTest : MonoBehaviour
{
    [Header("组件引用")]
    [SerializeField] private SpineAnimationManager animationManager;
    [SerializeField] private AgentAnimationBridge animationBridge;

    [Header("快捷键配置")]
    [SerializeField] private KeyCode idleKey = KeyCode.Alpha0;
    [SerializeField] private KeyCode smileKey = KeyCode.Alpha1;
    [SerializeField] private KeyCode thinkKey = KeyCode.Alpha2;
    [SerializeField] private KeyCode actionKey = KeyCode.Alpha3;
    [SerializeField] private KeyCode expressionKey = KeyCode.Alpha4;
    [SerializeField] private KeyCode noKey = KeyCode.Alpha5;
    [SerializeField] private KeyCode specialKey = KeyCode.Alpha6;
    [SerializeField] private KeyCode talkStartKey = KeyCode.Alpha7;
    [SerializeField] private KeyCode talkEndKey = KeyCode.Alpha8;

    [Header("JSON 测试")]
    [SerializeField] private KeyCode jsonTestKey = KeyCode.J;
    [SerializeField] private string jsonTestCommand = "{\"type\":\"play_animation\",\"name\":\"smile\"}";

    private void Start()
    {
        // 自动获取组件
        if (animationManager == null)
        {
            animationManager = GetComponent<SpineAnimationManager>();
        }

        if (animationBridge == null)
        {
            animationBridge = GetComponent<AgentAnimationBridge>();
        }

        if (animationManager == null)
        {
            Debug.LogError($"[{nameof(SpineAnimationTest)}] SpineAnimationManager not found!");
            return;
        }

        // 验证动画
        var missing = animationManager.ValidatePredefinedAnimations();
        if (missing.Count > 0)
        {
            Debug.LogWarning($"Missing animations: {string.Join(", ", missing)}");
        }

        // 列出所有可用动画
        var available = animationManager.GetAvailableAnimations();
        Debug.Log($"Available animations ({available.Length}): {string.Join(", ", available)}");

        // 订阅事件
        animationManager.OnAnimationStarted += OnAnimationStarted;
        animationManager.OnAnimationCompleted += OnAnimationCompleted;
        animationManager.OnAnimationInterrupted += OnAnimationInterrupted;

        Debug.Log($"[{nameof(SpineAnimationTest)}] Test script initialized, use number keys 0-8 to test animations");
    }

    private void Update()
    {
        if (animationManager == null) return;

        // 键盘测试
        if (Input.GetKeyDown(idleKey)) TestPlayIdle();
        if (Input.GetKeyDown(smileKey)) TestPlaySmile();
        if (Input.GetKeyDown(thinkKey)) TestPlayThink();
        if (Input.GetKeyDown(actionKey)) TestPlayAction();
        if (Input.GetKeyDown(expressionKey)) TestPlayExpression0();
        if (Input.GetKeyDown(noKey)) TestPlayNo();
        if (Input.GetKeyDown(specialKey)) TestPlaySpecial();
        if (Input.GetKeyDown(talkStartKey)) TestPlayTalkStart();
        if (Input.GetKeyDown(talkEndKey)) TestPlayTalkEnd();

        // JSON 命令测试
        if (Input.GetKeyDown(jsonTestKey) && animationBridge != null)
        {
            TestJsonCommand();
        }

        // 按 L 列出动画
        if (Input.GetKeyDown(KeyCode.L))
        {
            ListAnimations();
        }

        // 按 I 显示当前状态
        if (Input.GetKeyDown(KeyCode.I))
        {
            ShowCurrentState();
        }
    }

    #region 测试方法

    private void TestPlayIdle()
    {
        Debug.Log("[TEST] PlayIdle");
        animationManager.PlayIdle();
    }

    private void TestPlaySmile()
    {
        Debug.Log("[TEST] PlaySmile");
        bool success = animationManager.PlaySmile();
        Debug.Log($"PlaySmile result: {success}");
    }

    private void TestPlayThink()
    {
        Debug.Log("[TEST] PlayThink");
        bool success = animationManager.PlayThink();
        Debug.Log($"PlayThink result: {success}");
    }

    private void TestPlayAction()
    {
        Debug.Log("[TEST] PlayAction");
        bool success = animationManager.PlayAction();
        Debug.Log($"PlayAction result: {success}");
    }

    private void TestPlayExpression0()
    {
        Debug.Log("[TEST] PlayExpression0");
        bool success = animationManager.PlayExpression0();
        Debug.Log($"PlayExpression0 result: {success}");
    }

    private void TestPlayNo()
    {
        Debug.Log("[TEST] PlayNo");
        bool success = animationManager.PlayNo();
        Debug.Log($"PlayNo result: {success}");
    }

    private void TestPlaySpecial()
    {
        Debug.Log("[TEST] PlaySpecial");
        bool success = animationManager.PlaySpecial();
        Debug.Log($"PlaySpecial result: {success}");
    }

    private void TestPlayTalkStart()
    {
        Debug.Log("[TEST] PlayTalkStart");
        bool success = animationManager.PlayTalkStart();
        Debug.Log($"PlayTalkStart result: {success}");
    }

    private void TestPlayTalkEnd()
    {
        Debug.Log("[TEST] PlayTalkEnd");
        bool success = animationManager.PlayTalkEnd();
        Debug.Log($"PlayTalkEnd result: {success}");
    }

    private void TestJsonCommand()
    {
        Debug.Log($"[TEST] ExecuteJsonCommand: {jsonTestCommand}");
        bool success = animationBridge.ExecuteJsonCommand(jsonTestCommand);
        Debug.Log($"ExecuteJsonCommand result: {success}");
    }

    private void ListAnimations()
    {
        Debug.Log("[TEST] Listing all animations:");
        var animations = animationManager.GetAvailableAnimations();
        foreach (var anim in animations)
        {
            Debug.Log($"  - {anim}");
        }
    }

    private void ShowCurrentState()
    {
        Debug.Log("[TEST] Current state:");
        Debug.Log($"  Current animation: {animationManager.CurrentAnimationName}");
        Debug.Log($"  Is idle: {animationManager.IsIdle()}");
        Debug.Log($"  Progress: {animationManager.GetAnimationProgress():P0}");
    }

    #endregion

    #region 事件处理

    private void OnAnimationStarted(string animationName)
    {
        Debug.Log($"[EVENT] Animation started: {animationName}");
    }

    private void OnAnimationCompleted(string animationName)
    {
        Debug.Log($"[EVENT] Animation completed: {animationName}");
    }

    private void OnAnimationInterrupted(string animationName)
    {
        Debug.Log($"[EVENT] Animation interrupted: {animationName}");
    }

    #endregion

    private void OnDestroy()
    {
        // 取消订阅事件
        if (animationManager != null)
        {
            animationManager.OnAnimationStarted -= OnAnimationStarted;
            animationManager.OnAnimationCompleted -= OnAnimationCompleted;
            animationManager.OnAnimationInterrupted -= OnAnimationInterrupted;
        }
    }
}