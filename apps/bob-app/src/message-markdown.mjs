// Deliberately bounded Markdown subset. Never produce or accept raw HTML.
export function safeMessageUrl(value) {
  if (!/^https?:\/\//i.test(value) || /[\s\u0000-\u001f]/.test(value)) return null;
  try { const u = new URL(value); return u.username || u.password ? null : u.href; }
  catch { return null; }
}
export function inlineMarkdown(text, depth = 0) {
  if (depth > 12) return [text];
  const out = []; let cursor = 0;
  const pattern = /(`+)([^`]*?)\1|(!?)\[([^\]\n]*)\]\(([^\s)]*)\)|\*\*([^*\n]+)\*\*|__([^_\n]+)__|\*([^*\n]+)\*|~~([^~\n]+)~~/g;
  for (const m of text.matchAll(pattern)) {
    if (m.index > cursor) out.push(text.slice(cursor, m.index));
    if (m[1]) out.push({tag:'code', children:[m[2]]});
    else if (m[4] !== undefined) out.push({tag:m[3] ? 'image-reference' : 'reference', label:m[4], url:m[5], href:safeMessageUrl(m[5])});
    else out.push({tag:m[6] || m[7] ? 'strong' : m[8] ? 'em' : 's', children:inlineMarkdown(m[6] || m[7] || m[8] || m[9], depth + 1)});
    cursor = m.index + m[0].length;
  }
  if (cursor < text.length) out.push(text.slice(cursor));
  return out;
}
const listMatch = line => /^(\s*)([-+*]|\d+[.)])\s+(.*)$/.exec(line);
const cells = line => line.trim().replace(/^\|/, '').replace(/\|$/, '').split(/(?<!\\)\|/).map(s=>s.trim().replace(/\\\|/g,'|'));
const tableRule = line => line.includes('|') && cells(line).every(c=>/^:?-{3,}:?$/.test(c));
export function parseMessageMarkdown(source, depth = 0) {
  const lines = String(source ?? '').replace(/\r\n?/g,'\n').split('\n');
  if (depth > 12) return [{tag:'p',children:[lines.join('\n')]}];
  const blocks = []; let i = 0;
  const boundary = at => /^( {0,3}#{1,6}\s| {0,3}(`{3,}|~{3,})|\s*>|\s*$)/.test(lines[at]) || listMatch(lines[at]) || (at+1 < lines.length && tableRule(lines[at+1]));
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) { i++; continue; }
    const fence = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line);
    if (fence) {
      const content = []; i++;
      const close = new RegExp('^ {0,3}' + fence[1][0] + '{' + fence[1].length + ',}\\s*$');
      while(i < lines.length && !close.test(lines[i])) content.push(lines[i++]);
      if (i < lines.length) i++;
      blocks.push({tag:'pre',children:[{tag:'code',children:[content.join('\n')]}]}); continue;
    }
    const heading = /^ {0,3}(#{1,6})\s+(.+?)\s*#*\s*$/.exec(line);
    if (heading) { blocks.push({tag:'h'+heading[1].length,children:inlineMarkdown(heading[2])}); i++; continue; }
    if (/^\s*([-*_])(?:\s*\1){2,}\s*$/.test(line)) { blocks.push({tag:'hr'}); i++; continue; }
    if (/^\s*>/.test(line)) {
      const quote = []; while(i<lines.length && /^\s*>/.test(lines[i])) quote.push(lines[i++].replace(/^\s*> ?/,''));
      blocks.push({tag:'blockquote',children:parseMessageMarkdown(quote.join('\n'),depth+1)}); continue;
    }
    if (i+1<lines.length && line.includes('|') && tableRule(lines[i+1])) {
      const header = cells(line), rows = []; i+=2;
      while(i<lines.length && lines[i].trim() && lines[i].includes('|')) rows.push(cells(lines[i++]));
      blocks.push({tag:'table-wrap',children:[{tag:'table',children:[
        {tag:'thead',children:[{tag:'tr',children:header.map(c=>({tag:'th',children:inlineMarkdown(c)}))}]},
        {tag:'tbody',children:rows.map(row=>({tag:'tr',children:header.map((_,j)=>({tag:'td',children:inlineMarkdown(row[j] || '')}))}))}
      ]}]}); continue;
    }
    const list = listMatch(line);
    if (list) {
      const indent=list[1].length, ordered=/\d/.test(list[2]), items=[];
      while(i<lines.length) {
        const m=listMatch(lines[i]);
        if (!m || m[1].length!==indent || /\d/.test(m[2])!==ordered) break;
        const item=[m[3]]; i++;
        while(i<lines.length && lines[i].trim() && /^\s+/.exec(lines[i])?.[0].length>indent) {
          item.push(lines[i++].slice(indent+2));
        }
        items.push({tag:'li',children:parseMessageMarkdown(item.join('\n'),depth+1)});
      }
      blocks.push({tag:ordered?'ol':'ul',start:ordered?parseInt(list[2],10):undefined,children:items}); continue;
    }
    const paragraph=[line]; i++;
    while(i<lines.length && !boundary(i)) paragraph.push(lines[i++]);
    blocks.push({tag:'p',children:inlineMarkdown(paragraph.join('\n'))});
  }
  return blocks;
}
