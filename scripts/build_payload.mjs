import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import AdmZip from 'adm-zip';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const TAURI_DIR = path.join(ROOT_DIR, 'src-tauri');

// 目标文件：生成的 payload.zip
const OUTPUT_FILE = path.join(ROOT_DIR, 'payload.zip');

// 需要打包的文件和目录清单
const TARGETS = [
  { 
    src: path.join(TAURI_DIR, 'target', 'release', 'bob.exe'), 
    dest: 'bob.exe', 
    type: 'file',
    required: true
  },
  { 
    src: path.join(TAURI_DIR, 'pdfium.dll'), 
    dest: 'pdfium.dll', 
    type: 'file',
    required: true
  },
  { 
    src: path.join(ROOT_DIR, 'skills'), 
    dest: 'skills', 
    type: 'dir',
    required: true
  }
];

function buildPayload() {
  console.log('📦 开始构建 Payload...');

  // 确保先编译出最新的 bob.exe
  if (!fs.existsSync(TARGETS[0].src)) {
    console.error('❌ 找不到 bob.exe，请先运行 npm run tauri build 构建 Release 版本。');
    process.exit(1);
  }

  const zip = new AdmZip();

  for (const target of TARGETS) {
    if (!fs.existsSync(target.src)) {
      if (target.required) {
        console.error(`❌ 找不到必需的文件或目录: ${target.src}`);
        process.exit(1);
      } else {
        console.log(`⚠️ 跳过可选项目 (未找到): ${target.src}`);
        continue;
      }
    }

    if (target.type === 'file') {
      console.log(`➕ 添加文件: ${target.dest}`);
      zip.addLocalFile(target.src);
    } else if (target.type === 'dir') {
      console.log(`➕ 添加目录: ${target.dest}`);
      zip.addLocalFolder(target.src, target.dest);
    }
  }

  console.log('🔄 正在压缩文件...');
  zip.writeZip(OUTPUT_FILE);
  
  console.log(`✅ Payload 构建完成！`);
  console.log(`📂 文件位置: ${OUTPUT_FILE}`);
}

buildPayload();
