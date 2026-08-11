# Work 页面与按钮体系一致性设计

> 日期：2026-08-11
>
> 状态：视觉方向已确认，等待书面规格复核
>
> 范围：统一布局判断与按钮语义；先迁移 WorkView，再按审计清单渐进迁移其他页面

## 1. 问题

当前 App 使用整个窗口的宽高关系判断紧凑布局，WorkView 又使用独立的 `820px` CSS 断点。窗口为纵向桌面形态时，App 已显示底部导航，但 WorkView 仍保持左右两栏，形成同一画面中的两套布局模式。

WorkView 还使用 `1280px` 内容宽度和独立 `primary-button`。项目中的全局 `.btn-primary`、Work `primary-button`、GlobalDialog、ConfirmCard、Goal 选择按钮对“主要动作”使用了不同的颜色、边框和高度，Accent Color 同时承担提交、选中和运行状态，语义不稳定。

## 2. 设计目标

- Work 页面采用与其他主页面一致的单内容流，不在页面内部再建立左右导航栏；
- 整个应用只有一套终端与窗口形态判断，页面不得私设布局断点；
- 终端类型决定交互方式，窗口形态决定内容排列；
- Accent Color 只突出当前选择、活动状态和进度，不作为普通主要提交按钮的默认填充；
- 建立一套可渐进迁移的按钮语义，避免一次性重写全部成熟页面。

## 3. 统一布局状态

App 级布局状态拆成两个正交维度：

```text
terminalKind: native-mobile | desktop
viewportShape: portrait | landscape

derived layoutMode:
  native-mobile                 -> mobile-native
  desktop + portrait            -> desktop-compact
  desktop + landscape           -> desktop-wide
```

- `terminalKind` 使用现有原生移动端识别结果；Android 横屏仍是 `mobile-native`，不能因为宽度变大而展示桌面交互。
- `viewportShape` 继续使用整个可用窗口 `innerHeight > innerWidth`，不使用 WorkView 自己的像素阈值。
- App 统一 provide `terminalKind`、`viewportShape` 与 `layoutMode`；页面只消费状态。
- `isMobile` 兼容值在迁移期继续存在，语义为“当前使用紧凑导航”，不再代表物理终端。
- CSS media query 只负责极端尺寸下的排版保护，不得决定产品模式或显示不同业务入口。

## 4. WorkView 单内容流

### 4.1 页面骨架

```text
滚动容器（负责外侧 padding）
└─ 1000px 居中内容容器
   ├─ 页面标题与主要动作
   ├─ 待归属 / Change Review（存在时）
   ├─ 项目切换区（存在项目时）
   └─ 当前项目内容或唯一空状态
```

- 删除 `240px + main` 的内部左右分栏；
- Work 主内容宽度从 `1280px` 收敛为 `1000px`；
- Padding 只放在滚动容器，受限内容容器不带左右 Padding；
- 没有项目时只显示一个空状态，不再同时显示“空项目栏”和“空主区”；
- 页面标题文案保持现有 i18n，不新增解释性文字。

### 4.2 项目切换

- 项目切换区已经位于 Work section 内，不显示冗余的“项目”标签；
- 桌面横向与桌面纵向窗口都使用顶部横向项目选项；内容过长时允许横向滚动；
- 每个选项结构完全一致：状态圆点 + 项目名称；
- 当前项目使用 Accent 实心圆点和 Accent 浅色选中背景；
- 未选项目使用未激活语义色的空心圆点，不使用重复且无区分价值的文件夹图标；
- 原生手机使用紧凑项目选择控件，避免长项目名占据多行；选择结果仍使用同一 `activeProjectId`，不形成第二套状态。

## 5. 按钮语义契约

| 类型 | 视觉 | 使用范围 |
|---|---|---|
| Primary | 中性高对比实心，32px | 每个动作组唯一主要提交，例如新建、保存、确认 |
| Secondary | 中性描边或透明，32px | 取消、导出、刷新、返回 |
| Compact | 复用 Primary/Secondary，28px | 卡片内小型动作，不建立新配色 |
| Selected | Accent 浅色背景/边框/文字 | 当前项目、模式、选项与运行状态；不是提交按钮 |
| Danger | 红色描边；最终不可逆确认才允许红色实心 | 删除、解绑、重置 |
| Icon | 34 × 34px 无底色；Hover 使用中性表面 | 关闭、更多、复制等工具动作 |

约束：

- Primary 不直接使用 `--user-accent`；采用全局 `.btn-primary` 的中性高对比逻辑；
- Accent 不能同时表示“可点击”“当前选中”“成功”和“主要提交”；
- 所有类型使用现有 CSS 变量，亮暗主题同时成立；
- Lucide 图标统一为 14–16px，并与文字 Flex 垂直居中；
- 禁止组件重新定义同名 `.btn-primary`；局部布局可以组合全局按钮类，但不能重写配色和尺寸；
- Disabled、focus-visible、hover 和 active 状态在全局定义。

## 6. 渐进迁移边界

本轮不是一次性重写所有按钮：

1. 盘点全部按钮定义，生成“现有类 → 目标语义 → 所属文件 → 风险”的迁移清单；
2. 在全局 CSS 固化按钮契约和布局状态契约；
3. 完整迁移 WorkView，包括项目切换、空状态、表单、审批和运行时按钮；
4. 优先迁移重复定义 `.btn-primary` 的共享组件：GlobalDialog、ConfirmCard、BrowserEnableCard；
5. 其他业务页面按功能批次渐进迁移，未迁移组件不得被全局选择器意外改变；
6. 每批迁移后分别验证亮/暗主题、中文/英文、桌面横向、桌面纵向和原生手机。

这样既建立唯一方向，又避免大爆炸式视觉回归。

## 7. 验收标准

- `838 × 1092` Windows 窗口中，底部导航与 WorkView 都使用紧凑单栏结构；
- Windows 横向窗口仍为单内容流，不恢复左右项目栏；
- Android 横屏仍使用移动端交互与安全区；
- Work 空状态只有一处，新建按钮不重复；
- 项目选项不显示“项目”标签，所有项目都有同结构圆点；
- 选中圆点为 Accent 实心，未选圆点为未激活色空心；
- Work 的主要按钮与全局 Primary 一致，不再使用 Accent 实心；
- `WorkView.vue` 不再包含决定产品布局的 `820px` media query；
- 新增纯函数测试覆盖三种 `layoutMode`，前端测试与生产构建通过；
- 不增加 npm、Cargo、Python、MCP 或客户端运行时依赖；
- `design_principles.md`、`LLM_WIKI.md`、`docs/ARCHITECTURE.md`、`todo.md` 与 `progress.yaml` 同步真实状态。

## 8. 非目标

- 不改变 Work Core、Goal Runtime、Calendar 或同步的数据契约；
- 不改变项目创建、选择、审批和 Today 导航的业务行为；
- 不在本轮一次性迁移所有历史组件；
- 不升版本、不发布、不推送，除非用户另行授权。
