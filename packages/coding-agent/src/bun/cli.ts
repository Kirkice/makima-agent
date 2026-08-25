#!/usr/bin/env node
/**
 * Bun binary 的薄入口。
 *
 * runtime selector 必须先于 Bun OAuth、Bedrock 注册和 TypeScript 产品 runtime：native 路径不能
 * 因为 binary 入口的预加载而引入 Provider SDK。`cli.ts` 负责选择并在 ts 路径派生 cli-runtime。
 */
await import("../cli.ts");
