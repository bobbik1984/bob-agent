import { describe, expect, it, vi } from 'vitest';
import { parsePairingCode, parsePairingInvitation, executePairingWorkflow, applyStepTransition, handleStartupSyncOutcome } from './pairing-flow.js';

describe('SEC-01 Mobile UI / Bridge Pairing Flow (Real QR & PoP)', () => {
  const validQrUrl = 'bob://pair?v=0.9.6-sec01&id=inv-uuid-1234&sec=secret-high-entropy-val&iss=pc-device-alpha&ips=192.168.1.50&p=3722';
  const validJsonCode = JSON.stringify({
    protocol_version: '0.9.6-sec01',
    invitation_id: 'inv-uuid-5678',
    secret: 'secret-json-entropy',
    issuer_device_id: 'pc-device-beta',
    local_ips: ['192.168.1.60'],
    port: 3722,
    relay: 'wss://relay.bobbik.org',
  });

  it('1. 完整 URL 邀请能正确解析所有元数据字段', () => {
    const payload = parsePairingCode(validQrUrl);
    expect(payload.invitation_id).toBe('inv-uuid-1234');
    expect(payload.secret).toBe('secret-high-entropy-val');
    expect(payload.issuer_device_id).toBe('pc-device-alpha');
    expect(payload.local_ips).toEqual(['192.168.1.50']);
    expect(payload.port).toBe(3722);
  });

  it('2. 正向建信：sec01PairDevice 成功后才调用 setConfig 保存端点并启动同步', async () => {
    const setConfigMock = vi.fn().mockResolvedValue(true);
    const triggerSyncMock = vi.fn().mockResolvedValue({ status: 'applied', transport: 'lan' });
    const sec01PairDeviceMock = vi.fn().mockResolvedValue({
      status: 'trusted',
      transport: 'lan',
      target_device_id: 'pc-device-alpha',
      session_id: 'sess-lan-999',
    });

    const mockAppAPI = {
      sec01PairDevice: sec01PairDeviceMock,
      setConfig: setConfigMock,
      triggerMobileSync: triggerSyncMock,
    };

    const result = await executePairingWorkflow(validQrUrl, mockAppAPI);

    // 验证必须把完整 rawCode 传递给 PoP 签名入口
    expect(sec01PairDeviceMock).toHaveBeenCalledWith(validQrUrl);
    expect(result.success).toBe(true);
    expect(result.paired).toBe(true);
    expect(result.applied).toBe(true);
    expect(result.stage).toBe('applied');
    expect(result.savedConfig).toBe(true);
    expect(result.syncTriggered).toBe(true);
    expect(setConfigMock).toHaveBeenCalledTimes(1);
    expect(triggerSyncMock).toHaveBeenCalledTimes(1);
    expect(triggerSyncMock).toHaveBeenCalledWith(expect.objectContaining({
      skip_relay: true,
      invitation_id: 'inv-uuid-1234',
    }));
  });

  it('3. 负向防御：PC 拒绝配对 (已被消费/过期/签名错误) 时，严禁保存配置，严禁触发同步', async () => {
    const setConfigMock = vi.fn().mockResolvedValue(true);
    const triggerSyncMock = vi.fn().mockResolvedValue(true);
    const sec01PairDeviceMock = vi.fn().mockRejectedValue(new Error('Unauthorized: 配对邀请已被消费，严禁重复使用 (防重放)'));

    const mockAppAPI = {
      sec01PairDevice: sec01PairDeviceMock,
      setConfig: setConfigMock,
      triggerMobileSync: triggerSyncMock,
    };

    const result = await executePairingWorkflow(validQrUrl, mockAppAPI);

    expect(sec01PairDeviceMock).toHaveBeenCalledWith(validQrUrl);
    expect(result.success).toBe(false);
    expect(result.savedConfig).toBe(false);
    expect(result.syncTriggered).toBe(false);
    // 关键断言：绝对不能写入端点配置，绝对不能启动同步
    expect(setConfigMock).not.toHaveBeenCalled();
    expect(triggerSyncMock).not.toHaveBeenCalled();
  });

  it('4. 负向防御：网络超时或未连接中继时，严禁保存配置与启动同步', async () => {
    const setConfigMock = vi.fn().mockResolvedValue(true);
    const triggerSyncMock = vi.fn().mockResolvedValue(true);
    const sec01PairDeviceMock = vi.fn().mockRejectedValue(new Error('ERR-PAIRING-01: Relay 后台未连接'));

    const mockAppAPI = {
      sec01PairDevice: sec01PairDeviceMock,
      setConfig: setConfigMock,
      triggerMobileSync: triggerSyncMock,
    };

    const result = await executePairingWorkflow(validJsonCode, mockAppAPI);

    expect(result.success).toBe(false);
    expect(setConfigMock).not.toHaveBeenCalled();
    expect(triggerSyncMock).not.toHaveBeenCalled();
  });

  it('5. 格式错误：非法或损坏的配对邀请无法进入建信流程', async () => {
    const sec01PairDeviceMock = vi.fn();
    const setConfigMock = vi.fn();
    const mockAppAPI = { sec01PairDevice: sec01PairDeviceMock, setConfig: setConfigMock };

    await expect(executePairingWorkflow('invalid_random_string', mockAppAPI)).rejects.toThrow();
    expect(sec01PairDeviceMock).not.toHaveBeenCalled();
    expect(setConfigMock).not.toHaveBeenCalled();
  });

  it('6. 负向防御：PC 返回缺少 session_id 或 status 非 trusted 时，严禁保存配置与启动同步', async () => {
    const setConfigMock = vi.fn().mockResolvedValue(true);
    const triggerSyncMock = vi.fn().mockResolvedValue(true);

    // Case A: status is not trusted
    const sec01PairDeviceMockA = vi.fn().mockResolvedValue({
      status: 'discovered',
      transport: 'relay',
      target_device_id: 'pc-device-alpha',
      session_id: 'sess-123',
    });

    const resultA = await executePairingWorkflow(validQrUrl, {
      sec01PairDevice: sec01PairDeviceMockA,
      setConfig: setConfigMock,
      triggerMobileSync: triggerSyncMock,
    });
    expect(resultA.success).toBe(false);
    expect(setConfigMock).not.toHaveBeenCalled();
    expect(triggerSyncMock).not.toHaveBeenCalled();

    // Case B: session_id is empty
    const sec01PairDeviceMockB = vi.fn().mockResolvedValue({
      status: 'trusted',
      transport: 'lan',
      target_device_id: 'pc-device-alpha',
      session_id: '',
    });

    const resultB = await executePairingWorkflow(validQrUrl, {
      sec01PairDevice: sec01PairDeviceMockB,
      setConfig: setConfigMock,
      triggerMobileSync: triggerSyncMock,
    });
    expect(resultB.success).toBe(false);
    expect(setConfigMock).not.toHaveBeenCalled();
    expect(triggerSyncMock).not.toHaveBeenCalled();
  });

  it('7. 负向防御：协议版本缺失、为空或不支持时全面 Fail-Closed', () => {
    // Missing v in URL
    expect(() => parsePairingInvitation('bob://pair?id=inv-1&sec=sec-1&iss=dev-1')).toThrow(/Missing or empty protocol version|缺少协议版本/);

    // Empty v in URL
    expect(() => parsePairingInvitation('bob://pair?v=&id=inv-1&sec=sec-1&iss=dev-1')).toThrow(/Missing or empty protocol version|缺少协议版本/);

    // Unsupported v in URL
    expect(() => parsePairingInvitation('bob://pair?v=999.0.0&id=inv-1&sec=sec-1&iss=dev-1')).toThrow(/Unsupported protocol version|不支持的协议版本/);

    // Missing protocol_version in JSON
    expect(() => parsePairingInvitation(JSON.stringify({
      invitation_id: 'inv-1',
      secret: 'sec-1',
      issuer_device_id: 'dev-1',
    }))).toThrow(/Missing or empty protocol version|缺少协议版本/);

    // Empty protocol_version in JSON
    expect(() => parsePairingInvitation(JSON.stringify({
      protocol_version: '   ',
      invitation_id: 'inv-1',
      secret: 'sec-1',
      issuer_device_id: 'dev-1',
    }))).toThrow(/Missing or empty protocol version|缺少协议版本/);

    // Unsupported protocol_version in JSON
    expect(() => parsePairingInvitation(JSON.stringify({
      protocol_version: 'v0.1-alpha',
      invitation_id: 'inv-1',
      secret: 'sec-1',
      issuer_device_id: 'dev-1',
    }))).toThrow(/Unsupported protocol version|不支持的协议版本/);

    // Valid v succeeds
    const parsedUrl = parsePairingInvitation('bob://pair?v=0.9.6-sec01&id=inv-1&sec=sec-1&iss=dev-1');
    expect(parsedUrl.protocol_version).toBe('0.9.6-sec01');

    const parsedJson = parsePairingInvitation(JSON.stringify({
      protocol_version: '0.9.6-sec01',
      invitation_id: 'inv-1',
      secret: 'sec-1',
      issuer_device_id: 'dev-1',
    }));
    expect(parsedJson.protocol_version).toBe('0.9.6-sec01');
  });

  it('8. 返回的设备与扫码邀请不一致时，不能保存或同步', async () => {
    const setConfig = vi.fn();
    const triggerMobileSync = vi.fn();
    const result = await executePairingWorkflow(validQrUrl, {
      sec01PairDevice: vi.fn().mockResolvedValue({
        status: 'trusted',
        transport: 'lan',
        target_device_id: 'different-device',
        session_id: 'sess-wrong-target',
      }),
      setConfig,
      triggerMobileSync,
    });
    expect(result.success).toBe(false);
    expect(result.error).toMatch(/does not match/);
    expect(setConfig).not.toHaveBeenCalled();
    expect(triggerMobileSync).not.toHaveBeenCalled();
  });

  it('9. 配置写入明确失败时，不能报告配对成功或启动同步', async () => {
    const triggerMobileSync = vi.fn();
    const result = await executePairingWorkflow(validQrUrl, {
      sec01PairDevice: vi.fn().mockResolvedValue({
        status: 'trusted',
        transport: 'lan',
        target_device_id: 'pc-device-alpha',
        session_id: 'sess-valid',
      }),
      setConfig: vi.fn().mockResolvedValue(false),
      triggerMobileSync,
    });
    expect(result.success).toBe(false);
    expect(result.savedConfig).toBe(false);
    expect(triggerMobileSync).not.toHaveBeenCalled();
  });

  it('10. 首次同步发生部分同步失败时，步骤必须更新为 error，严禁误报 done，且工作流返回 failed', async () => {
    const stepUpdates = [];
    const onStepUpdate = (step, status, detail) => {
      stepUpdates.push({ step, status, detail });
    };

    const triggerMobileSync = vi.fn().mockRejectedValue(new Error('部分同步失败: Outbox 投递失败: HTTP 500'));

    const result = await executePairingWorkflow(validQrUrl, {
      appAPI: {
        sec01PairDevice: vi.fn().mockResolvedValue({
          status: 'trusted',
          transport: 'lan',
          target_device_id: 'pc-device-alpha',
          session_id: 'sess-valid',
        }),
        setConfig: vi.fn().mockResolvedValue(true),
        triggerMobileSync,
      },
      onStepUpdate,
    });

    // 设备本身已成功完成握手与配置落盘，但首次同步失败
    expect(result.success).toBe(false);
    expect(result.paired).toBe(true);
    expect(result.applied).toBe(false);
    expect(result.pending_apply).toBe(false);
    expect(result.stage).toBe('failed');
    expect(result.error).toContain('部分同步失败');

    // 验证 lan_sync 步骤最后更新的状态必须是 error，包含详细错误，绝不能是 done
    const lanSyncUpdates = stepUpdates.filter(u => u.step === 'lan_sync');
    expect(lanSyncUpdates.length).toBeGreaterThanOrEqual(2);
    const lastUpdate = lanSyncUpdates[lanSyncUpdates.length - 1];
    expect(lastUpdate.status).toBe('error');
    expect(lastUpdate.detail).toContain('部分同步失败');
  });

  it('11. 当首次同步返回 pending_apply 时，步骤必须更新为 pending_apply，严禁更新为 done', async () => {
    const stepUpdates = [];
    const onStepUpdate = (step, status, detail) => {
      stepUpdates.push({ step, status, detail });
    };

    const triggerMobileSync = vi.fn().mockResolvedValue({
      status: 'pending_apply',
      transport: 'lan',
      reasons: ['Outbox 暂存待应用: disk wait'],
    });

    const result = await executePairingWorkflow(validQrUrl, {
      appAPI: {
        sec01PairDevice: vi.fn().mockResolvedValue({
          status: 'trusted',
          transport: 'lan',
          target_device_id: 'pc-device-alpha',
          session_id: 'sess-valid',
        }),
        setConfig: vi.fn().mockResolvedValue(true),
        triggerMobileSync,
      },
      onStepUpdate,
    });

    expect(result.success).toBe(true);
    expect(result.paired).toBe(true);
    expect(result.applied).toBe(false);
    expect(result.pending_apply).toBe(true);
    expect(result.stage).toBe('pending_apply');
    expect(result.reasons).toEqual(['Outbox 暂存待应用: disk wait']);

    const lanSyncUpdates = stepUpdates.filter(u => u.step === 'lan_sync');
    expect(lanSyncUpdates.length).toBeGreaterThanOrEqual(2);
    const lastUpdate = lanSyncUpdates[lanSyncUpdates.length - 1];
    expect(lastUpdate.status).toBe('pending_apply');
    expect(lastUpdate.detail).toContain('待目标设备应用');
    expect(stepUpdates.some(u => u.step === 'lan_sync' && u.status === 'done')).toBe(false);
  });

  it('12. 当首次同步返回 applied 时，步骤正常更新为 done，pending_apply 为 false', async () => {
    const stepUpdates = [];
    const onStepUpdate = (step, status, detail) => {
      stepUpdates.push({ step, status, detail });
    };

    const triggerMobileSync = vi.fn().mockResolvedValue({
      status: 'applied',
      transport: 'lan',
    });

    const result = await executePairingWorkflow(validQrUrl, {
      appAPI: {
        sec01PairDevice: vi.fn().mockResolvedValue({
          status: 'trusted',
          transport: 'lan',
          target_device_id: 'pc-device-alpha',
          session_id: 'sess-valid',
        }),
        setConfig: vi.fn().mockResolvedValue(true),
        triggerMobileSync,
      },
      onStepUpdate,
    });

    expect(result.success).toBe(true);
    expect(result.paired).toBe(true);
    expect(result.applied).toBe(true);
    expect(result.pending_apply).toBe(false);
    expect(result.stage).toBe('applied');

    const lanSyncUpdates = stepUpdates.filter(u => u.step === 'lan_sync');
    const lastUpdate = lanSyncUpdates[lanSyncUpdates.length - 1];
    expect(lastUpdate.status).toBe('done');
  });

  it('13. 当缺少 triggerMobileSync 接口时，标记 stage 为 skipped，严禁误报 applied', async () => {
    const result = await executePairingWorkflow(validQrUrl, {
      appAPI: {
        sec01PairDevice: vi.fn().mockResolvedValue({
          status: 'trusted',
          transport: 'lan',
          target_device_id: 'pc-device-alpha',
          session_id: 'sess-valid',
        }),
        setConfig: vi.fn().mockResolvedValue(true),
        // triggerMobileSync 未提供
      },
    });

    expect(result.success).toBe(false);
    expect(result.paired).toBe(true);
    expect(result.applied).toBe(false);
    expect(result.stage).toBe('skipped');
    expect(result.error).toContain('Missing triggerMobileSync');
  });

  it('14. 当首次同步返回非预期响应时，步骤置为 error，stage 为 unexpected', async () => {
    const stepUpdates = [];
    const triggerMobileSync = vi.fn().mockResolvedValue({ status: 'unknown_stage' });

    const result = await executePairingWorkflow(validQrUrl, {
      appAPI: {
        sec01PairDevice: vi.fn().mockResolvedValue({
          status: 'trusted',
          transport: 'lan',
          target_device_id: 'pc-device-alpha',
          session_id: 'sess-valid',
        }),
        setConfig: vi.fn().mockResolvedValue(true),
        triggerMobileSync,
      },
      onStepUpdate: (step, status, detail) => stepUpdates.push({ step, status, detail }),
    });

    expect(result.success).toBe(false);
    expect(result.paired).toBe(true);
    expect(result.applied).toBe(false);
    expect(result.stage).toBe('unexpected');
    const lanSyncUpdates = stepUpdates.filter(u => u.step === 'lan_sync');
    expect(lanSyncUpdates[lanSyncUpdates.length - 1].status).toBe('error');
  });

  it('15. 迟到事件防护：applyStepTransition 生产状态机中，步骤处于 error 或 pending_apply 时，不能被迟到的 done/running 覆盖', () => {
    // 真实生产步骤数组 (与 SettingsConnections.vue pairingSteps 相同结构)
    const steps = [
      { id: 'lan_sync', status: 'error', detail: '连接超时' },
      { id: 'relay_sync', status: 'pending_apply', detail: '待目标设备应用' },
    ];

    // 迟到的 done 试图覆盖 error -> 应当被丢弃并返回 false
    const res1 = applyStepTransition(steps, 'lan_sync', 'done', '同步完成');
    expect(res1).toBe(false);
    expect(steps.find(s => s.id === 'lan_sync').status).toBe('error');
    expect(steps.find(s => s.id === 'lan_sync').detail).toBe('连接超时');

    // 迟到的 running 试图覆盖 pending_apply -> 应当被丢弃并返回 false
    const res2 = applyStepTransition(steps, 'relay_sync', 'running', '重新传输中');
    expect(res2).toBe(false);
    expect(steps.find(s => s.id === 'relay_sync').status).toBe('pending_apply');

    // 迟到的 done 试图覆盖 pending_apply -> 应当被丢弃并返回 false
    const res3 = applyStepTransition(steps, 'relay_sync', 'done', '同步就绪');
    expect(res3).toBe(false);
    expect(steps.find(s => s.id === 'relay_sync').status).toBe('pending_apply');
    expect(steps.find(s => s.id === 'relay_sync').detail).toBe('待目标设备应用');

    // 合法的状态推进应返回 true
    const res4 = applyStepTransition(steps, 'relay_sync', 'error', '后续确认失败');
    expect(res4).toBe(true);
    expect(steps.find(s => s.id === 'relay_sync').status).toBe('error');
    expect(steps.find(s => s.id === 'relay_sync').detail).toBe('后续确认失败');
  });

  it('16. 启动自动同步回执判定：仅明确 status === "applied" 视为成功并更新同步时间与状态', () => {
    let currentStatus = '';
    let currentTime = '';
    const fakeStorage = {
      store: {},
      setItem(k, v) { this.store[k] = v; },
      getItem(k) { return this.store[k]; }
    };
    const fixedNow = 1711324800000;

    const outcome = handleStartupSyncOutcome(
      { status: 'applied', version: 42 },
      {
        now: fixedNow,
        setLastSyncStatus: (s) => { currentStatus = s; },
        setLastSyncTime: (t) => { currentTime = t; },
        storage: fakeStorage,
      }
    );

    expect(outcome.success).toBe(true);
    expect(outcome.status).toBe('applied');
    expect(outcome.updatedTime).toBe(true);
    expect(currentStatus).toBe('success');
    expect(currentTime).toBe(fixedNow.toString());
    expect(fakeStorage.getItem('bob-last-sync-status')).toBe('success');
    expect(fakeStorage.getItem('bob-last-sync-time')).toBe(fixedNow.toString());
  });

  it('17. 启动自动同步回执判定：pending_apply 标记为 pending 状态，绝不更新同步时间', () => {
    let currentStatus = '';
    let currentTime = 'prior-time';
    const fakeStorage = {
      store: { 'bob-last-sync-time': 'prior-time' },
      setItem(k, v) { this.store[k] = v; },
      getItem(k) { return this.store[k]; }
    };

    const outcome = handleStartupSyncOutcome(
      { status: 'pending_apply', reasons: ['target_offline'] },
      {
        now: 1711324800000,
        setLastSyncStatus: (s) => { currentStatus = s; },
        setLastSyncTime: (t) => { currentTime = t; },
        storage: fakeStorage,
      }
    );

    expect(outcome.success).toBe(false);
    expect(outcome.status).toBe('pending_apply');
    expect(outcome.updatedTime).toBe(false);
    expect(currentStatus).toBe('pending');
    expect(currentTime).toBe('prior-time'); // 未被更新
    expect(fakeStorage.getItem('bob-last-sync-status')).toBe('pending');
    expect(fakeStorage.getItem('bob-last-sync-time')).toBe('prior-time'); // 存储未被更新
  });

  it('18. 启动自动同步回执判定：空值、非对象、未知状态或非 applied 均判定为 error，绝不更新时间', () => {
    const testCases = [
      null,
      undefined,
      {},
      { status: 'unknown_stage' },
      { status: 'ok' }, // 缺少明确 applied
      'applied', // 字符串而非标准对象
    ];

    for (const testCase of testCases) {
      let currentStatus = '';
      let currentTime = 'prior-time';
      const fakeStorage = {
        store: { 'bob-last-sync-time': 'prior-time' },
        setItem(k, v) { this.store[k] = v; },
        getItem(k) { return this.store[k]; }
      };

      const outcome = handleStartupSyncOutcome(
        testCase,
        {
          now: 1711324800000,
          setLastSyncStatus: (s) => { currentStatus = s; },
          setLastSyncTime: (t) => { currentTime = t; },
          storage: fakeStorage,
        }
      );

      expect(outcome.success).toBe(false);
      expect(outcome.status).toBe('error');
      expect(outcome.updatedTime).toBe(false);
      expect(currentStatus).toBe('error');
      expect(currentTime).toBe('prior-time'); // 严禁更新
      expect(fakeStorage.getItem('bob-last-sync-status')).toBe('error');
      expect(fakeStorage.getItem('bob-last-sync-time')).toBe('prior-time'); // 严禁更新
    }
  });
});

