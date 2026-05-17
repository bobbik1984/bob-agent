# Bob-Agent 错题集：Electron → Tauri (Rust) 迁移纠错报告

> **用途**：后续开发的防坑指南。每一条都是真实踩过的坑，附带根因分析和防御规则。
> **维护方式**：遇到新 Bug 时，按模板追加到对应分类下。
> **创建日期**：2026-05-15

---

## 一、Rust UTF-8 字符串陷阱 ☠️

> **核心认知**：JavaScript 的字符串是 UTF-16，可以随意按索引切片。Rust 的 `String` 是 UTF-8，中文字符占 3 字节，Emoji 占 4 字节。**按字节索引切片如果落在字符中间，进程直接 panic 崩溃。**

### Bug #1: `<think>` 标签缓冲区切片中文 panic

- **症状**：用户用中文提问，Tauri 进程直接崩溃退出，终端报 `byte index 2 is not a char boundary; it is inside '好' (bytes 0..3)`
- **代码位置**：`llm.rs` 第 542 行
- **根因**：`<think>` 标签状态机保留 7 字节的尾部缓冲（防止标签被截断），计算 `safe_len = buffer.len() - 7`，然后用 `&buffer[..safe_len]` 切片。当 buffer 内容是 `好问题`（9 字节），`safe_len = 2`，而 `好` 占 bytes 0-2，byte 2 不是字符边界 → panic。
- **修复**：加入 `is_char_boundary()` 回退循环：
  ```rust
  while safe_len > 0 && !buffer.is_char_boundary(safe_len) {
      safe_len -= 1;
  }
  ```

### Bug #2: 工具结果截断切片 panic（潜伏）

- **症状**：未实际触发，但代码审查发现
- **代码位置**：`llm.rs` 工具结果截断 `&result_str[..8000]`
- **根因**：与 #1 完全相同。如果第 8000 字节恰好在中文字符中间，就会 panic。
- **修复**：同样加入 `is_char_boundary()` 回退。

### Bug #3: T-505 历史修复 — 13 处裸 unwrap

- **症状**：早期开发中，`lib.rs` 中 13 处 `.unwrap()` 在遇到非预期输入时导致整个 Tauri 进程崩溃。
- **根因**：从 JavaScript 翻译过来时，AI 直接把 `JSON.parse()` 翻译成了 `.unwrap()`，没有考虑 Rust 中 unwrap 失败 = 进程死亡。
- **修复**：全部改为 `match` 或 `.unwrap_or_default()`。

> **铁律 #1**：在 Rust 中，**任何涉及字符串按位置切片的代码**（`&s[..n]`、`&s[n..]`），都必须先检查 `s.is_char_boundary(n)`。或者使用 `.chars().take(n)` 按字符操作。这条规则没有例外。

> **铁律 #2**：Tauri Command 和 async 任务中**绝对禁止** `.unwrap()` 和 `panic!()`。一次 panic = 整个桌面应用崩溃退出。必须用 `Result<T, String>` + `match` 优雅降级。

---

## 二、UI与交互异常 🎨

### Bug #16: 局部样式强覆盖全局变量导致对比度灾难

- **症状**：拖拽文件夹时，“收藏到知识库”按钮在暗色主题下变成浅灰背景配白色文字，几乎不可见。
- **根因**：全局 `index.css` 已定义了完善的高对比度 `.btn-primary`（随暗色/亮色自动适配）。但 `FolderDropCard.vue` 内部写了 `scoped` 样式，强行用极浅灰色的 `--accent-primary` 覆写了按钮背景。
- **修复**：删除组件内多余的局部 `.btn-primary` CSS 代码，使其自然降级继承全局样式。
- **教训**：**设计系统必须维持单一事实来源（SSOT）**。组件应当复用全局类名，绝对禁止在组件内随意重定义品牌色或基础组件样式。

> **核心认知**：OpenAI 的 API 是"事实标准"，但每家厂商的"兼容"都有微妙差异。不能假设一套代码跑通所有供应商。

### Bug #4: MiniMax API 路由到错误域名

- **症状**：MiniMax 模型返回 401 Unauthorized
- **根因**：MiniMax 有两个域名：`api.minimax.io`（国际版）和 `api.minimax.chat`（国内版）。用户的 API Key 是国内版的，但代码里硬编码了国际版域名。
- **修复**：将路由表中 MiniMax 的 base URL 改为 `https://api.minimax.chat/v1`。
- **教训**：中国大模型厂商几乎都有国内/国际双域名。接入新供应商时**必须确认用户的 Key 属于哪个区域**。

### Bug #5: `<think>` 标签混入正文

