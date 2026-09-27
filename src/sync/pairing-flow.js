/**
 * 生产级配对邀请解析与工作流编排模块 (Production Pairing Flow Orchestrator)
 * 供 SettingsConnections.vue 与 pairing-flow.test.js 统一消费
 */

/**
 * 生产级配对邀请解析函数 (Pure Parser)
 * 支持 JSON 载荷与 bob://pair? (以及 http/https) 扫码 URL 格式
 */
export function parsePairingInvitation(rawCode) {
  const raw = (rawCode || '').trim();
  if (!raw) {
    throw new Error('Empty pairing code');
  }

  if (raw.startsWith('{')) {
    try {
      const payload = JSON.parse(raw);
      const version = payload.protocol_version || payload.v;
      if (!version || typeof version !== 'string' || !version.trim()) {
        throw new Error('Missing or empty protocol version');
      }
      const v = version.trim();
      if (v !== '0.9.6-sec01' && v !== '0.9.7-sec01') {
        throw new Error(`Unsupported protocol version: ${version}`);
      }
      payload.protocol_version = v;

      if (!payload.device_id && payload.issuer_device_id) {
        payload.device_id = payload.issuer_device_id;
      } else if (!payload.issuer_device_id && payload.device_id) {
        payload.issuer_device_id = payload.device_id;
      }
      if (!payload.invitation_id || !payload.secret || !payload.issuer_device_id) {
        throw new Error('Pairing invitation missing critical credentials');
      }
      return payload;
    } catch (e) {
      throw new Error(`Failed to parse JSON pairing invitation: ${e.message || e}`);
    }
  } else if (raw.startsWith('bob://pair') || raw.startsWith('http://') || raw.startsWith('https://')) {
    const qIndex = raw.indexOf('?');
    if (qIndex === -1) {
      throw new Error('Missing query parameters in pairing URL');
    }
    const params = new URLSearchParams(raw.slice(qIndex + 1));
    const version = params.get('v');
    if (!version || !version.trim()) {
      throw new Error('Missing or empty protocol version');
    }
    const v = version.trim();
    if (v !== '0.9.6-sec01' && v !== '0.9.7-sec01') {
      throw new Error(`Unsupported protocol version: ${version}`);
    }
    const issuer_id = params.get('iss') || params.get('dev') || '';
    const secret = params.get('sec') || '';
    const invitation_id = params.get('id') || '';
    const relay = params.get('rly') || 'wss://relay.bobbik.org';
    const ips = (params.get('ips') || '').split(',').map(s => s.trim()).filter(Boolean);
    const port = parseInt(params.get('p') || '3722', 10);

    if (!invitation_id || !secret || !issuer_id) {
      throw new Error('Pairing URL missing critical credentials');
    }

    return {
      protocol_version: version.trim(),
      device_id: issuer_id,
      issuer_device_id: issuer_id,
      public_key: issuer_id,
      invitation_id,
      secret,
      local_ips: ips,
      port,
      relay,
    };
  }
  throw new Error('Unrecognized pairing invitation format');
}

/**
 * 兼容旧名称别名导出
 */
export const parsePairingCode = parsePairingInvitation;

/**
 * 生产级移动端配对工作流编排器 (Production Pairing Flow Orchestrator)
 */
