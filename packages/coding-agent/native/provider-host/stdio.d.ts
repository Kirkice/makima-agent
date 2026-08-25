import type { ProviderHost } from "./index.ts";
/** Provider Host 进程边界使用的最小字节输入接口。 */
export interface ProviderHostByteInput {
    on(event: "data", listener: (chunk: Uint8Array) => void): unknown;
    on(event: "end", listener: () => void): unknown;
    on(event: "error", listener: (error: Error) => void): unknown;
}
/** 将已构帧的响应写入 Rust Core 的最小字节输出接口。 */
export interface ProviderHostByteOutput {
    write(chunk: Uint8Array, callback: (error?: Error | null) => void): boolean;
}
/**
 * 把一个 ProviderHost 绑定为 framed CBOR stdio 服务。
 *
 * 输入 request 后异步消费 Host 流；abort 不等待流完成，因此可以在同一连接中取消正在运行
 * 的请求。每个流无论成功、失败或取消都只写入一个 complete，供 Core 清理 request 状态。
 */
export declare class ProviderHostStdioServer {
    private readonly decoder;
    private readonly activeTasks;
    private writeTail;
    private closing;
    private ended;
    private readonly host;
    private readonly output;
    constructor(host: ProviderHost, output: ProviderHostByteOutput);
    attach(input: ProviderHostByteInput): void;
    receive(chunk: Uint8Array): void;
    /**
     * 关闭输入并等待已排队的 framed-CBOR 输出完成。
     *
     * stdin EOF 是 Rust supervisor 的正常关闭信号。必须先 abort 所有 Provider 请求，再等待
     * 每个执行任务写完它唯一的 complete；否则子进程退出会让 Core 永久保留 active request。
     */
    close(): Promise<void>;
    private closeInternal;
    /** 与旧的同步调用点兼容；进程入口应使用 `close` 等待完整关闭。 */
    end(): void;
    private handle;
    private execute;
    private send;
}
/** 启动独立 Provider Host 子进程的 stdio 服务。 */
export declare function runProviderHostStdio(host: ProviderHost): ProviderHostStdioServer;
//# sourceMappingURL=stdio.d.ts.map