- **症状**：MiniMax 的回复把思考过程和正文混在一起显示，无法折叠。
- **根因**：DeepSeek 用专有字段 `delta.reasoning_content` 传递思考内容，SSE 解析器只处理了这个字段。MiniMax/Qwen 用的是 OpenAI 兼容格式，思考内容用 `<think>...</think>` 标签包裹在 `delta.content` 字段里。解析器把标签当正文原样输出了。
- **修复**：在 SSE 解析器中加入 `<think>` 标签状态机，实时检测标签边界，将思考内容路由到 `thinking` 通道。
- **教训**：每家的 "OpenAI 兼容" 都不一样。DeepSeek 用专有字段，MiniMax/Qwen 用标签包裹，Anthropic 用完全不同的 SSE 格式。**接入新供应商时，必须用 curl 手动测试 SSE 原始输出**。

### Bug #6: DeepSeek thinking mode + Tool Calling 400 错误

- **症状**：用户提问 → DeepSeek 先思考再调用工具 → 工具执行成功 → 第二轮请求返回 400: `The reasoning_content in the thinking mode must be passed back to the API`
- **根因**：Tool Calling 循环中构建 assistant 消息时，只回传了 `content` 和 `tool_calls`，丢弃了 `reasoning_content`。DeepSeek 要求在 thinking 模式下，每一轮的 assistant 消息都必须包含上一轮的思考内容。
- **修复**：在 assistant 消息中追加 `reasoning_content` 字段。
- **教训**：**每个供应商的 Tool Calling + Thinking 交互协议都不同**。DeepSeek 要求回传 reasoning，其他厂商可能不需要。必须逐一测试。

### Bug #7: MiniMax 不支持 `stream_options`

- **症状**：MiniMax 返回 400，因为请求体中包含了 `stream_options: { include_usage: true }`。
- **根因**：这是 OpenAI 的扩展参数，MiniMax 不认识。
- **修复**：在构建请求体时，按 provider 条件排除不支持的参数。
- **教训**：**不要假设所有 OpenAI 兼容 API 支持所有 OpenAI 参数**。新增参数时必须加 provider 白名单。

> **铁律 #3**：接入新 LLM 供应商时，必须完成以下检查清单：
> 1. 确认 base URL（国内/国际）
> 2. curl 测试 SSE 原始输出，确认 thinking 字段名（`reasoning_content` vs `<think>` 标签）
> 3. 测试 Tool Calling 循环的第二轮请求，确认是否需要回传 thinking
> 4. 确认不支持的参数（`stream_options`、`reasoning_effort` 等），按 provider 条件排除

---

## 三、架构级死代码与路由冲突 🏗️

> **核心认知**：代码编译通过 ≠ 逻辑正确。Rust 的编译器能捕获类型错误，但捕获不了"两套路由逻辑互相覆盖"这种语义错误。

### Bug #8: 双路由引擎冲突（智能路由被忽略）

- **症状**：MiniMax 始终返回 401，尽管 `read_llm_config_for_model()` 已经正确返回了 `api.minimax.chat` 的 URL。
- **根因**：`stream_internal()` 函数中有**两套**base URL 解析逻辑：
  1. 新的智能路由：`read_llm_config_for_model()` 返回 `(provider, api_key, model_id, base_url)`
  2. 旧的硬编码路由：`get_provider_base_url(&provider)` 直接返回字符串
  
  代码先调用了 (1)，拿到了正确的 URL，但随后在第 312 行**用 (2) 的结果覆盖了 (1)**。智能路由变成了死代码。
- **修复**：删除旧的 `get_provider_base_url()` 函数，让 `stream_internal()` 直接使用 `read_llm_config_for_model()` 的返回值。
- **教训**：重构时如果引入了新的数据流路径，**必须搜索旧路径的所有调用点并删除**，否则旧逻辑会静默覆盖新逻辑。

### Bug #9: 流式缓冲区 flush 但未 emit

- **症状**：模型的回复看起来"停在了思考中"，实际上短回复的最后几个字被吞掉了。
- **根因**：`<think>` 标签状态机有一个 7 字节的安全缓冲区。SSE 流结束后，代码把缓冲区残留内容追加到了内部的 `content` 字符串（用于最终返回值），但**忘了通过 `app.emit("llm:chunk", ...)` 推送给前端**。前端看不到这些字符，但后端的最终 JSON 包含它们——造成了一种"后端有数据但前端没收到"的幽灵截断。
- **修复**：在 flush 逻辑中同步调用 `app.emit()`。
- **教训**：Tauri 的双通道架构（SSE 事件流 + 最终返回值）容易出现"只更新了一个通道"的不一致。**每次修改数据累加逻辑，都必须同时检查 emit 和 return 两条路径。**

