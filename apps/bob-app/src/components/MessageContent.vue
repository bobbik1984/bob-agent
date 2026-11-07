<script>
import { defineComponent, h, inject, ref, watch, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import DshArtifactLink from './DshArtifactLink.vue';
import { parseMessageMarkdown, inlineMarkdown } from '../message-markdown.mjs';
import {
  extractThinkingAndBody,
  parseMessageSegments,
  summarizeProcessSteps,
  TERMINAL_ROD_FRAMES,
} from '../thinking-process.mjs';

export default defineComponent({
  props: {
    content: { type: String, default: '' },
    plain: Boolean,
    thinkingSteps: { type: Array, default: () => [] },
    stream: { type: Boolean, default: false },
  },
  setup(props) {
    const { t, locale } = useI18n();
    const openFile = inject('dshOpenFile', null);
    const isBrowser = typeof window !== 'undefined';

    const visibleLen = ref(props.stream && isBrowser ? 0 : Infinity);
    const rodIdx = ref(0);
    let streamTimer = null;

    function stopTimer() {
      if (streamTimer !== null) {
        clearInterval(streamTimer);
        streamTimer = null;
      }
    }

    function startStream(targetLen, reset = false) {
      if (!isBrowser || !props.stream || props.plain || targetLen <= 0) {
        stopTimer();
        visibleLen.value = Infinity;
        return;
      }
      if (reset || !Number.isFinite(visibleLen.value)) {
        visibleLen.value = Math.min(6, targetLen);
      }
      if (visibleLen.value >= targetLen) {
        stopTimer();
        visibleLen.value = Infinity;
        return;
      }
      if (streamTimer !== null) return;
      let tick = 0;
      streamTimer = setInterval(() => {
        tick += 1;
        if (tick % 4 === 0) {
          rodIdx.value = (rodIdx.value + 1) % TERMINAL_ROD_FRAMES.length;
        }
        const parsed = extractThinkingAndBody(props.content, props.thinkingSteps);
        const fullLen = parsed.body.length;
        if (!Number.isFinite(visibleLen.value) || visibleLen.value >= fullLen) {
          stopTimer();
          visibleLen.value = Infinity;
          return;
        }
        const remaining = fullLen - visibleLen.value;
        const step = Math.max(2, Math.min(18, Math.ceil(remaining / 32)));
        const next = visibleLen.value + step;
        if (next >= fullLen) {
          visibleLen.value = Infinity;
          stopTimer();
        } else {
          visibleLen.value = next;
        }
      }, 24);
    }

    function finishStream() {
      stopTimer();
      visibleLen.value = Infinity;
    }

    watch(
      () => [props.content, props.stream],
      ([newContent, newStream], [oldContent, oldStream] = []) => {
        if (!newStream || props.plain || !isBrowser) {
          finishStream();
          return;
        }
        const parsed = extractThinkingAndBody(newContent, props.thinkingSteps);
        const prevParsed =
          oldContent !== undefined ? extractThinkingAndBody(oldContent, props.thinkingSteps) : { body: '' };
        const reset = !oldStream || !prevParsed.body || !parsed.body.startsWith(prevParsed.body.slice(0, 16));
        if (reset) {
          startStream(parsed.body.length, true);
        } else if (parsed.body.length > prevParsed.body.length) {
          if (!Number.isFinite(visibleLen.value)) {
            visibleLen.value = prevParsed.body.length;
          }
          startStream(parsed.body.length, false);
        }
      },
      { immediate: true },
    );

    onUnmounted(stopTimer);

    function renderNode(node) {
      if (typeof node === 'string') return node;
      if (node.tag === 'reference' || node.tag === 'image-reference') {
        const label = node.label || node.url;
        if (node.href)
          return h(
            'a',
            { href: node.href, target: '_blank', rel: 'noopener noreferrer', referrerpolicy: 'no-referrer' },
            node.tag === 'image-reference' ? t('messageImageLink', { name: label }) : label,
          );
        if (openFile) return h(DshArtifactLink, { path: node.url, label });
        return h('span', { class: 'message-file-reference', title: node.url }, [
          label,
          ' ',
          h('small', t('messageFileUnavailable')),
          ' ',
          h('code', node.url),
        ]);
      }
      if (node.tag === 'table-wrap')
        return h(
          'div',
          { class: 'message-table-scroll', tabindex: 0, role: 'region', 'aria-label': t('messageTable') },
          node.children.map(renderNode),
        );
      if (node.tag === 'pre')
        return h('pre', { tabindex: 0, 'aria-label': t('messageCode') }, node.children.map(renderNode));
      return h(
        node.tag,
        node.start === undefined ? null : { start: node.start },
        node.children?.map(renderNode),
      );
    }

    function renderStepsBox(steps, openByDefault, keyIdx) {
      const naturalSummary = summarizeProcessSteps(steps, locale?.value || 'zh-CN', t);
      const label = naturalSummary || t('thinkingProcessCount', { count: steps.length });
      return h('details', { key: `steps-${keyIdx}`, class: 'thinking-process-box', open: openByDefault }, [
        h('summary', { class: 'thinking-process-summary' }, [
          h('span', { class: 'thinking-process-icon', 'aria-hidden': 'true' }, '⚙'),
          h('span', { class: 'thinking-process-title' }, label),
        ]),
        h(
          'ul',
          { class: 'thinking-process-list' },
          steps.map((st, idx) =>
            h('li', { key: idx, class: 'thinking-process-item' }, inlineMarkdown(st).map(renderNode)),
          ),
        ),
      ]);
    }

    return () => {
      if (props.plain) {
        return h('div', { class: 'message-body' }, props.content);
      }

      const segments = parseMessageSegments(props.content, props.thinkingSteps);
      const textSegments = segments.filter(s => s.kind === 'text');
      const totalBodyLen = textSegments.reduce(
        (acc, seg, idx) => acc + seg.text.length + (idx > 0 ? 2 : 0),
        0,
      );
      const hasAnyText = textSegments.length > 0;
      const activelyStreaming =
        isBrowser && props.stream && Number.isFinite(visibleLen.value) && visibleLen.value < totalBodyLen;

      const children = [];

      if (activelyStreaming) {
        const rod = TERMINAL_ROD_FRAMES[rodIdx.value % TERMINAL_ROD_FRAMES.length];
        children.push(
          h('div', { class: 'streaming-output-bar', role: 'status', 'aria-live': 'polite' }, [
            h('span', { class: 'terminal-rod', 'aria-hidden': 'true' }, `[\u2060${rod}\u2060]`),
            h('span', { class: 'streaming-output-label' }, String(t('streamingOutputStatus')).replace(/\s*(?:…|\.{2,})\s*$/, '')),
            h('span', { class: 'terminal-dots', 'aria-hidden': 'true' }, [
              h('span', '.'),
              h('span', '.'),
              h('span', '.'),
            ]),
            h(
              'button',
              { type: 'button', class: 'streaming-skip-btn', onClick: finishStream },
              t('streamingSkipBtn'),
            ),
          ]),
        );
      }

      let rem = activelyStreaming ? visibleLen.value : Infinity;
      let cursorPlaced = false;

      for (let i = 0; i < segments.length; i += 1) {
        const seg = segments[i];
        if (seg.kind === 'steps') {
          if (activelyStreaming && rem <= 0 && cursorPlaced) break;
          children.push(renderStepsBox(seg.steps, !hasAnyText && i === segments.length - 1, i));
        } else if (seg.kind === 'text' && seg.text) {
          if (activelyStreaming && rem <= 0) break;
          const shown =
            activelyStreaming && rem < seg.text.length ? seg.text.slice(0, Math.max(1, rem)) : seg.text;
          rem -= seg.text.length + 2;
          const mdNodes = parseMessageMarkdown(shown).map(renderNode);
          if (activelyStreaming && !cursorPlaced && rem <= 0) {
            mdNodes.push(h('span', { class: 'streaming-cursor', 'aria-hidden': 'true' }, '▋'));
            cursorPlaced = true;
          }
          children.push(h('div', { key: `text-${i}`, class: 'message-markdown-content' }, mdNodes));
        }
      }

      return h('div', { class: 'message-body message-markdown' }, children);
    };
  },
});
</script>
