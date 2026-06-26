# Settings 面板 GUI 排布问题清单

本文记录当前 `apps/desktop-egui` 中 Settings 面板的布局与可用性问题，基于现有实现和界面截图做静态审查，重点关注右侧 `Settings` 停靠面板在桌面端的排布是否合理。

## 范围

- 主容器：[apps/desktop-egui/src/ui/panels/settings.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/settings.rs:8)
- Dock 宽度策略：[apps/desktop-egui/src/ui/dock.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/dock.rs:31)
- Mode 子面板：[apps/desktop-egui/src/ui/panels/modes.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/modes.rs:12)
- 默认状态值：[apps/desktop-egui/src/state/app_state.rs](/H:/Project/makima-agent/apps/desktop-egui/src/state/app_state.rs:197)

## 主要问题

### 1. Settings 面板默认宽度过窄，与内容复杂度不匹配

- Dock 侧栏把 Settings 宽度限制在 `240-380px`，默认宽度为 `280px`。
- `AppState` 和 `AppConfig` 中初始 `inspector_width` 还是 `210px`，虽然后续会被归一化，但这个默认值本身已经说明当前设计把 Settings 当成了“轻量信息栏”。
- 对 `Modes`、`Providers` 这类包含表单、卡片、按钮、描述文本的页面来说，这个宽度明显不够，导致界面横向拥挤、信息被压缩成高密度竖排。

代码位置：

- [apps/desktop-egui/src/ui/dock.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/dock.rs:31)
- [apps/desktop-egui/src/state/app_state.rs](/H:/Project/makima-agent/apps/desktop-egui/src/state/app_state.rs:198)

影响：

- 信息阅读成本高。
- 操作区和内容区抢空间。
- 后续增加字段时很容易出现截断、换行过多或卡片比例失衡。

### 2. Settings 内容区和子面板列表存在双层纵向滚动

- `settings.rs` 在右侧内容区外层包了一层 `ScrollArea::vertical()`。
- `modes.rs` 又在模式列表内部包了一层 `ScrollArea::vertical()`。
- 这会导致鼠标滚轮、触控板手势以及滚动条交互出现“外层滚一段、内层再滚一段”的体验。

代码位置：

- [apps/desktop-egui/src/ui/panels/settings.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/settings.rs:52)
- [apps/desktop-egui/src/ui/panels/modes.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/modes.rs:62)

影响：

- 滚动行为不直观。
- 用户难以判断当前在滚哪个区域。
- 面板内容一多时会让右侧区域显得更“卡”和更拥挤。

### 3. Settings 头部和子面板头部重复，占用了窄栏宝贵的垂直空间

- `Settings` 主面板先显示了一次 `Settings` 标题和副标题。
- 进入 `Modes` 页后，子面板又显示 `Mode Management`、激活态条和工具栏。
- 在右侧窄栏场景下，这类多层头部会明显推高首屏占用，让真正可操作内容更晚出现。

代码位置：

- [apps/desktop-egui/src/ui/panels/settings.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/settings.rs:12)
- [apps/desktop-egui/src/ui/panels/modes.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/modes.rs:12)

影响：

- 首屏有效内容偏少。
- 视觉层级显得啰嗦。
- 在窗口高度有限时，用户要更频繁地滚动才能看到列表主体。

### 4. 左侧导航使用手绘文本布局，适配性差

- 目前 tab 列表不是标准按钮或可自适应控件，而是 `allocate_exact_size + painter.text` 手动绘制。
- 文本和 emoji 的位置通过固定偏移量摆放。
- 这种方式在不同字体、DPI、语言长度下容易出现对齐误差，扩展成本也高。

代码位置：

- [apps/desktop-egui/src/ui/panels/settings.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/settings.rs:67)

影响：

- 本地化或改文案后容易错位。
- 后续如果要加数字徽标、状态点、禁用态会比较难做。
- 控件语义较弱，不利于统一交互反馈。

### 5. 左侧导航宽度固定且偏窄，长标签扩展空间不足

- `TAB_WIDTH` 固定为 `120.0`。
- 当前标签虽然大多较短，但像 `Diagnostics`、未来可能新增的更长中文标签，都已经接近上限。
- 当前实现没有省略策略，也没有根据面板整体宽度进行更灵活的响应。

代码位置：

- [apps/desktop-egui/src/ui/panels/settings.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/settings.rs:7)

