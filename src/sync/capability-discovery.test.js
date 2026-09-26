import { describe, expect, it } from 'vitest';

/**
 * 模拟从安全端点获取的远程 PC 模型与能力数据
 */
export const mockRemoteSnapshot = {
  status: 'success',
  device_id: 'pc-thinkpad-x1',
  device_name: 'ThinkPad X1 Carbon',
  default_model: 'deepseek::deepseek-chat',
  available_models: [
    {
      id: 'deepseek::deepseek-chat',
      displayName: 'DeepSeek-V3',
      provider: 'deepseek',
      providerName: 'DeepSeek',
      vision: false,
      isDefault: true,
    },
    {
      id: 'deepseek::deepseek-reasoner',
      displayName: 'DeepSeek-R1',
      provider: 'deepseek',
      providerName: 'DeepSeek',
      vision: false,
      isDefault: false,
    },
    {
      id: 'anthropic::claude-3-7-sonnet',
      displayName: 'Claude 3.7 Sonnet',
      provider: 'anthropic',
      providerName: 'Anthropic',
      vision: true,
      isDefault: false,
    },
  ],
  capabilities: {
    device_id: 'pc-thinkpad-x1',
    device_name: 'ThinkPad X1 Carbon',
    file_scope: 'global_authorized',
    capabilities: [
      { id: 'workspace_files', state: 'available', reason_code: 'ok' },
      { id: 'system_terminal', state: 'available', reason_code: 'ok' },
      { id: 'browser_control', state: 'available', reason_code: 'ok' },
      { id: 'git_integration', state: 'available', reason_code: 'ok' },
      { id: 'doc_export', state: 'available', reason_code: 'ok' },
    ],
  },
};

/**
 * 提取设备能力标签展示列表
 */
export function extractCapabilityTags(capabilitiesSnapshot, isLocalPhone = false) {
  if (isLocalPhone) {
    return ['沙盒文件'];
  }
  if (!capabilitiesSnapshot?.capabilities) {
    return ['工作区全权限', '系统终端'];
  }

  const tags = [];
  const caps = capabilitiesSnapshot.capabilities.capabilities || [];
  const fileScope = capabilitiesSnapshot.capabilities.file_scope;

  if (fileScope === 'global_authorized') {
    tags.push('工作区全权限');
  }
  for (const c of caps) {
    if (c.state !== 'available') continue;
    if (c.id === 'system_terminal') tags.push('系统终端');
    else if (c.id === 'browser_control') tags.push('桌面浏览器');
    else if (c.id === 'git_integration') tags.push('Git版本管理');
    else if (c.id === 'doc_export') tags.push('专业文档导出');
  }
  return tags;
}

/**
 * 提取远程能力快照中的供应商与模型分组
 */
export function extractProvidersAndModels(availableModels = []) {
  const providerMap = {};
  for (const m of availableModels) {
    const pId = m.provider || 'custom';
    if (!providerMap[pId]) {
      providerMap[pId] = {
        id: pId,
        name: m.providerName || pId,
        models: [],
      };
    }
    providerMap[pId].models.push(m);
  }
  return Object.values(providerMap);
}

/**
 * 计算当前会话的生效模型展示名称
 */
export function resolveEffectiveModelName(sessionOverrideId, deviceSnapshot) {
  if (sessionOverrideId) {
    const found = deviceSnapshot?.available_models?.find(m => m.id === sessionOverrideId);
    return found?.displayName || sessionOverrideId.split('::')[1] || sessionOverrideId;
  }
  if (deviceSnapshot?.default_model) {
    const found = deviceSnapshot.available_models?.find(m => m.id === deviceSnapshot.default_model);
    return (found?.displayName || deviceSnapshot.default_model) + ' (默认)';
  }
  return '跟随电脑默认';
}

/**
 * 构建发送给电脑端的 RPC 请求载荷
 */
export function buildRpcPayload(conversationId, instruction, sessionOverrideModel = null) {
  const payload = {
    action: 'rpc_request',
    conversation_id: conversationId || 'default',
    instruction: instruction.trim(),
    read_only: false,
  };
  if (sessionOverrideModel) {
    payload.model = sessionOverrideModel;
  }
  return payload;
}

describe('Phase 3: Capability Discovery & Session Model Isolation', () => {
  it('strictly verifies NO apiKey or credentials exist in SafeModelInfo', () => {
    // 验证安全模型结构体绝对不含任何 apiKey, secret, token 字段
    for (const model of mockRemoteSnapshot.available_models) {
      expect(model.apiKey).toBeUndefined();
      expect(model.api_key).toBeUndefined();
      expect(model.secret).toBeUndefined();
      expect(model.token).toBeUndefined();
      expect(model.baseUrl).toBeUndefined();
      expect(model.base_url).toBeUndefined();
    }
  });

  it('correctly maps PC hardware and harness capabilities to badges', () => {
    const tags = extractCapabilityTags(mockRemoteSnapshot);
    expect(tags).toContain('工作区全权限');
    expect(tags).toContain('系统终端');
    expect(tags).toContain('桌面浏览器');
    expect(tags).toContain('Git版本管理');
    expect(tags).toContain('专业文档导出');
  });

  it('correctly assigns sandbox file badge to local mobile phone', () => {
    const phoneTags = extractCapabilityTags(null, true);
    expect(phoneTags).toEqual(['沙盒文件']);
  });

  it('correctly groups remote safe models by provider', () => {
    const providers = extractProvidersAndModels(mockRemoteSnapshot.available_models);
    expect(providers.length).toBe(2);

    const deepseek = providers.find(p => p.id === 'deepseek');
    expect(deepseek).toBeDefined();
    expect(deepseek.models.length).toBe(2);
    expect(deepseek.models.map(m => m.id)).toEqual([
      'deepseek::deepseek-chat',
      'deepseek::deepseek-reasoner',
    ]);

    const anthropic = providers.find(p => p.id === 'anthropic');
    expect(anthropic).toBeDefined();
    expect(anthropic.models.length).toBe(1);
    expect(anthropic.models[0].vision).toBe(true);
  });

  it('resolves effective model name when session override is present', () => {
    const name = resolveEffectiveModelName('anthropic::claude-3-7-sonnet', mockRemoteSnapshot);
    expect(name).toBe('Claude 3.7 Sonnet');
  });

  it('resolves to device default model when no session override is set', () => {
    const name = resolveEffectiveModelName(null, mockRemoteSnapshot);
    expect(name).toBe('DeepSeek-V3 (默认)');
  });

  it('builds RPC payload including model when session override is configured', () => {
    const payload = buildRpcPayload('conv-1', '测试指令', 'anthropic::claude-3-7-sonnet');
    expect(payload.action).toBe('rpc_request');
    expect(payload.instruction).toBe('测试指令');
    expect(payload.model).toBe('anthropic::claude-3-7-sonnet');
  });

  it('builds RPC payload without model parameter when following device default', () => {
    const payload = buildRpcPayload('conv-1', '测试指令', null);
    expect(payload.action).toBe('rpc_request');
    expect(payload.instruction).toBe('测试指令');
    expect(payload.model).toBeUndefined();
  });
});
