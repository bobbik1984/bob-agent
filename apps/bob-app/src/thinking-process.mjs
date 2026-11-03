// Utilities for collapsing intermediate agent thinking / tool-use steps
// into a foldable "思考与过程" (Thinking & Process) section and driving
// terminal micro-animations + progressive streaming output.

export const TERMINAL_ROD_FRAMES = ['|', '/', '─', '\\'];

const TOOL_LINE_RE = /^(?:`?\[(?:Tool|Thinking):\s*[^\]]*\]`?)/i;

export function cleanStepLine(line) {
  const s = String(line ?? '')
    .replace(/^\s*[-*]\s+/, '')
    .replace(/\s+/g, ' ')
    .trim();
  return s.length > 220 ? `${s.slice(0, 217)}...` : s;
}

export function extractThinkingAndBody(rawContent, extraSteps = []) {
  const steps = Array.isArray(extraSteps)
    ? extraSteps.map(cleanStepLine).filter(Boolean)
    : [];
  let text = String(rawContent ?? '').replace(/\r\n?/g, '\n');

  // 1. Extract fenced ```thinking ... ``` blocks
  text = text.replace(/```thinking\s*\n([\s\S]*?)(?:```|$)/gi, (_, inner) => {
    for (const rawLine of String(inner).split('\n')) {
      const cleaned = cleanStepLine(rawLine);
      if (cleaned) steps.push(cleaned);
    }
    return '\n';
  });

  // 2. Extract <think>...</think> blocks (including unclosed <think> during streaming)
  text = text.replace(/<think>([\s\S]*?)(?:<\/think>|$)/gi, (_, inner) => {
    for (const para of String(inner).split(/\n{2,}/)) {
      const cleaned = cleanStepLine(para);
      if (cleaned) steps.push(cleaned.startsWith('`[Thinking]`') ? cleaned : `\`[Thinking]\` ${cleaned}`);
    }
    return '\n';
  });

  // 3. Separate standalone [Tool: ...] paragraphs from formal reply paragraphs
  const paragraphs = text.split(/\n{2,}/);
  const bodyParts = [];
  for (const para of paragraphs) {
    const trimmed = para.trim();
    if (!trimmed) continue;
    if (TOOL_LINE_RE.test(trimmed)) {
      steps.push(cleanStepLine(trimmed));
    } else {
      bodyParts.push(trimmed);
    }
  }

  const body = bodyParts.join('\n\n').trim();
  return {
    steps,
    body,
    isThinkingOnly: steps.length > 0 && !body,
  };
}

export function consolidateSessionMessages(messages) {
  if (!Array.isArray(messages) || messages.length === 0) return [];
  const out = [];
  let assistantRun = [];

  function flushRun() {
    if (assistantRun.length === 0) return;
    if (assistantRun.length === 1) {
      const msg = assistantRun[0];
      const parsed = extractThinkingAndBody(msg.text, msg.thinkingSteps);
      out.push({
        ...msg,
        thinkingSteps: parsed.steps,
        displayText: parsed.body,
      });
      assistantRun = [];
      return;
    }

    const items = [];
    let hasTools = false;
    for (const msg of assistantRun) {
      const parsed = extractThinkingAndBody(msg.text, msg.thinkingSteps);
      for (const st of parsed.steps) {
        items.push({ kind: 'step', value: st, msg });
        if (TOOL_LINE_RE.test(st) || st.includes('[Tool:')) hasTools = true;
      }
      if (parsed.body) {
        items.push({ kind: 'text', value: parsed.body, msg });
      }
    }

    const textOnly = items.filter(x => x.kind === 'text').map(x => x.value);
    const steps = [];
    const formalParts = [];

    if (textOnly.length === 0) {
      for (const it of items) steps.push(it.value);
    } else if (!hasTools && items.every(x => x.kind === 'text')) {
      for (const t of textOnly) formalParts.push(t);
    } else {
      const lastText = textOnly[textOnly.length - 1] || '';
      const keepCount =
        textOnly.length >= 2 && lastText.trimEnd().endsWith(':') && lastText.length < 240
          ? 2
          : 1;
      const cutoff = textOnly.length - keepCount;
      let seenText = 0;
      for (const it of items) {
        if (it.kind === 'step') {
          steps.push(it.value);
        } else {
          if (seenText < cutoff) {
            const cleaned = cleanStepLine(it.value);
            if (cleaned) steps.push(cleaned.startsWith('`[Thinking]`') ? cleaned : `\`[Thinking]\` ${cleaned}`);
          } else {
            formalParts.push(it.value);
          }
          seenText += 1;
        }
      }
    }

    const anchor = assistantRun[assistantRun.length - 1];
    const body = formalParts.join('\n\n').trim();
    out.push({
      ...anchor,
      thinkingSteps: steps,
      displayText: body,
      text: body || anchor.text,
    });
    assistantRun = [];
  }

  for (const m of messages) {
    if (!m || typeof m !== 'object') continue;
    if (m.role === 'user') {
      flushRun();
      out.push({
        ...m,
        thinkingSteps: [],
        displayText: String(m.text ?? ''),
      });
    } else {
      assistantRun.push(m);
    }
  }
  flushRun();
  return out;
}