> **铁律 #4**：在 Tauri 的流式架构中，数据有两条出口：实时事件（`app.emit`）和最终返回值（`json!({...})`）。修改任何一条时，**必须同步检查另一条**。

---

## 四、前端-后端同步问题 🔄

### Bug #17: 前后端配置 Key 名称不一致造成“信息断层”

- **症状**：用户在设置面板填写了 Tavily API 密钥并保存成功，但 Bob 联网搜索时却一直报错说“未配置 API Key”。
- **根因**：Vue 前端在 `saveApiKey` 时使用的是前端常量 `TAVILY_API_KEY`，但 Rust 后端 `tools.rs` 在做检查时使用的是硬编码的字符串 `tavily`。导致锁孔和钥匙完全对不上。
- **修复**：在 Rust 侧增加双向回退检查：`api_keys.get("TAVILY_API_KEY").or_else(|| api_keys.get("tavily"))`。
- **教训**：跨语言调用（JS ⇌ Rust）的配置文件读取，极易发生这种幽灵断层。**必须在前后端之间共享一套严谨的常量枚举，或者确保配置项名称在架构设计时就被固定。**

### Bug #10: Typing Indicator 不可见

- **症状**：用户按下回车后，没有任何视觉反馈（无弹跳圆点），直到回复文字开始流入。
- **根因**：`isStreaming = true` 之后，Vue 在下一帧渲染了弹跳圆点，但代码**没有调用 `scrollToBottom()`**。圆点被渲染在消息列表底部、屏幕可视区域之外。
- **修复**：在 `isStreaming = true` 之后加入 `await nextTick(); scrollToBottom();`。
- **教训**：Vue 的响应式渲染和 DOM 滚动是异步的。**每次改变会增加页面高度的响应式状态后，都必须在 `nextTick` 后触发滚动。**

### Bug #11: ModelHub 数据结构不匹配

- **症状**：早期开发中，模型选择器显示空白或排列错乱。
- **根因**：Electron 版的 `getModelPool()` 返回的 JSON 结构和 Rust 版不一致。Vue 前端期望 `{ id, provider, displayName, pricing: { input, output } }`，但 Rust 返回的字段名或嵌套层级不同。
- **修复**：严格比对 Vue 组件中解构的字段名，在 Rust 侧精确匹配。
- **教训**：前后端数据契约没有 TypeScript 接口约束时，**必须维护一份 IPC 数据契约审计表**（见 `todo.md` 的"IPC 数据契约审计"章节）。

### Bug #12: 跨供应商切换时 API Key 错配

- **症状**：从 DeepSeek 切换到 Doubao 后，请求仍使用 DeepSeek 的 API Key，导致鉴权失败。
- **根因**：早期的 `stream_internal()` 只读取全局的 `apiKey` 字段，没有根据当前选中模型的 `provider` 动态查找对应的 Key。
- **修复**：引入 `read_llm_config_for_model()` 函数，从 `apiKeys` 对象中按 provider 动态取 Key。
- **教训**：多供应商系统中，**凭证必须与 provider 绑定，不能有全局兜底的单一 Key**。

> **铁律 #5**：前后端数据契约的任何变更（新增字段、改名、改嵌套），都必须同时更新 `tauri-bridge.js` 和 Vue 组件中的解构代码。最好维护一份 IPC 契约文档作为 single source of truth。

---

## 五、开发环境与工具链问题 🛠️

### Bug #13: 僵尸进程导致端口占用和死 UI

- **症状**：`npm run dev:tauri` 报 `Port 5173 is already in use`；或者 Bob-Agent 窗口可见但完全无响应（黑屏/假死）。
- **根因**：`Ctrl+C` 终止开发服务器时，Node (Vite) 和 WebView2 进程有时不会被正确杀掉，变成孤儿进程继续占用端口。更糟的情况是 Rust 后端被杀了但前端还活着，用户看到的是一个没有后端的空壳。
- **修复**：手动 `taskkill /F /PID xxx` 或 `netstat -ano | findstr :5173` 找到并杀掉残余进程。
- **教训**：**每次重启 `dev:tauri` 之前，先确认 5173 端口没有被占用。** 如果发现 Bob 窗口无响应，大概率是僵尸进程问题。

### Bug #14: Windows 系统托盘幽灵图标

- **症状**：任务栏托盘区积累了多个 Bob 图标。
- **根因**：Windows 的 `explorer.exe` 对托盘图标采用惰性回收。进程被强杀时不会主动清理图标，只有鼠标划过时才检测并移除。这是 Windows 的系统级行为，非 Bug。
- **缓解**：鼠标在托盘区从左到右划一遍即可清理。正式打包版本通过优雅退出（右键→退出）可大幅减少。

