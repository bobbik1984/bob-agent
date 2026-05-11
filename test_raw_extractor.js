const fs = require('fs');
const path = require('path');
const documentParser = require('./electron/tools/built-in/document_parser');

const TARGET_DIR = 'D:\\OneDrive\\Projects\\华润深圳大区\\Ref\\竞品项目考察报告\\20250928_华发冰雪世界';
const OUTPUT_FILE = path.join(__dirname, '华发报告.raw.md');

async function main() {
  console.log('🚀 开始测试原生 JS 解析引擎 (替代 markitdown)...');
  console.log(`📂 目标目录: ${TARGET_DIR}`);
  
  if (!fs.existsSync(TARGET_DIR)) {
    console.error(`❌ 找不到目标目录: ${TARGET_DIR}`);
    return;
  }

  const files = fs.readdirSync(TARGET_DIR)
    .filter(f => f.endsWith('.pptx') || f.endsWith('.xlsx') || f.endsWith('.pdf'))
    .map(f => path.join(TARGET_DIR, f));

  console.log(`🔍 找到 ${files.length} 个重型测试文件:`);
  files.forEach(f => console.log(`   - ${path.basename(f)}`));

  let combinedMarkdown = '# 华发冰雪世界 - 原生解析器测试结果\n\n';

  for (const file of files) {
    console.log(`\n⏳ 正在解析: ${path.basename(file)}...`);
    try {
      const startTime = Date.now();
      
      // 使用 document_parser.js 工具执行
      const content = await documentParser.execute({ filePath: file });
      
      const duration = ((Date.now() - startTime) / 1000).toFixed(2);
      console.log(`✅ 解析成功! (耗时: ${duration}s, 提取了 ${content.length} 个字符)`);
      
      combinedMarkdown += `## 文件: ${path.basename(file)}\n\n`;
      combinedMarkdown += content;
      combinedMarkdown += `\n\n---\n\n`;
    } catch (err) {
      console.error(`❌ 解析失败:`, err.message);
      combinedMarkdown += `## 文件: ${path.basename(file)}\n\n*解析失败: ${err.message}*\n\n---\n\n`;
    }
  }

  fs.writeFileSync(OUTPUT_FILE, combinedMarkdown, 'utf-8');
  console.log(`\n🎉 测试完成！结果已保存至: ${OUTPUT_FILE}`);
}

main().catch(console.error);
