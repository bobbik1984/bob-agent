const { spawn } = require('child_process');
const path = require('path');
const WebSocket = require('ws');

class ObscuraDriver {
  constructor() {
    this.host = '127.0.0.1';
    this.port = 9222;
    this.ws = null;
    this.msgId = 0;
    this.sessionId = null;
    this.targetId = null;
    this.process = null;
    this.pendingRequests = new Map();
  }

  /**
   * Lazily starts Obscura if not running, then connects via WebSocket.
   */
  async ensureConnected() {
    if (this.ws && this.ws.readyState === WebSocket.OPEN && this.sessionId) {
      return; // Already connected
    }

    try {
      // 1. Try to connect first in case it's already running
      await this._connectWs();
    } catch (err) {
      console.log('[ObscuraDriver] Not running, starting Obscura...');
      await this._spawnProcess();
      await this._waitForPort(3000);
      await this._connectWs();
    }
  }

  async _spawnProcess() {
    let obscuraPath;
    const fs = require('fs');
    try {
      const { app } = require('electron');
      if (app && app.isPackaged) {
        // When packaged, extraResources places it here:
        obscuraPath = path.join(process.resourcesPath, 'resources', 'bin', 'obscura.exe');
        if (!fs.existsSync(obscuraPath)) {
            // Fallback just in case
            obscuraPath = path.join(process.resourcesPath, 'bin', 'obscura.exe');
        }
      } else {
        obscuraPath = path.resolve(__dirname, '../../resources/bin/obscura.exe');
      }
    } catch (e) {
      // Not running in Electron environment (e.g. test script)
      obscuraPath = path.resolve(__dirname, '../../resources/bin/obscura.exe');
    }
    
    if (!fs.existsSync(obscuraPath)) {
      throw new Error(`Obscura executable not found at: ${obscuraPath}`);
    }
    
    return new Promise((resolve, reject) => {
      this.process = spawn(obscuraPath, ['serve', '--port', this.port.toString(), '--stealth'], {
        detached: true,
        windowsHide: true,
      });

      this.process.stdout.on('data', (data) => console.log(`[Obscura] ${data.toString().trim()}`));
      this.process.stderr.on('data', (data) => console.error(`[Obscura ERR] ${data.toString().trim()}`));

      this.process.on('error', (err) => {
        reject(new Error(`Failed to start Obscura: ${err.message}`));
      });

      // Give it a brief moment to throw synchronous spawn errors
      setTimeout(resolve, 1000);
    });
  }

  async _waitForPort(delayMs) {
    return new Promise(resolve => setTimeout(resolve, delayMs));
  }

  async _connectWs() {
    return new Promise((resolve, reject) => {
      const uri = `ws://${this.host}:${this.port}/devtools/browser`;
      this.ws = new WebSocket(uri, { maxPayload: 50 * 1024 * 1024 });

      this.ws.on('open', async () => {
        console.log('[ObscuraDriver] WebSocket connected');
        try {
          await this._initializeSession();
          resolve();
        } catch (err) {
          reject(err);
        }
      });

      this.ws.on('message', (data) => {
        const msg = JSON.parse(data);
        
        if (msg.method === 'Target.attachedToTarget') {
          this.sessionId = msg.params.sessionId;
        }

        if (msg.id && this.pendingRequests.has(msg.id)) {
          const { resolve: res, reject: rej } = this.pendingRequests.get(msg.id);
          this.pendingRequests.delete(msg.id);
          if (msg.error) {
            rej(new Error(`CDP Error: ${JSON.stringify(msg.error)}`));
          } else {
            res(msg.result || {});
          }
        }
      });

      this.ws.on('error', (err) => {
        reject(err);
      });

      this.ws.on('close', () => {
        console.log('[ObscuraDriver] WebSocket closed');
        this.ws = null;
        this.sessionId = null;
        this.pendingRequests.forEach(({ reject }) => reject(new Error('WebSocket closed')));
        this.pendingRequests.clear();
      });
    });
  }

