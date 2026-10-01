export function registerToolConfirmListener({ listen, invoke, showConfirm }) {
  return listen('tool:confirm_required', async (event) => {
    const { request_id, tool_name, args_preview, risk_level } = event.payload;
    const isR3 = risk_level === 'R3';

    const approved = await showConfirm({
      title: isR3 ? '⚠️ 高危操作确认' : '敏感操作确认',
      message: `Bob 请求执行${isR3 ? '高危' : '敏感'}工具：\n\n🔧 ${tool_name}\n📋 ${args_preview.substring(0, 200)}`,
      confirmText: isR3 ? '确认执行' : '允许',
      cancelText: '拒绝',
      confirmClass: isR3 ? 'btn-danger' : '',
    });

    await invoke('tool_confirm_response', {
      requestId: request_id,
      approved: !!approved,
    });
  });
}