export async function executePairingWorkflow(rawCode, optsOrApi = {}) {
  const options = optsOrApi && typeof optsOrApi.sec01PairDevice === 'function'
    ? { appAPI: optsOrApi }
    : (optsOrApi || {});
  const { appAPI, onStepUpdate = () => {}, timeoutMs = 30000 } = options;

  if (!appAPI || typeof appAPI.sec01PairDevice !== 'function') {
    throw new Error('appAPI.sec01PairDevice is required');
  }

  // Step 1: 解析邀请
  onStepUpdate('parse', 'running');
  const payload = parsePairingInvitation(rawCode);
  onStepUpdate('parse', 'done');

  // Step 2: 执行 SEC-01 持钥验证与配对握手 (PoP Exchange)
  onStepUpdate('relay_connect', 'running');
  let pairResult;
  try {
    const pairTimeout = new Promise((_, reject) =>
      setTimeout(() => reject(new Error('Pairing handshake timed out')), timeoutMs)
    );
    pairResult = await Promise.race([
      appAPI.sec01PairDevice(rawCode),
      pairTimeout,
    ]);

    // 强校验 P1 契约：必须受信任且会话严格非空
    if (!pairResult || pairResult.status !== 'trusted' || !pairResult.session_id) {
      throw new Error('Pairing returned incomplete or untrusted credentials (missing valid session_id)');
    }
    if (pairResult.target_device_id !== payload.issuer_device_id) {
      throw new Error('Pairing response does not match the invited device');
    }
    onStepUpdate('relay_connect', 'done', pairResult.transport || 'lan');
  } catch (pairErr) {
    onStepUpdate('relay_connect', 'error', pairErr.message || String(pairErr));
    return {
      success: false,
      paired: false,
      applied: false,
      pending_apply: false,
      stage: 'failed',
      error: pairErr.message || String(pairErr),
      savedConfig: false,
      syncTriggered: false,
    };
  }

  // Step 3: PC 消费建立会话成功后，手机端落盘端点配置 (Fail-Closed)
  onStepUpdate('save_config', 'running');
  try {
    const saved = await appAPI.setConfig('pairing_payload', payload);
    if (saved === false) {
      throw new Error('Pairing endpoint configuration was not saved');
    }
    onStepUpdate('save_config', 'done');
  } catch (cfgErr) {
    onStepUpdate('save_config', 'error', cfgErr.message || String(cfgErr));
    return {
      success: false,
      paired: false,
      applied: false,
      pending_apply: false,
      stage: 'failed',
      error: cfgErr.message || String(cfgErr),
      savedConfig: false,
      syncTriggered: false,
    };
  }

  // Step 4: 启动首次数据同步 (Initial Sync)
  // 必须严格区分：此时“设备已配对”已成立，但“首次同步是否生效”需独立判定
  let syncTriggered = false;
  let syncStage = 'skipped';
  let syncError = null;
  let isPendingApply = false;
  let pendingReasons = [];
  let rawSyncOutcome = null;

  if (appAPI.triggerMobileSync) {
    const isLan = pairResult.transport === 'lan';
    const syncStepId = isLan ? 'lan_sync' : 'relay_sync';
    onStepUpdate(syncStepId, 'running');
    try {
      const syncTimeout = new Promise((_, reject) =>
        setTimeout(() => reject(new Error('Initial sync timed out')), 45000)
      );
      const syncPayload = {
        ...payload,
        skip_relay: isLan,
        local_ips: isLan ? payload.local_ips : [],
      };
      rawSyncOutcome = await Promise.race([
        appAPI.triggerMobileSync(syncPayload),
        syncTimeout,
      ]);
      syncTriggered = true;

      if (rawSyncOutcome && typeof rawSyncOutcome === 'object' && rawSyncOutcome.status === 'applied') {
        syncStage = 'applied';
        onStepUpdate(syncStepId, 'done');
      } else if (rawSyncOutcome && typeof rawSyncOutcome === 'object' && rawSyncOutcome.status === 'pending_apply') {
        syncStage = 'pending_apply';
        isPendingApply = true;
        pendingReasons = rawSyncOutcome.reasons || [];
        const detailMsg = (rawSyncOutcome.reasons && rawSyncOutcome.reasons.length > 0)
          ? `配置变更已入队，待目标设备应用: ${rawSyncOutcome.reasons.join('; ')}`
          : '配置变更已入队，待目标设备应用';
        onStepUpdate(syncStepId, 'pending_apply', detailMsg);
      } else {
        syncStage = 'unexpected';
        syncError = typeof rawSyncOutcome === 'string' ? rawSyncOutcome : '非预期同步响应';
        onStepUpdate(syncStepId, 'error', syncError);
      }
    } catch (syncErr) {
      syncTriggered = true;
      syncStage = 'failed';
      syncError = syncErr.message || String(syncErr);
      onStepUpdate(syncStepId, 'error', syncError);
    }
  } else {
    syncStage = 'skipped';
    syncError = 'Missing triggerMobileSync API';
  }

  const isApplied = syncStage === 'applied';
  const isPending = syncStage === 'pending_apply';
  const isSyncSuccess = isApplied || isPending;

  return {
    success: isSyncSuccess,
    paired: true,
    applied: isApplied,
    pending_apply: isPending,
    stage: syncStage,
    error: syncError,
    reasons: pendingReasons,
    transport: pairResult.transport,
    session_id: pairResult.session_id,
    payload,
    savedConfig: true,
    syncTriggered,
  };
}

/**
 * 生产状态机：应用配对步骤状态更新并执行迟到事件防护 (Late-arriving event guard)
 * 1. 处于 error 终态时，丢弃任何迟到的非 error 进度信号 (done, running, pending)
 * 2. 处于 pending_apply 状态时，丢弃迟到的 done / running / pending 信号，防止虚报已应用
 * 3. 正常状态按序推进并记录 detail
 */
export function applyStepTransition(steps, id, status, detail) {
  if (!Array.isArray(steps)) return false;
  const step = steps.find(s => s && s.id === id);
  if (!step) return false;

  // 迟到事件防护：已处于终态或关键阶段时，不被低优先级或回退事件覆盖
  if (step.status === 'error' && status !== 'error') {
    return false;
  }
  if (step.status === 'pending_apply' && (status === 'done' || status === 'running' || status === 'pending')) {
    return false;
  }

  step.status = status;
  if (detail !== undefined) {
    step.detail = detail;
  }
  return true;
}

/**
 * 生产处理函数：启动自动同步回执判定与状态收口
 * 严格契约：仅明确的 status === 'applied' 视为成功并更新同步时间戳；
 * pending_apply 标记为 pending 且绝不更新时间戳；
 * 空值、非对象、未知状态或缺少明确 applied 视为错误，阻断更新时间戳并标记 error。
 */
export function handleStartupSyncOutcome(res, options = {}) {
  const {
    now = Date.now(),
    setLastSyncStatus = () => {},
    setLastSyncTime = () => {},
    storage = (typeof localStorage !== 'undefined' ? localStorage : null),
  } = options;

  if (res && typeof res === 'object' && res.status === 'applied') {
    setLastSyncStatus('success');
    setLastSyncTime(now.toString());
    if (storage) {
      storage.setItem('bob-last-sync-status', 'success');
      storage.setItem('bob-last-sync-time', now.toString());
    }
    return { status: 'applied', success: true, updatedTime: true };
  }

  if (res && typeof res === 'object' && res.status === 'pending_apply') {
    setLastSyncStatus('pending');
    if (storage) {
      storage.setItem('bob-last-sync-status', 'pending');
    }
    return { status: 'pending_apply', success: false, updatedTime: false };
  }

  // 非预期、空回执或未识别状态：必须作为错误处理，不得更新同步时间
  setLastSyncStatus('error');
  if (storage) {
    storage.setItem('bob-last-sync-status', 'error');
  }
  return {
    status: 'error',
    success: false,
    updatedTime: false,
    error: `Unexpected startup sync outcome: ${JSON.stringify(res)}`,
  };
}