  async _send(method, params = null) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error('WebSocket is not connected');
    }

    this.msgId++;
    const id = this.msgId;
    const msg = { id, method };
    if (params) msg.params = params;

    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });
      this.ws.send(JSON.stringify(msg));
      
      // Safety timeout
      setTimeout(() => {
        if (this.pendingRequests.has(id)) {
          this.pendingRequests.delete(id);
          reject(new Error(`Timeout waiting for CDP response to ${method}`));
        }
      }, 15000);
    });
  }

  async _sendSession(method, params = null) {
    if (!this.sessionId) throw new Error('No active session ID');
    
    this.msgId++;
    const id = this.msgId;
    const msg = { id, method, sessionId: this.sessionId };
    if (params) msg.params = params;

    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });
      this.ws.send(JSON.stringify(msg));
      
      // Session commands like evaluate or navigate can take longer, use 30s timeout
      setTimeout(() => {
        if (this.pendingRequests.has(id)) {
          this.pendingRequests.delete(id);
          reject(new Error(`Timeout waiting for Session CDP response to ${method}`));
        }
      }, 30000);
    });
  }

  async _initializeSession() {
    // 1. Get targets
    const targetsRes = await this._send('Target.getTargets');
    const targets = targetsRes.targetInfos || [];
    
    if (targets.length > 0) {
      this.targetId = targets[0].targetId;
    } else {
      const createRes = await this._send('Target.createTarget', { url: 'about:blank' });
      this.targetId = createRes.targetId;
    }

    // 2. Attach
    const attachRes = await this._send('Target.attachToTarget', {
      targetId: this.targetId,
      flatten: true
    });
    
    if (attachRes.sessionId) {
      this.sessionId = attachRes.sessionId;
    } else {
      // Wait a bit for the attachedToTarget event
      await new Promise(r => setTimeout(r, 1000));
    }

    if (!this.sessionId) throw new Error('Failed to obtain session ID');

    // 3. Enable domains
    await this._sendSession('Page.enable').catch(() => {});
    await this._sendSession('Runtime.enable').catch(() => {});
  }

  // ================= API Methods =================

  async navigate(url) {
    await this.ensureConnected();
    console.log(`[ObscuraDriver] Navigating to ${url}...`);
    
    await this._sendSession('Page.navigate', { url });
    
    // Polling for readyState complete (max 30s)
    let elapsed = 0;
    while (elapsed < 30) {
      await new Promise(r => setTimeout(r, 1000));
      elapsed++;
      try {
        const res = await this._sendSession('Runtime.evaluate', { expression: 'document.readyState' });
        if (res.result && res.result.value === 'complete') {
          console.log(`[ObscuraDriver] Page loaded in ${elapsed}s`);
          return { success: true, message: `Navigated to ${url}` };
        }
      } catch (e) {
        // Ignore evaluation errors during navigation
      }
    }
    console.log('[ObscuraDriver] Navigation polling timed out, proceeding anyway.');
    return { success: true, message: `Navigated to ${url} (Warning: Page might still be loading)` };
  }

  async getHtml(maxChars = 10000) {
    await this.ensureConnected();
    // Return a minified structure to save tokens: innerText + links/inputs
    const js = `
      (() => {
        let text = document.body ? document.body.innerText.substring(0, ${maxChars}) : '';
        
        const elements = [];
        document.querySelectorAll('a, button, input, textarea, select').forEach(el => {
            const tag = el.tagName.toLowerCase();
            const elText = (el.innerText || el.value || el.placeholder || '').trim().substring(0, 50);
            const id = el.id ? '#' + el.id : '';
            let classes = el.className;
            if (typeof classes === 'string') {
               classes = classes.split(' ').slice(0, 2).map(c => '.'+c).join('');
            } else {
               classes = '';
            }
            if (elText || el.name || el.id) {
               const nameAttr = el.name ? ' name="' + el.name + '"' : '';
               elements.push('<' + tag + ' ' + (id || classes) + nameAttr + '> ' + elText);
            }
        });
        
        return {
           title: document.title,
           text: text,
           actionable: elements.slice(0, 100)
        };
      })()
    `;
    const res = await this._sendSession('Runtime.evaluate', {
      expression: js,
      returnByValue: true
    });
    
    if (res.exceptionDetails) {
      console.error('[ObscuraDriver] getHtml JS error:', res.exceptionDetails.exception.description);
      return { error: 'Failed to extract DOM', details: res.exceptionDetails.exception.description };
    }
    
    return res.result ? res.result.value : { error: 'Failed to extract DOM' };
  }

  async click(selector) {
    await this.ensureConnected();
    const res = await this._sendSession('Runtime.evaluate', {
      expression: `
        (() => {
          const el = document.querySelector('${selector}');
          if (el) { el.click(); return true; }
          return false;
        })()
      `,
      returnByValue: true
    });
    
    const success = res.result && res.result.value;
    return { success, message: success ? `Clicked ${selector}` : `Element not found: ${selector}` };
  }

  async type(selector, text) {
    await this.ensureConnected();
    const res = await this._sendSession('Runtime.evaluate', {
      expression: `
        (() => {
          const el = document.querySelector('${selector}');
          if (!el) return false;
          el.focus();
          el.value = '${text}';
          el.dispatchEvent(new Event('input', { bubbles: true }));
          el.dispatchEvent(new Event('change', { bubbles: true }));
          return true;
        })()
      `,
      returnByValue: true
    });
    
    const success = res.result && res.result.value;
    return { success, message: success ? `Typed text into ${selector}` : `Element not found: ${selector}` };
  }

  async evaluateJs(expression) {
    await this.ensureConnected();
    const res = await this._sendSession('Runtime.evaluate', {
      expression: expression,
      returnByValue: true
    });
    
    return { 
      value: res.result ? res.result.value : null,
      error: res.exceptionDetails ? res.exceptionDetails.exception.description : null 
    };
  }

  async close() {
    if (this.ws) {
      this.ws.close();
    }
    if (this.process) {
      this.process.kill();
      this.process = null;
    }
  }
}

// Singleton export
module.exports = new ObscuraDriver();
