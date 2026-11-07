// Utilities for collapsing intermediate agent thinking / tool-use steps
// into foldable process sections (including interleaved Claude Code style
// tool summaries between progress paragraphs) and driving terminal
// micro-animations + progressive streaming output.

export const TERMINAL_ROD_FRAMES = ['|', '/', '-', '\\'];

const TOOL_LINE_RE = /^(?:[-*]\s+)?(?:`?\[(?:Tool|ToolResult|Thinking)(?::\s*[^\]]*)?\]`?)/i;

export const AUDIT_LINE_RE = /^(?:[-*]\s+)?(?:[\w./\\-]+(?::\d+)?)?\s*\[(?:CHECK|DOC|BREAKS|PASS|FAIL|WARN|ERROR|INFO|FIX|TODO|VERIFY|TEST|MUTATION)\]/i;

export function cleanStepLine(line) {
  const s = String(line ?? '')
    .replace(/^\s*[-*]\s+/, '')
    .replace(/\s+/g, ' ')
    .trim();
  return s.length > 220 ? `${s.slice(0, 217)}...` : s;
}

export function summarizeProcessSteps(steps, locale = 'zh-CN', translate) {
  if (!Array.isArray(steps) || steps.length === 0) return '';
  let cmd = 0;
  let read = 0;
  let edit = 0;
  let search = 0;
  let otherTool = 0;

  for (const raw of steps) {
    const s = String(raw ?? '');
    if (/\[Tool:\s*(?:Bash|sh|zsh|cmd|powershell|exec|shell|command|terminal)\b/i.test(s) || /^Execution\s+`/i.test(s)) {
      cmd += 1;
    } else if (/\[Tool:\s*(?:Read|read_file|view_file|cat)\b/i.test(s)) {
      read += 1;
    } else if (/\[Tool:\s*(?:Edit|Write|MultiEdit|edit_file|write_file|patch)\b/i.test(s)) {
      edit += 1;
    } else if (/\[Tool:\s*(?:Grep|Glob|ToolSearch|Search|WebSearch|grep|find)\b/i.test(s)) {
      search += 1;
    } else if (/\[Tool:/i.test(s)) {
      otherTool += 1;
    }
  }

  const totalTools = cmd + read + edit + search + otherTool;
  if (totalTools === 0) return '';

  if (typeof translate !== 'function') return '';
  const counts = { command: cmd, read, edit, search, other: otherTool };
  return Object.entries(counts).filter(([, count]) => count > 0)
    .map(([kind, count]) => translate(`processCall_${kind}`, { count }))
    .join(translate('processCall_separator'));
}

export function parseMessageSegments(rawContent, extraSteps = []) {
  const segments = [];
  let pendingSteps = Array.isArray(extraSteps)
    ? extraSteps.map(cleanStepLine).filter(Boolean)
    : [];
  let pendingText = [];

  const flushSteps = () => {
    if (pendingSteps.length > 0) {
      segments.push({ kind: 'steps', steps: pendingSteps });
      pendingSteps = [];
    }
  };

  const flushText = () => {
    if (pendingText.length > 0) {
      const joined = pendingText.join('\n\n').trim();
      if (joined) segments.push({ kind: 'text', text: joined });
      pendingText = [];
    }
  };

  const processPlainChunk = chunk => {
    const paragraphs = String(chunk ?? '').split(/\n{2,}/);
    for (const para of paragraphs) {
      const trimmed = para.trim();
      if (!trimmed) continue;
      if (TOOL_LINE_RE.test(trimmed)) {
        flushText();
        const cleaned = cleanStepLine(trimmed);
        if (cleaned) pendingSteps.push(cleaned);
      } else {
        const lines = trimmed.split('\n').map(l => l.trim()).filter(Boolean);
        const auditMatches = lines.filter(l => AUDIT_LINE_RE.test(l));
        if (lines.length >= 4 && auditMatches.length >= Math.ceil(lines.length * 0.8)) {
          flushSteps();
          flushText();
          segments.push({ kind: 'audit', lines });
        } else {
          flushSteps();
          pendingText.push(trimmed);
        }
      }
    }
  };

  const text = String(rawContent ?? '').replace(/\r\n?/g, '\n');
  const blockRe = /```thinking\s*\n([\s\S]*?)(?:```|$)|<think>([\s\S]*?)(?:<\/think>|$)/gi;
  let lastIndex = 0;
  let match;

  while ((match = blockRe.exec(text)) !== null) {
    if (match.index > lastIndex) {
      processPlainChunk(text.slice(lastIndex, match.index));
    }
    flushText();
    if (match[1] !== undefined) {
      for (const rawLine of String(match[1]).split('\n')) {
        const cleaned = cleanStepLine(rawLine);
        if (cleaned) pendingSteps.push(cleaned);
      }
    } else if (match[2] !== undefined) {
      for (const para of String(match[2]).split(/\n{2,}/)) {
        const cleaned = cleanStepLine(para);
        if (cleaned) {
          pendingSteps.push(cleaned.startsWith('`[Thinking]`') ? cleaned : `\`[Thinking]\` ${cleaned}`);
        }
      }
    }
    lastIndex = blockRe.lastIndex;
  }

  if (lastIndex < text.length) {
    processPlainChunk(text.slice(lastIndex));
  }

  flushSteps();
  flushText();
  return segments;
}

export function extractThinkingAndBody(rawContent, extraSteps = []) {
  const segments = parseMessageSegments(rawContent, extraSteps);
  const steps = [];
  const bodyParts = [];
  for (const seg of segments) {
    if (seg.kind === 'steps') {
      steps.push(...seg.steps);
    } else if (seg.kind === 'text' && seg.text) {
      bodyParts.push(seg.text);
    } else if (seg.kind === 'audit' && seg.lines) {
      bodyParts.push(seg.lines.join('\n'));
    }
  }
  const body = bodyParts.join('\n\n').trim();
  return {
    steps,
    body,
    segments,
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
        interleavedText: String(msg.text ?? ''),
      });
      assistantRun = [];
      return;
    }

    const allSegments = [];
    for (const msg of assistantRun) {
      const segs = parseMessageSegments(msg.text, msg.thinkingSteps);
      for (const seg of segs) {
        if (seg.kind === 'steps') {
          if (allSegments.length > 0 && allSegments[allSegments.length - 1].kind === 'steps') {
            allSegments[allSegments.length - 1].steps.push(...seg.steps);
          } else {
            allSegments.push({ kind: 'steps', steps: [...seg.steps] });
          }
        } else if (seg.kind === 'text' && seg.text) {
          allSegments.push({ kind: 'text', text: seg.text });
        }
      }
    }

    const steps = [];
    const formalParts = [];
    const interleavedChunks = [];

    for (const seg of allSegments) {
      if (seg.kind === 'steps') {
        steps.push(...seg.steps);
        const lines = seg.steps.map(s => `- ${s}`).join('\n');
        interleavedChunks.push(`\`\`\`thinking\n${lines}\n\`\`\``);
      } else if (seg.kind === 'text') {
        formalParts.push(seg.text);
        interleavedChunks.push(seg.text);
      }
    }

    const anchor = assistantRun[assistantRun.length - 1];
    const body = formalParts.join('\n\n').trim();
    const interleavedText = interleavedChunks.join('\n\n').trim();
    out.push({
      ...anchor,
      thinkingSteps: steps,
      displayText: body,
      interleavedText: interleavedText || String(anchor.text ?? ''),
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
        interleavedText: String(m.text ?? ''),
      });
    } else {
      assistantRun.push(m);
    }
  }
  flushRun();
  return out;
}
