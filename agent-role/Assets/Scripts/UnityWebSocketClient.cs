using UnityEngine;
using System;
using System.Collections.Generic;
using NativeWebSocket;

/// <summary>
/// Unity WebSocket client that connects to the Rust desktop app's WebSocket bridge.
/// Receives animation commands and forwards them to AgentAnimationBridge.
/// </summary>
public class UnityWebSocketClient : MonoBehaviour
{
    [Header("WebSocket Settings")]
    [Tooltip("WebSocket server URL")]
    public string serverUrl = "ws://127.0.0.1:9001";

    [Tooltip("Auto connect on start")]
    public bool autoConnect = true;

    [Tooltip("Reconnect on disconnect")]
    public bool autoReconnect = true;

    [Tooltip("Reconnect delay in seconds")]
    public float reconnectDelay = 3f;

    [Header("References")]
    [Tooltip("AgentAnimationBridge to send animation commands to")]
    public AgentAnimationBridge animationBridge;

    private WebSocket websocket;
    private bool isConnecting = false;
    private float reconnectTimer = 0f;

    private void Start()
    {
        // Auto-find AgentAnimationBridge if not assigned
        if (animationBridge == null)
        {
            animationBridge = FindObjectOfType<AgentAnimationBridge>();
            if (animationBridge == null)
            {
                Debug.LogError("[UnityWebSocketClient] AgentAnimationBridge not found in scene!");
                return;
            }
            Debug.Log("[UnityWebSocketClient] Auto-found AgentAnimationBridge");
        }

        if (autoConnect)
        {
            Connect();
        }
    }

    private void Update()
    {
        // Dispatch WebSocket messages
#if !UNITY_WEBGL || UNITY_EDITOR
        if (websocket != null)
        {
            websocket.DispatchMessageQueue();
        }
#endif

        // Handle reconnection
        if (autoReconnect && !isConnecting && (websocket == null || websocket.State != WebSocketState.Open))
        {
            reconnectTimer += Time.deltaTime;
            if (reconnectTimer >= reconnectDelay)
            {
                reconnectTimer = 0f;
                Connect();
            }
        }
    }

    public async void Connect()
    {
        if (isConnecting) return;
        isConnecting = true;

        Debug.Log($"[UnityWebSocketClient] Connecting to {serverUrl}...");

        websocket = new WebSocket(serverUrl);

        websocket.OnOpen += () =>
        {
            Debug.Log("[UnityWebSocketClient] Connected to WebSocket server");
            isConnecting = false;
        };

        websocket.OnError += (e) =>
        {
            Debug.LogError($"[UnityWebSocketClient] WebSocket error: {e}");
            isConnecting = false;
        };

        websocket.OnClose += (e) =>
        {
            Debug.LogWarning("[UnityWebSocketClient] WebSocket closed");
            isConnecting = false;
        };

        websocket.OnMessage += (bytes) =>
        {
            var message = System.Text.Encoding.UTF8.GetString(bytes);
            HandleMessage(message);
        };

        try
        {
            await websocket.Connect();
        }
        catch (Exception e)
        {
            Debug.LogError($"[UnityWebSocketClient] Connection failed: {e.Message}");
            isConnecting = false;
        }
    }

    private void HandleMessage(string message)
    {
        Debug.Log($"[UnityWebSocketClient] Received: {message}");

        // Parse JSON: {"animation": "smile"}
        try
        {
            // Simple JSON parsing (avoid Newtonsoft.Json dependency)
            var animation = ParseAnimationFromJson(message);
            
            if (!string.IsNullOrEmpty(animation))
            {
                Debug.Log($"[UnityWebSocketClient] Parsed animation: {animation}");
                animationBridge?.Play(animation);
            }
            else
            {
                Debug.LogWarning("[UnityWebSocketClient] Failed to parse animation from message");
            }
        }
        catch (Exception e)
        {
            Debug.LogError($"[UnityWebSocketClient] Error handling message: {e.Message}");
        }
    }

    /// <summary>
    /// Simple JSON parser for {"animation": "xxx"} format
    /// </summary>
    private string ParseAnimationFromJson(string json)
    {
        // Find "animation" key
        int keyIndex = json.IndexOf("\"animation\"");
        if (keyIndex < 0) return null;

        // Find the colon after "animation"
        int colonIndex = json.IndexOf(':', keyIndex);
        if (colonIndex < 0) return null;

        // Find the opening quote of the value
        int startQuote = json.IndexOf('"', colonIndex + 1);
        if (startQuote < 0) return null;

        // Find the closing quote
        int endQuote = json.IndexOf('"', startQuote + 1);
        if (endQuote < 0) return null;

        return json.Substring(startQuote + 1, endQuote - startQuote - 1);
    }

    private void OnDestroy()
    {
        if (websocket != null && websocket.State == WebSocketState.Open)
        {
            websocket.Close();
        }
    }

    private void OnApplicationQuit()
    {
        if (websocket != null && websocket.State == WebSocketState.Open)
        {
            websocket.Close();
        }
    }
}