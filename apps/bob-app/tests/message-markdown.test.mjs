import test from 'node:test';
import assert from 'node:assert/strict';
import { parseMessageMarkdown as parse, inlineMarkdown, safeMessageUrl } from '../src/message-markdown.mjs';
test('headings, emphasis, quote and fenced code are structural nodes',()=>{
  const a=parse('# Title\n\n**bold** and `code`\n\n> quote\n\n```js\n<script>literal</script>\n```');
  assert.deepEqual(a.map(x=>x.tag),['h1','p','blockquote','pre']);
  assert.equal(a[1].children[0].tag,'strong');
  assert.equal(a[3].children[0].children[0],'<script>literal</script>');
});
test('table and nested numbered lists retain content',()=>{
  assert.equal(parse('| Name | Result |\n| --- | --- |\n| A | **OK** |')[0].tag,'table-wrap');
  const a=parse('3. three\n   - nested\n4. four')[0];
  assert.equal(a.start,3); assert.equal(a.children.length,2);
  assert.equal(a.children[0].children[1].tag,'ul');
});
test('unsafe URLs and credentials cannot become active links',()=>{
  for(const url of ['javascript:alert(1)','data:text/html,x','file:///a','//evil.test','https://u:p@example.test','https://example.test/\nfoo']) assert.equal(safeMessageUrl(url),null);
  assert.equal(safeMessageUrl('https://example.test/docs'),'https://example.test/docs');
});
test('raw HTML stays literal text, relative image is a reference',()=>{
  assert.equal(parse('<img src=x onerror=alert(1)>')[0].children[0],'<img src=x onerror=alert(1)>');
  const a=inlineMarkdown('![preview](snapshots/scene.png)')[0];
  assert.equal(a.tag,'image-reference'); assert.equal(a.href,null);
});
test('streaming unfinished fence retains all text',()=>{
  assert.equal(parse('```js\nconst x = 1;')[0].children[0].children[0],'const x = 1;');
});
test('long messages retain the final block without truncation',()=>{
  const a=parse(Array.from({length:300},(_,i)=>'## Section '+i+'\n\nBody '+i).join('\n\n')+'\n\nEND-OF-MESSAGE');
  assert.equal(a.at(-1).children[0],'END-OF-MESSAGE'); assert.equal(a.length,601);
});