### Bug #15: Vite HMR 状态损坏

- **症状**：多次热更新 Vue 模板后，整个应用黑屏，Vue 根组件挂载失败。
- **根因**：Vite 的 Hot Module Replacement 在频繁修改组件模板时，内部的组件树状态可能与新模板不一致，导致渲染管线崩溃。
- **修复**：完全杀掉所有进程，冷启动 `npm run dev:tauri`。
- **教训**：**密集修改 Vue 模板后，如果出现异常行为，先尝试冷启动再排查代码。**

---

## 六、元规则：从 JavaScript 翻译到 Rust 的系统性陷阱

| JavaScript 的宽容 | Rust 的严格 | 后果 |
|:---|:---|:---|
| 字符串随意切片 `str.slice(0, n)` | 必须在 char boundary 切 | 中文/Emoji → panic |
| `JSON.parse()` 失败返回 undefined | `.unwrap()` 失败 = 进程死亡 | 整个桌面应用崩溃 |
| 动态类型，多一个字段无所谓 | 必须精确匹配 serde 结构体 | 反序列化静默失败 |
| async/await 异常被 catch 吞掉 | tokio task panic = 线程死亡 | 后台任务无声消失 |
| `console.log` 随处可用 | 需要 `log` crate + 配置 | 无日志 = 黑盒调试 |
| 单线程事件循环，不会有竞态 | 多线程 tokio，共享状态需要 Mutex | 数据竞争 |

> **终极防御**：Rust 的编译器是最好的测试工具，但它只能检查类型安全，不能检查语义正确性。**每个 LLM 供应商的接入、每个字符串操作的边界、每个异步任务的错误路径，都需要人工审查。** "编译通过"在 Rust 中只意味着"类型安全"，不意味着"逻辑正确"。

---

## 附录：快速检查清单

### 新增 Rust 代码前必查

- [ ] 是否有 `&str[..n]` 或 `&str[n..]` 切片？→ 加 `is_char_boundary` 检查
- [ ] 是否有 `.unwrap()` 或 `.expect()`？→ 改为 `match` 或 `unwrap_or_default()`
- [ ] 是否修改了 SSE 事件流？→ 同时检查 `emit` 和 `return` 两条路径
- [ ] 是否新增了供应商特有参数？→ 加 `if provider != "xxx"` 条件排除

### 新增 LLM 供应商前必查

- [ ] Base URL 确认（国内/国际双域名）
- [ ] curl 测试 SSE 原始输出，记录 thinking 字段格式
- [ ] 测试 Tool Calling 第二轮请求是否需要回传 thinking
- [ ] 测试不支持的参数（`stream_options` 等）
- [ ] 在 `AGENTS.md` 的"模型生态扩展指南"中更新文档

### 调试无响应问题

1. 终端是否有 panic 或错误输出？
2. `netstat -ano | findstr :5173` 检查端口占用
3. 是否是僵尸进程？→ `taskkill` 后冷启动
4. 是否是 Vite HMR 损坏？→ 冷启动

---

## 六、Agent 认知与工具链异常 🧠

> **核心认知**：大语言模型（Agent）的大脑和用来执行动作的工具（Tools）是分离的。大模型的记忆受上下文窗口支配，它会产生记忆惯性。

### Bug #18: 工具调用失败引发的“自主幻觉”与“记忆污染”

- **症状**：在 Tavily API Key 未打通（参考 Bug #17）期间，Bob 居然还是成功输出了新闻简报。但当我们修好 Bug 后，他仍然坚称自己“没有配置 Key”。
- **根因分析**：
  1. **Agent 的极限生存法则（自主降级）**：当专属的 `web_search` 工具报错“未配置 Key”时，大模型极其聪明地利用了基础工具 `fetch_url`，人工拼凑了 Google News 的链接去“生拉硬拽”纯文本并解析。这是令人惊喜的 Agent 自主纠错行为。
  2. **上下文污染（Context Pollution）**：我们修好代码后，由于他之前的**失败报错**还写在他当前的短期上下文窗口里，当你问他有没有 Key 时，他会根据自己10分钟前的失败记忆回答“没有”，而不会主动再去调用一次工具验证代码是否已修好。
- **缓解与防御**：
  - 代码热修复后，调试 Agent 必须**强制开启新会话**或**发强制验证指令**。
  - 在底层工具报错信息中，应当嵌入“解决建议”（如：`请在设置中添加，或把密钥发给我`），利用错误日志来做 Prompt Engineering，从而引导模型的错误处理流。
