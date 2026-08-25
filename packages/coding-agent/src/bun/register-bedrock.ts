import { bedrockProviderModule } from "@earendil-works/pi-ai/bedrock-provider";
import { setBedrockProviderModule } from "@earendil-works/pi-ai/compat";

/**
 * 注册 Bun 发布物使用的 Bedrock provider 实现。
 *
 * 保持显式函数而非模块求值副作用，使 TypeScript runtime 能在恢复 sandbox 环境和注册 OAuth 后
 * 按确定顺序完成初始化；重复注册同一模块是幂等的。
 */
export function registerBedrockProvider(): void {
	setBedrockProviderModule(bedrockProviderModule);
}
