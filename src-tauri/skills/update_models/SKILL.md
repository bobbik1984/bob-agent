---
name: 更新模型注册表
description: 搜索供应商最新模型，验证后更新本地注册表
---

# 更新模型注册表

当用户要求更新某个供应商的模型列表时，按以下步骤执行：

## 步骤

1. **读取当前配置**：调用 `read_model_registry(provider_id)` 查看现有模型列表
2. **搜索最新信息**：调用 `web_search` 搜索该供应商的最新模型文档，例如搜索 "通义千问 最新模型 API model ID"
3. **对比差异**：找出新增、停用或更名的模型
4. **逐个验证**：对每个新模型调用 `test_model_endpoint(provider_id, model_id)` 验证连通性
5. **更新注册表**：只将测试通过的模型调用 `update_model_registry` 写入
6. **汇报结果**：告诉用户更新了哪些模型，哪些验证失败

## 安全规则

- **绝不跳过验证步骤**：未经 test_model_endpoint 确认的模型不得写入
- **保留旧模型**：除非确认已停用，否则保留现有模型（用户可能正在使用）
- **记录变更**：在回复中明确说明新增/删除/保留了哪些模型

## 示例对话

用户：「Qwen 的模型好像更新了，你搜索一下然后更新」

你的执行流程：
1. 调用 read_model_registry("qwen")
2. 调用 web_search("通义千问 DashScope 最新模型 API model ID 2026")
3. 解析搜索结果，提取新模型 ID
4. 对每个新模型调用 test_model_endpoint("qwen", "新模型ID")
5. 调用 update_model_registry("qwen", [...更新后的完整列表])
6. 回复：「已为通义千问更新模型列表：新增 qwen3.7-turbo（验证通过），保留 qwen3.6-plus、qwen3-max」
