import type { Api, AssistantMessageEventStream, Context, Model, SimpleStreamOptions } from "@earendil-works/pi-ai";
import { type ModelRef, type ProviderRequest, type ProviderStreamEvent } from "@earendil-works/pi-protocol";
/** 根据 Rust Core 传来的稳定模型引用定位 Host 私有的完整模型配置。 */
export interface ProviderModelResolver {
    resolve(model: ModelRef): Model<Api> | Promise<Model<Api>>;
}
/**
 * 将已完成的共享 transcript 映射为 Provider SDK 的上下文。
 *
 * 协议中不存在的 SDK 专用字段（provider 原始响应、签名与诊断）不可被伪造；它们在重放时
 * 自然缺失，Provider adapter 仍应按共享的文本、thinking、工具调用与工具结果工作。
 */
export interface ProviderContextConverter {
    toContext(request: ProviderRequest): Context;
}
/**
 * Provider SDK 流端口。
 *
 * 核心 Host 不导入具体 Provider catalog；生产入口负责注入 `streamSimple`，测试和嵌入场景可注入
 * 确定性实现。这样仅加载协议转换与 stdio 边界时，不会触发模型数据或 Provider 注册副作用。
 */
export type ProviderStreamFactory = (model: Model<Api>, context: Context, options: SimpleStreamOptions) => AssistantMessageEventStream;
export interface ProviderHostOptions {
    modelResolver: ProviderModelResolver;
    contextConverter?: ProviderContextConverter;
    stream: ProviderStreamFactory;
    requestOptions?: Omit<SimpleStreamOptions, "signal">;
    defaultTimeoutMs?: number;
    now?: () => number;
}
/**
 * 真实 Provider SDK 的异步执行边界。
 *
 * 每个 requestId 绑定一个独立 AbortController。取消会立即转发给 pi-ai Provider；无论是 SDK
 * 主动发送 aborted 事件、抛出异常或超时，Host 都只向 Rust Core 输出稳定的 `error` DTO。
 */
export declare class ProviderHost {
    private readonly modelResolver;
    private readonly contextConverter;
    private readonly stream;
    private readonly requestOptions;
    private readonly defaultTimeoutMs;
    private readonly now;
    private readonly active;
    constructor(options: ProviderHostOptions);
    /** 执行并投影一次请求；同一 requestId 同时只能存在一个活动流。 */
    execute(input: unknown): AsyncGenerator<ProviderStreamEvent>;
    /** 中止指定请求；不存在的 requestId 返回 false，调用方可安全重试取消。 */
    abort(requestId: string): boolean;
    /**
     * 在 stdio EOF 或进程信号到达时取消全部活动请求。
     *
     * 这是关闭边界而非错误恢复：Provider 流仍由各自的 `execute` finally 清理，并由
     * stdio 服务写入唯一的 complete，避免 Rust Core 永远保留活动 request ID。
     */
    abortAll(): void;
    get activeRequestCount(): number;
    private scheduleTimeout;
    private error;
}
export declare const defaultProviderContextConverter: ProviderContextConverter;
//# sourceMappingURL=index.d.ts.map