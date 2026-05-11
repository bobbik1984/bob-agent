/**
 * Phase 2: Wiki 层语义提纯 + Phase 3: 全局索引织网
 * 
 * 放弃使用不稳定的外部 gemini CLI，
 * 改用 bob-agent 原生内置的 LLMClient 进行直接调用。
 */

const fs = require('fs');
const path = require('path');
const { LLMClient } = require('./electron/services/llm-client');
const { Database } = require('./electron/services/db');
const { app } = require('electron'); // 在纯 Node 环境下会报错，因此我们需要手动模拟或跳过 app 依赖

// ─── 配置 ────────────────────────────────────────────────────────
const RAW_MD_FILE  = path.join(__dirname, '华发报告.raw.md');
const WIKI_OUT_DIR = path.join(__dirname, '华发_wiki');
const INDEX_FILE   = path.join(WIKI_OUT_DIR, 'directory_index.md');

// Wiki 提纯的 System Prompt
const WIKI_PROMPT_TEMPLATE = (filename, rawContent) => `
你是一位信息密度极高的"文档知识蒸馏师"。
我即将给你一份从【${filename}】中提取的原始文本，你需要将它提纯为一张结构化的"LLM 知识库卡片"。

**严格按照以下 Markdown 格式输出，不要输出任何额外的解释或前言：**

## 文件：${filename}

### 【核心价值】
（1-2 句话，回答"这份文件最核心的信息/结论是什么"）

### 【关键数据 & 实体】
（提炼所有重要数字、指标、项目名称、公司名称、人名、地点，以项目符号列出）

### 【结构摘要】
（以 2-5 个要点列出文件的主要章节/板块和其核心内容）

### 【关联推测】
（推测这份文件与目录中其他文件的关联：例如"顾问清单.xlsx 提供了本报告中提及的顾问单位的联系方式"）

### 【适用查询场景】
（列出 3 个用户可能会问、而此文件能直接回答的自然语言问题）

---

原始文本内容如下：

\`\`\`
${rawContent.substring(0, 15000)}
\`\`\`
`;

// 全局目录索引 Prompt
const INDEX_PROMPT_TEMPLATE = (wikiContents) => `
你是一位严谨的"知识库图书馆员"。
我即将给你一个目录下所有文件的 wiki 卡片摘要，
你需要将它整合为一张全局的【目录导览索引表】，让 AI Agent 或用户能在 5 秒内定位到他们需要的文件。

**严格按照以下 Markdown 格式输出：**

# 华发冰雪世界项目 — 全局知识库导览索引

## 快速导航表

| 文件名 | 类型 | 核心价值（一句话） | 最适合回答 |
|--------|------|-------------------|-----------|
（每个文件一行）

## 文件关系图

（用文字描述这些文件之间的逻辑关系和依赖关系，例如"看 PPT 整体架构 → 查 Excel 数据细节 → 阅读 PDF 深度研究"）

## 推荐阅读路径

**场景1：想了解整体市场背景** → 先看...
**场景2：想查顾问和供应商** → 先看...
**场景3：想了解项目架构框架** → 先看...

---

以下是所有文件的 Wiki 卡片：

${wikiContents}
`;

// ─── 解析 raw.md 为各文件的独立 section ────────────────────────
function parseRawMarkdown(rawContent) {
  const sections = [];
  const parts = rawContent.split(/^## 文件: /m);

  for (let i = 1; i < parts.length; i++) {
    const part = parts[i];
    const firstNewline = part.indexOf('\n');
    const filename = part.substring(0, firstNewline).trim();
    const content  = part.substring(firstNewline + 1).trim();
    if (!content || content.startsWith('*解析失败')) continue;
    sections.push({ filename, content });
  }
  return sections;
}

// ─── 主流程 ────────────────────────────────────────────────────
async function main() {
  console.log('🧠 Phase 2+3: Wiki 语义提纯 + 全局索引织网 (使用内置 LLMClient)\n');

  // 由于脚本在纯 Node 环境运行，我们手动提取 .env 中支持的模型 API KEY
  // 这里我们假设环境变量或代码里配置好 deepseek-v4-flash 或者可以直接访问的提供商
  // 为确保100%跑通，我们可以借用 TinyFish 或其它直接配置
  
  // 初始化 LLMClient
  // 这里为了独立测试，我们提供硬编码回退，或者读取 db
  // 由于我们是 bob-agent 的开发者，我们可以直接构造一个简单的 LLMClient 实例
  // 因为没有 electron.app，直接用模拟路径
  const userDataPath = path.join(process.env.APPDATA || process.env.HOME || '.', 'bob-agent');
  if (!fs.existsSync(userDataPath)) fs.mkdirSync(userDataPath, { recursive: true });
  
  const db = new Database(userDataPath);
  const provider = db.getConfig('provider') || 'deepseek';
  const apiKey = db.getConfig('apiKey') || '';
  const model = db.getConfig('model') || '';

  if (!apiKey) {
    console.error('❌ 请先在 bob-agent 客户端中配置大模型 API Key！');
    process.exit(1);
  }

  const llm = new LLMClient({ provider, apiKey, model });
  console.log(`🔌 已连接 LLM 引擎: [${provider}] - ${model || '默认模型'}`);

  if (!fs.existsSync(RAW_MD_FILE)) {
    console.error(`❌ 找不到 ${RAW_MD_FILE}，请先运行 test_raw_extractor.js`);
    process.exit(1);
  }
  const rawContent = fs.readFileSync(RAW_MD_FILE, 'utf-8');
  const sections   = parseRawMarkdown(rawContent);

  console.log(`📂 共找到 ${sections.length} 个可解析文件段落\n`);

  if (!fs.existsSync(WIKI_OUT_DIR)) {
    fs.mkdirSync(WIKI_OUT_DIR, { recursive: true });
  }

  const wikiCards = [];

  for (const { filename, content } of sections) {
    console.log(`⏳ 正在提纯: ${filename}...`);
    const startTime = Date.now();

    try {
      const prompt = WIKI_PROMPT_TEMPLATE(filename, content);
      
      const response = await llm.chat([{ role: 'user', content: prompt }]);
      
      const wikiCard = response.content;
      const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
      console.log(`✅ 提纯完成! (${elapsed}s, 压缩为 ${wikiCard.length} 字符)`);

      const safeFilename = filename.replace(/[\\/:*?"<>|]/g, '_');
      const outFile = path.join(WIKI_OUT_DIR, `${safeFilename}.wiki.md`);
      fs.writeFileSync(outFile, wikiCard, 'utf-8');

      wikiCards.push({ filename, wikiCard, outFile });
    } catch (err) {
      console.error(`❌ 提纯失败 [${filename}]:`, err.message);
    }
  }

  console.log('\n🕸️  Phase 3: 织网 → 生成全局目录索引...');
  const allWikiContent = wikiCards.map(w => w.wikiCard).join('\n\n---\n\n');

  try {
    const indexPrompt = INDEX_PROMPT_TEMPLATE(allWikiContent);
    const indexResponse = await llm.chat([{ role: 'user', content: indexPrompt }]);
    
    fs.writeFileSync(INDEX_FILE, indexResponse.content, 'utf-8');
    console.log(`✅ 全局索引已生成: ${INDEX_FILE}`);
  } catch (err) {
    console.error('❌ 全局索引生成失败:', err.message);
  }

  console.log('\n🎉 Phase 2+3 全部完成！');
}

main().catch(console.error);
