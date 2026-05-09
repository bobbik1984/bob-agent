/**
 * bob-agent — 文件读取引擎
 *
 * 读取本地文件并提取纯文本内容。
 * 支持纯文本 + Office 格式 (.docx/.xlsx/.pdf)
 *
 * TODO: Jules 完成 Office 格式实现
 */

const fs = require('fs');
const path = require('path');

const MAX_FILE_SIZE = 500 * 1024; // 500KB

// 纯文本扩展名
const PLAIN_TEXT_EXTENSIONS = new Set([
  '.txt', '.md', '.csv', '.json', '.yaml', '.yml', '.log',
  '.py', '.js', '.ts', '.jsx', '.tsx', '.java', '.go', '.sql',
  '.html', '.css', '.xml', '.toml', '.ini', '.sh', '.bat',
]);

/**
 * 读取文件并提取文本内容
 * @param {string} filePath - 文件绝对路径
 * @returns {Promise<{content: string, type: string, size: number, name: string}>}
 */
async function readFile(filePath) {
  // 检查文件是否存在
  if (!fs.existsSync(filePath)) {
    throw new Error(`文件不存在: ${filePath}`);
  }

  const stat = fs.statSync(filePath);
  if (stat.size > MAX_FILE_SIZE) {
    throw new Error(`文件过大 (${(stat.size / 1024).toFixed(0)}KB)，最大支持 500KB`);
  }

  const ext = path.extname(filePath).toLowerCase();
  const name = path.basename(filePath);

  // 纯文本
  if (PLAIN_TEXT_EXTENSIONS.has(ext)) {
    let content = fs.readFileSync(filePath, 'utf-8');
    if (content.charCodeAt(0) === 0xFEFF) {
      content = content.slice(1);
    }
    return { content, type: ext, size: stat.size, name };
  }

  // Office 格式
  switch (ext) {
    case '.docx':
      return await readDocx(filePath, stat.size, name);
    case '.xlsx':
      return await readXlsx(filePath, stat.size, name);
    case '.pdf':
      return await readPdf(filePath, stat.size, name);
    default:
      throw new Error(`不支持的文件格式: ${ext}`);
  }
}

async function readDocx(filePath, size, name) {
  try {
    const mammoth = require('mammoth');
    const result = await mammoth.extractRawText({ path: filePath });
    return { content: result.value, type: '.docx', size, name };
  } catch (error) {
    throw new Error(`Word 文档读取失败: ${error.message || '文件可能已损坏'}`);
  }
}

async function readXlsx(filePath, size, name) {
  try {
    const XLSX = require('xlsx');
    const workbook = XLSX.readFile(filePath);
    const sheets = [];
    for (const sheetName of workbook.SheetNames) {
      const sheet = workbook.Sheets[sheetName];
      const range = XLSX.utils.decode_range(sheet['!ref'] || 'A1');
      if (range.e.r > 500) {
        range.e.r = 499; // limit to 500 rows (0-indexed)
        sheet['!ref'] = XLSX.utils.encode_range(range);
      }
      const csv = XLSX.utils.sheet_to_csv(sheet, { FS: ',', blankrows: false });
      const lines = csv.split('\n').slice(0, 500); // safety net
      sheets.push(`=== Sheet: ${sheetName} ===\n${lines.join('\n')}`);
    }
    return { content: sheets.join('\n\n'), type: '.xlsx', size, name };
  } catch (error) {
    throw new Error(`Excel 文件读取失败: ${error.message || '未知错误'}`);
  }
}

async function readPdf(filePath, size, name) {
  try {
    const pdfParse = require('pdf-parse');
    const buffer = fs.readFileSync(filePath);
    const data = await pdfParse(buffer);
    return { content: data.text, type: '.pdf', size, name };
  } catch (error) {
    throw new Error(`PDF 文件读取失败: ${error.message || '未知错误'}`);
  }
}

module.exports = { readFile, MAX_FILE_SIZE };
