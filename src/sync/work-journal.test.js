import { describe, expect, it } from 'vitest';

/**
 * 模拟 WorkOverview.vue 中的 eventLabel 格式化逻辑
 */
export function formatWorkEventLabel(event, mockT) {
  const type = event?.eventType || '';
  if (type === 'project.created') return mockT('work.event_project_created');
  if (type === 'remote.instruction.executed') {
    const dev = event.payload?.executorDevice;
    return dev
      ? mockT('work.event_remote_instruction_executed_with_device', { device: dev })
      : mockT('work.event_remote_instruction_executed');
  }
  if (type === 'remote.change.approved') return mockT('work.event_remote_change_approved');
  if (type === 'remote.change.rejected') return mockT('work.event_remote_change_rejected');
  if (type === 'remote.task.cancelled') return mockT('work.event_remote_task_cancelled');
  if (type === 'change.created') return mockT('work.event_change_created');
  if (type === 'artifact.created') return mockT('work.event_artifact_created');
  if (type.endsWith('.created')) return mockT('work.event_item_created');
  if (type.endsWith('.status_changed')) return mockT('work.event_status_changed');
  if (type === 'relation.created') return mockT('work.event_relation_created');
  return type || mockT('work.empty_activity');
}

/**
 * 校验远程指令事件与变更对象的合法性
 */
export function validateRemoteWorkEvent(event) {
  if (!event.id || !event.projectId || !event.eventType || !event.actor) {
    return false;
  }
  if (!event.eventType.startsWith('remote.') && !event.eventType.includes('.')) {
    return false;
  }
  if (event.idempotencyKey && !event.idempotencyKey.includes('_')) {
    return false;
  }
  return true;
}

describe('Phase 4: Work Object, Journal & Evidence Integration', () => {
  const translations = {
    'work.event_project_created': '建立项目状态',
    'work.event_remote_instruction_executed': '远程指令已执行完成',
    'work.event_remote_instruction_executed_with_device': ({ device }) => `远程指令已在 ${device} 执行完成`,
    'work.event_remote_change_approved': '远程变更已批准并应用',
    'work.event_remote_change_rejected': '远程变更提案已被拒绝',
    'work.event_remote_task_cancelled': '远程任务已由移动端中止',
    'work.event_change_created': '新变更提案已生成',
    'work.event_artifact_created': '新工作产物已生成',
    'work.event_item_created': '新增工作项',
    'work.event_status_changed': '更新工作项状态',
    'work.empty_activity': '还没有项目活动',
  };

  const mockT = (key, params) => {
    const val = translations[key];
    if (typeof val === 'function') return val(params);
    return val || key;
  };

  it('formats remote.instruction.executed event with device name', () => {
    const event = {
      eventType: 'remote.instruction.executed',
      payload: {
        executorDevice: 'ThinkPad X1',
        instruction: '分析工作区架构',
        elapsedMs: 650,
      },
    };
    const label = formatWorkEventLabel(event, mockT);
    expect(label).toBe('远程指令已在 ThinkPad X1 执行完成');
  });

  it('formats remote.instruction.executed event without device fallback', () => {
    const event = {
      eventType: 'remote.instruction.executed',
      payload: {},
    };
    const label = formatWorkEventLabel(event, mockT);
    expect(label).toBe('远程指令已执行完成');
  });

  it('formats remote change approval and rejection events', () => {
    expect(formatWorkEventLabel({ eventType: 'remote.change.approved' }, mockT))
      .toBe('远程变更已批准并应用');
    expect(formatWorkEventLabel({ eventType: 'remote.change.rejected' }, mockT))
      .toBe('远程变更提案已被拒绝');
  });

  it('formats remote task cancelled event', () => {
    expect(formatWorkEventLabel({ eventType: 'remote.task.cancelled' }, mockT))
      .toBe('远程任务已由移动端中止');
  });

  it('formats change.created and artifact.created events', () => {
    expect(formatWorkEventLabel({ eventType: 'change.created' }, mockT))
      .toBe('新变更提案已生成');
    expect(formatWorkEventLabel({ eventType: 'artifact.created' }, mockT))
      .toBe('新工作产物已生成');
  });

  it('validates remote work event structure and idempotency key format', () => {
    const validEvent = {
      id: 'work_event_01',
      projectId: 'project_personal_inbox',
      eventType: 'remote.instruction.executed',
      actor: 'remote:mobile-001',
      payload: { instruction: '测试指令' },
      idempotencyKey: 'event_rpc_exec_req-123',
      createdAt: Date.now(),
    };
    expect(validateRemoteWorkEvent(validEvent)).toBe(true);

    const invalidEvent = {
      id: 'work_event_02',
      projectId: '',
      eventType: 'remote.instruction.executed',
      actor: '',
    };
    expect(validateRemoteWorkEvent(invalidEvent)).toBe(false);
  });
});
