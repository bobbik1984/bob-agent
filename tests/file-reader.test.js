import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { readFile, MAX_FILE_SIZE } from '../electron/services/file-reader.js';
import * as fs from 'fs';
import * as path from 'path';

describe('FileReader', () => {
  const testDir = path.join(__dirname, 'test-files');
  const txtPath = path.join(testDir, 'test.txt');
  const bomPath = path.join(testDir, 'test_bom.txt');
  const unsupportPath = path.join(testDir, 'test.unsupported');
  const largePath = path.join(testDir, 'toolarge.txt');

  // Setup actual files for testing
  beforeAll(() => {
    if (!fs.existsSync(testDir)) {
      fs.mkdirSync(testDir);
    }
    fs.writeFileSync(txtPath, 'hello world', 'utf-8');
    fs.writeFileSync(bomPath, '\uFEFFhello world with BOM', 'utf-8');
    fs.writeFileSync(unsupportPath, 'data', 'utf-8');

    // Create a large file
    const largeBuffer = Buffer.alloc(MAX_FILE_SIZE + 10, 'A');
    fs.writeFileSync(largePath, largeBuffer);
  });

  afterAll(() => {
    if (fs.existsSync(txtPath)) fs.unlinkSync(txtPath);
    if (fs.existsSync(bomPath)) fs.unlinkSync(bomPath);
    if (fs.existsSync(unsupportPath)) fs.unlinkSync(unsupportPath);
    if (fs.existsSync(largePath)) fs.unlinkSync(largePath);
    if (fs.existsSync(testDir)) fs.rmdirSync(testDir);
  });

  it('should read a plain text file without BOM successfully', async () => {
    const result = await readFile(txtPath);

    expect(result.content).toBe('hello world');
    expect(result.type).toBe('.txt');
    expect(result.size).toBeGreaterThan(0);
    expect(result.name).toBe('test.txt');
  });

  it('should read a plain text file and remove UTF-8 BOM', async () => {
    const result = await readFile(bomPath);

    expect(result.content).toBe('hello world with BOM');
  });

  it('should throw an error for unsupported file formats', async () => {
    await expect(readFile(unsupportPath)).rejects.toThrow('不支持的文件格式: .unsupported');
  });

  it('should throw an error if the file does not exist', async () => {
    await expect(readFile(path.join(testDir, 'nonexistent.txt'))).rejects.toThrow('文件不存在:');
  });

  it('should reject files exceeding MAX_FILE_SIZE', async () => {
    await expect(readFile(largePath)).rejects.toThrow(/文件过大 .*，最大支持 500KB/);
  });
});
