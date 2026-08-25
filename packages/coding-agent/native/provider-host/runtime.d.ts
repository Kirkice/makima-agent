/**
 * 启动 Provider Host 的标准输入输出服务。
 *
 * 该函数是 Node sidecar 与 Bun 编译二进制之间唯一共享的启动边界。两者均将 Provider SDK
 * 放在独立子进程中：Rust Core 只通过 framed-CBOR 与它通信，因此不能把模型注册、信号处理
 * 或 stdout 生命周期复制到两个启动器中。
 */
export declare function startProviderHost(): void;
//# sourceMappingURL=runtime.d.ts.map