影响：

- 标签长度一旦增长就容易拥挤。
- 图标与文本的间距余量偏小。
- 左侧导航会显得“像能用，但不宽松”。

### 6. Modes 卡片的信息密度过高，层级没有拉开

- 单张模式卡片同时展示了名称、slug、source、temperature、max_steps、tools、role preview。
- 右上角还叠加了 `Select` / 删除 / `Active` 状态操作。
- 在窄侧栏中，这种“单卡承载过多信息”的布局会让每张卡片都显得很重。

代码位置：

- [apps/desktop-egui/src/ui/panels/modes.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/modes.rs:95)

影响：

- 扫读效率低。
- 用户很难快速分清“主信息”和“次要元信息”。
- 卡片之间虽然有留白，但因为信息块都很满，整体仍然显得压迫。

### 7. Modes 卡片横向布局对窄面板不友好

- 卡片采用“左侧文本堆叠 + 右侧垂直按钮”的横向结构。
- 当容器变窄时，左侧文本会挤压右侧操作区，或者文本区出现过多换行。
- 当前没有针对窄宽度做操作区下沉、按钮折叠、信息分组等响应式处理。

代码位置：

- [apps/desktop-egui/src/ui/panels/modes.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/modes.rs:85)

影响：

- 容器宽度稍小就容易产生拥挤感。
- 操作按钮与内容描述之间缺少稳定边界。
- 卡片视觉重心偏左，右侧按钮像是“硬塞进去”的。

### 8. Toolbar 放在水平滚动区里，但按钮数量并不多，属于过度设计

- `Modes` 工具栏被包在 `ScrollArea::horizontal()` 里。
- 当前只有 `Reload from Config`、`New Mode`、`Refresh List` 三个按钮，本身完全可以自然换行或直接平铺。
- 在右侧小面板里使用横向滚动工具栏，会让布局语义显得奇怪，也增加未来交互复杂度。

代码位置：

- [apps/desktop-egui/src/ui/panels/modes.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/modes.rs:45)

影响：

- 用户会误以为工具栏还隐藏了更多内容。
- 增加了一个不必要的滚动容器。
- 与整体简洁面板风格不一致。

### 9. “激活模式”提示条视觉很重，但信息增量有限

- 当前激活态条使用整块红底样式，占用了一整行宽度。
- 其中展示的信息主要是当前模式名、温度、步数、工具数量。
- 这块区域的视觉权重很高，但与下方列表卡片中的信息重复度也较高。

代码位置：

- [apps/desktop-egui/src/ui/panels/modes.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/modes.rs:20)

影响：

- 强调过度。
- 压缩首屏列表空间。
- 与列表中的“Active”状态形成重复表达。

### 10. 当前 Settings 面板更像“复杂配置页”，但被放进了“Inspector 宽度”的容器

- 从功能上看，`Providers`、`Modes`、`Persona` 这些页已经不只是查看上下文，而是完整配置工作区。
- 从布局上看，它们仍然被放在一个偏窄的右侧停靠栏里。
- 这是结构层面的定位冲突：容器按“辅助侧栏”设计，内容按“主配置页”设计。

代码位置：

- [apps/desktop-egui/src/ui/dock.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/dock.rs:45)
- [apps/desktop-egui/src/ui/panels/settings.rs](/H:/Project/makima-agent/apps/desktop-egui/src/ui/panels/settings.rs:8)

影响：

- 不管怎么微调间距，都会持续感觉“内容太重、容器太轻”。
- 某些子页天然更适合更宽的主工作区或独立弹窗，而不是固定放在侧栏。

## 结论

当前 Settings 面板最核心的问题不是单个控件样式，而是三件事叠加：

1. 容器宽度偏窄。
2. 内容层级过重。
3. 滚动和信息组织方式没有针对右侧窄栏做专门优化。

因此现状会让面板“功能上可用，但视觉上拥挤、阅读上费劲、扩展上风险较高”。

## 后续改版建议方向

- 提高 Settings 停靠面板的最小宽度和默认宽度。
- 去掉双层纵向滚动，只保留单一主滚动区。
- 让 `Modes` 卡片改为更轻的两段式信息结构。
- 把左侧导航改成标准按钮或可复用的 tab 组件。
- 重新判断哪些设置页适合留在侧栏，哪些应升级为更宽的主页面或弹窗。
