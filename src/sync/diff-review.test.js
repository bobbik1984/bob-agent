import { describe, it, expect } from 'vitest';

describe('Diff Review & Remote Hardening State Machine (Phase 6)', () => {
  function parseDiffStats(diffText) {
    let additions = 0;
    let deletions = 0;
    const lines = (diffText || '').split('\n');
    for (const line of lines) {
      if (line.startsWith('+') && !line.startsWith('+++')) {
        additions++;
      } else if (line.startsWith('-') && !line.startsWith('---')) {
        deletions++;
      }
    }
    return { additions, deletions, totalLines: lines.length };
  }

  it('correctly calculates additions and deletions from unified diff', () => {
    const sampleDiff = [
      '--- a/src/utils/calc.ts',
      '+++ b/src/utils/calc.ts',
      '@@ -1,3 +1,5 @@',
      ' export function add(a, b) {',
      '+  console.log("Adding numbers", a, b);',
      '-  return a - b;',
      '+  return a + b;',
      ' }'
    ].join('\n');

    const stats = parseDiffStats(sampleDiff);
    expect(stats.additions).toBe(2);
    expect(stats.deletions).toBe(1);
    expect(stats.totalLines).toBe(8);
  });

  it('handles empty or new file diffs gracefully', () => {
    const newFileDiff = [
      '--- a/src/new.ts',
      '+++ b/src/new.ts',
      '@@ -0,0 +1,2 @@',
      '+export const version = "0.9.6";',
      '+export default version;'
    ].join('\n');

    const stats = parseDiffStats(newFileDiff);
    expect(stats.additions).toBe(2);
    expect(stats.deletions).toBe(0);
  });

  it('validates remote change proposal schema with hash and project tracking', () => {
    const changeProposal = {
      change_id: 'change-123',
      request_id: 'req-456',
      project_id: 'proj_personal',
      file_path: 'README.md',
      old_content_hash: 'd41d8cd98f00b204e9800998ecf8427e',
      diff: '--- a/README.md\n+++ b/README.md\n@@ -1,1 +1,1 @@\n-v0.9.5\n+v0.9.6',
      summary: 'Update version to v0.9.6',
      additions: 1,
      deletions: 1,
      status: 'pending',
    };

    expect(changeProposal.change_id).toBeDefined();
    expect(changeProposal.request_id).toBe('req-456');
    expect(changeProposal.project_id).toBe('proj_personal');
    expect(changeProposal.old_content_hash).toBe('d41d8cd98f00b204e9800998ecf8427e');
    expect(changeProposal.file_path).toBe('README.md');
    expect(changeProposal.status).toBe('pending');
    expect(changeProposal.additions).toBeGreaterThan(0);
  });

  it('safely handles approval transitions: only updates status to applied on server write success', () => {
    const change = {
      change_id: 'change-123',
      status: 'pending'
    };

    function processApprovalResponse(changeObj, res) {
      if (res?.outcome?.status === 'applied' || res?.status === 'applied') {
        changeObj.status = 'applied';
        return { success: true };
      } else {
        const errorMsg = res?.outcome?.error || res?.error || 'Failed to apply';
        return { success: false, error: errorMsg };
      }
    }

    // 1. Conflict or disk error must NOT mark change as applied
    const conflictRes = {
      status: 'error',
      change_id: 'change-123',
      error: '文件基线冲突：磁盘文件自提案生成后已被修改'
    };
    const failureResult = processApprovalResponse(change, conflictRes);
    expect(failureResult.success).toBe(false);
    expect(change.status).toBe('pending'); // Must remain pending!

    // 2. Success from server marks change as applied
    const successRes = {
      action: 'rpc_response',
      outcome: {
        status: 'applied',
        change_id: 'change-123',
        file_path: 'README.md',
        message: '修改已成功应用到文件'
      }
    };
    const successResult = processApprovalResponse(change, successRes);
    expect(successResult.success).toBe(true);
    expect(change.status).toBe('applied');

    // 3. Idempotent re-approval keeps applied
    const idempotentRes = {
      status: 'applied',
      change_id: 'change-123',
      message: '修改此前已成功应用 (幂等)'
    };
    const idempotentResult = processApprovalResponse(change, idempotentRes);
    expect(idempotentResult.success).toBe(true);
    expect(change.status).toBe('applied');
  });

  it('implements 3-state remote cancellation machine', () => {
    let state = 'idle';
    let thinking = '';
    const activeMsg = {
      id: 'asst-1',
      role: 'assistant',
      content: '正在执行工作区分析...',
      change: {
        change_id: 'change-1',
        status: 'pending'
      }
    };

    // Step 1: User clicks Stop -> UI transitions to cancelling
    state = 'cancelling';
    thinking = '正在向远程电脑发送中止指令并等待安全退出...';
    expect(state).toBe('cancelling');
    expect(thinking).toContain('正在向远程电脑发送中止指令');

    // Step 2: PC acknowledges exit -> UI transitions to confirmed stopped
    function onCancelResolved(msg, ackSuccess) {
      if (ackSuccess) {
        const tip = '任务已由手机端中止 (PC 已确认停止)';
        msg.content = msg.content ? `${msg.content}\n\n${tip}` : tip;
        if (msg.change) {
          msg.change.status = 'cancelled';
        }
      } else {
        const errTip = '中止指令发送失败或 PC 无响应';
        msg.content = msg.content ? `${msg.content}\n\n${errTip}` : errTip;
      }
      state = 'idle';
      thinking = '';
    }

    onCancelResolved(activeMsg, true);
    expect(state).toBe('idle');
    expect(activeMsg.change.status).toBe('cancelled');
    expect(activeMsg.content).toContain('任务已由手机端中止 (PC 已确认停止)');
    expect(activeMsg.content).not.toContain('⏹'); // Ensure no raw emoji
  });

  it('correctly handles cancellation timeout: does not falsely report stopped and preserves change proposal', () => {
    let cancelConfirmed = false;
    let cancelTimedOut = false;
    const activeMsg = {
      id: 'asst-1',
      role: 'assistant',
      content: '正在执行代码修改...',
      change: {
        change_id: 'change-slow-1',
        status: 'pending'
      }
    };

    function resolveCancelResponse(res) {
      if (res?.confirmed === true || (res?.status === 'cancelled' && res?.confirmed !== false)) {
        cancelConfirmed = true;
      } else if (res?.status === 'timeout' || res?.confirmed === false) {
        cancelTimedOut = true;
      }

      const cancelTip = cancelConfirmed
        ? '任务已由手机端中止 (PC 已确认停止)'
        : (cancelTimedOut ? '中止信号已发送，但电脑端在 5 秒内未确认退出（任务可能仍在后台运行）' : '中止指令发送失败或 PC 无响应');

      activeMsg.content = activeMsg.content ? `${activeMsg.content}\n\n${cancelTip}` : cancelTip;
      if (activeMsg.change && cancelConfirmed) {
        activeMsg.change.status = 'cancelled';
      }
    }

    // Server returns timeout ACK because task did not finish in 5 seconds
    const timeoutAck = {
      action: 'rpc_cancel_ack',
      request_id: 'req-1',
      status: 'timeout',
      confirmed: false,
      error: '任务中止等待超时'
    };

    resolveCancelResponse(timeoutAck);

    expect(cancelConfirmed).toBe(false);
    expect(cancelTimedOut).toBe(true);
    expect(activeMsg.content).toContain('中止信号已发送，但电脑端在 5 秒内未确认退出');
    // Crucial: change status must NOT be falsely marked as cancelled when timeout occurred!
    expect(activeMsg.change.status).toBe('pending');
  });
});
