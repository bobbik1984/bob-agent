<script>
import { defineComponent, h } from 'vue';
import { useI18n } from 'vue-i18n';
import { parseMessageMarkdown } from '../message-markdown.mjs';

export default defineComponent({
  props: { content: { type: String, default: '' }, plain: Boolean },
  setup(props) {
    const { t } = useI18n();
    function renderNode(node) {
      if (typeof node === 'string') return node;
      if (node.tag === 'reference' || node.tag === 'image-reference') {
        const label = node.label || node.url;
        if (node.href) return h('a', { href:node.href, target:'_blank', rel:'noopener noreferrer', referrerpolicy:'no-referrer' },
          node.tag === 'image-reference' ? t('messageImageLink', { name:label }) : label);
        return h('span', { class:'message-file-reference', title:node.url }, [
          label, ' ', h('small', t('messageFileUnavailable')), ' ', h('code', node.url),
        ]);
      }
      if (node.tag === 'table-wrap') return h('div', { class:'message-table-scroll', tabindex:0, role:'region', 'aria-label':t('messageTable') }, node.children.map(renderNode));
      if (node.tag === 'pre') return h('pre', { tabindex:0, 'aria-label':t('messageCode') }, node.children.map(renderNode));
      return h(node.tag, node.start === undefined ? null : {start:node.start}, node.children?.map(renderNode));
    }
    return () => props.plain
      ? h('div', {class:'message-body'}, props.content)
      : h('div', {class:'message-body message-markdown'}, parseMessageMarkdown(props.content).map(renderNode));
  },
});
</script>
