/**
 * Vite dev server 启动等待脚本
 *
 * Electron 需要等 Vite dev server 完全就绪后再启动。
 * 每 500ms 轮询 http://localhost:5173 直到可访问。
 */

const http = require('http');

const VITE_URL = 'http://localhost:5173';
const MAX_RETRIES = 30;
const INTERVAL = 500;

let retries = 0;

function check() {
  http.get(VITE_URL, (res) => {
    if (res.statusCode === 200) {
      console.log('✓ Vite dev server is ready');
      process.exit(0);
    } else {
      retry();
    }
  }).on('error', retry);
}

function retry() {
  retries++;
  if (retries >= MAX_RETRIES) {
    console.error('✗ Vite dev server did not start in time');
    process.exit(1);
  }
  setTimeout(check, INTERVAL);
}

console.log('Waiting for Vite dev server...');
check();
