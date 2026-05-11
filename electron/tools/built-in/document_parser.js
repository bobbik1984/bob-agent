const fs = require('fs');
const path = require('path');
const { BaseTool } = require('../base');

/**
 * 知识库智能解析引擎核心组件
 */
class DocumentParserTool extends BaseTool {
  constructor() {
    super();
    this.name = 'document_parser';
    this.description = '智能知识库解析能力，能将各种复杂文档（PDF, PPTX, DOCX, XLSX）高保真地解析为纯文本 Markdown 格式。';
    this.input_schema = {
      type: 'object',
      properties: {
        filePath: {
          type: 'string',
          description: '需要解析的源文件绝对路径'
        }
      },
      required: ['filePath']
    };
  }

  async execute(params) {
    const { filePath } = params;
    
    if (!fs.existsSync(filePath)) {
      throw new Error(`[DocumentParser] File not found: ${filePath}`);
    }

    const ext = path.extname(filePath).toLowerCase();
    
    try {
      if (ext === '.pdf') {
        const pdf = require('pdf-parse');
        const dataBuffer = fs.readFileSync(filePath);
        const data = await pdf(dataBuffer);
        return data.text;
      } 
      
      else if (ext === '.docx') {
        const mammoth = require('mammoth');
        const result = await mammoth.extractRawText({ path: filePath });
        return result.value;
      }
      
      else if (ext === '.xlsx' || ext === '.csv') {
        const xlsx = require('xlsx');
        const workbook = xlsx.readFile(filePath);
        let output = '';
        
        workbook.SheetNames.forEach(sheetName => {
          output += `\n### Sheet: ${sheetName}\n\n`;
          const worksheet = workbook.Sheets[sheetName];
          const json = xlsx.utils.sheet_to_json(worksheet, { header: 1 });
          if (json.length > 0) {
            // Add headers
            const headers = json[0].map(h => String(h || '').replace(/\|/g, '\\|').replace(/\n/g, ' '));
            output += '| ' + headers.join(' | ') + ' |\n';
            output += '| ' + headers.map(() => '---').join(' | ') + ' |\n';
            
            // Add rows
            for (let i = 1; i < json.length; i++) {
              const row = json[i].map(c => String(c || '').replace(/\|/g, '\\|').replace(/\n/g, '<br>'));
              while (row.length < headers.length) row.push('');
              output += '| ' + row.join(' | ') + ' |\n';
            }
          }
        });
        return output;
      }
      
      else if (ext === '.pptx') {
        const officeParser = require('officeparser');
        const parsed = await officeParser.parseOffice(filePath);
        return typeof parsed.toText === 'function' ? parsed.toText() : String(parsed);
      }
      
      else if (ext === '.md' || ext === '.txt') {
        return fs.readFileSync(filePath, 'utf-8');
      }
      
      throw new Error(`Unsupported file type: ${ext}. Currently supports .pdf, .docx, .xlsx, .csv, .pptx, .md, .txt`);

    } catch (error) {
      console.error(`[DocumentParser] Failed to parse ${filePath}:`, error);
      throw new Error(`Failed to parse document: ${error.message}`);
    }
  }
}

module.exports = new DocumentParserTool();
