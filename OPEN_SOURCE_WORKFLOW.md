# 开源发布工作流 (Dual-Repo Workflow)

> ⚠️ **所有 Agent 请注意**：本项目采用“私有开发库 + 公开开源库”的双轨制管理方案。严禁将日常的、带有敏感信息或庞大历史包袱的开发提交（Commits）直接推送到公开仓库。

## 架构说明

为了在保证开发历史可回溯、测试环境安全的同时，为开源社区提供最纯净的代码，本项目的远端（Remote）配置如下：

1. **私有开发库 (`origin`)**：
   - 地址：`https://github.com/bobbik1984/bob-agent-private.git`
   - 用途：日常开发的 `main` 分支、功能分支（Feature Branches）、所有的测试提交和开发历史，全部推送到此仓库。
   - 特点：包含完整历史，属于 Private 仓库。

2. **公开开源库 (`public`)**：
   - 地址：`https://github.com/bobbik1984/bob-agent.git`
   - 用途：仅用于向外发布清理过、脱敏后的正式 Release 源码。
   - 特点：属于 Public 仓库，无日常碎片化历史。

## 发布新版本到公开库的 SOP

当在 `main` 分支（私有库）上完成了诸如 `v0.5.0` 等大版本的开发，并准备对外开源发布时，**Agent 必须严格执行以下指令序列**来同步代码，而不是直接 `git push public main`：

```bash
# 1. 切换到专门用于同步公开库的干净分支
git checkout public-release

# 2. 将私有主分支（main）的最新代码状态“强拉”过来（不带历史记录）
git checkout main -- .

# 3. 提交一个整洁的发布说明
git commit -m "chore: release vX.X.X (Public Sync)"

# 4. 推送到公开开源仓库
git push public

# 5. 切换回日常开发主分支
git checkout main
```

## 注意事项
* 在同步代码到公开库前，确保代码已处于“瘦身”状态（即通过 `.gitignore` 排除了超大的测试文档、二进制构建产物等），保持公开仓库的克隆体积最小。
* 公开库和私有库在本地共享同一个工作区，绝不要搞混 `origin` 和 `public` 两个远端。
