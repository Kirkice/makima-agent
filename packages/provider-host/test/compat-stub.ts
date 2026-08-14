/**
 * Provider Host 单元测试不会发起真实网络请求。
 * 该 stub 仅满足默认依赖的模块解析；所有测试均注入 ProviderStreamFactory。
 */
export function streamSimple(): never {
	throw new Error("Tests must inject ProviderHostOptions.stream");
}
