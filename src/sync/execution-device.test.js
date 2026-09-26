import { describe, expect, it } from 'vitest';

export function parseDeviceInstruction(input, pairedDevices = []) {
  const pcMatch = (input || '').match(/^(@pc|\/pc)\s*(.*)/is);
  if (pcMatch) {
    const stripped = pcMatch[2].trim();
    const targetPc = pairedDevices.find(d => d.platform === 'windows' || d.platform === 'desktop' || !d.platform);
    return {
      isRemote: true,
      instruction: stripped,
      targetDeviceId: targetPc ? targetPc.device_id : null,
      deviceFound: !!targetPc,
    };
  }
  return {
    isRemote: false,
    instruction: (input || '').trim(),
    targetDeviceId: 'local',
    deviceFound: true,
  };
}

export function formatRemoteResponse(payload, fallbackName = '电脑端 (PC)') {
  return {
    content: payload?.result || '执行完毕，无返回内容。',
    elapsed_ms: payload?.elapsed_ms || 0,
    executor_device: payload?.executor_device || fallbackName,
    is_error: payload?.status === 'error',
  };
}

describe('Execution Device & Remote RPC Logic', () => {
  const mockDevices = [
    { device_id: 'pc-thinkpad', platform: 'windows', device_name: 'ThinkPad X1', last_seen: Date.now() },
  ];

  it('correctly parses @pc prefix and binds to paired PC', () => {
    const res = parseDeviceInstruction('@pc 读取工作区 README.md 文件', mockDevices);
    expect(res.isRemote).toBe(true);
    expect(res.instruction).toBe('读取工作区 README.md 文件');
    expect(res.targetDeviceId).toBe('pc-thinkpad');
    expect(res.deviceFound).toBe(true);
  });

  it('correctly parses /pc prefix case-insensitively', () => {
    const res = parseDeviceInstruction('/PC 分析代码架构', mockDevices);
    expect(res.isRemote).toBe(true);
    expect(res.instruction).toBe('分析代码架构');
    expect(res.targetDeviceId).toBe('pc-thinkpad');
  });

  it('keeps normal messages local', () => {
    const res = parseDeviceInstruction('你好，今天天气怎么样？', mockDevices);
    expect(res.isRemote).toBe(false);
    expect(res.instruction).toBe('你好，今天天气怎么样？');
    expect(res.targetDeviceId).toBe('local');
  });

  it('detects when no paired PC is available for @pc instruction', () => {
    const res = parseDeviceInstruction('@pc 查看文件', []);
    expect(res.isRemote).toBe(true);
    expect(res.deviceFound).toBe(false);
    expect(res.targetDeviceId).toBeNull();
  });

  it('formats remote response with execution badge and elapsed time', () => {
    const payload = {
      action: 'rpc_response',
      request_id: 'req-123',
      status: 'success',
      result: '已读取 1024 字节内容',
      elapsed_ms: 1200,
      executor_device: 'ThinkPad X1',
    };
    const formatted = formatRemoteResponse(payload);
    expect(formatted.content).toBe('已读取 1024 字节内容');
    expect(formatted.elapsed_ms).toBe(1200);
    expect(formatted.executor_device).toBe('ThinkPad X1');
    expect(formatted.is_error).toBe(false);
  });
});
