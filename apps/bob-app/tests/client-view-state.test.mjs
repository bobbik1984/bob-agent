import { test } from 'node:test';
import assert from 'node:assert/strict';
import { loadClientState, saveClientState, chooseStartupConversation } from '../src/client-view-state.mjs';
function storage() { const data = new Map(); return { getItem: k => data.get(k), setItem: (k,v) => data.set(k,v) }; }
test('first visit ignores server/import selection; explicit client choice restores', () => {
 const catalog = { selected_conversation_id: 'snake', conversations: [{id:'snake'}, {id:'today'}] };
 assert.equal(chooseStartupConversation({}, catalog), null);
 assert.equal(chooseStartupConversation({selectedConversation:'today'}, catalog), 'today');
 assert.equal(chooseStartupConversation({selectedConversation:'deleted'}, catalog), null);
 assert.equal(chooseStartupConversation({selectedConversation:'old'}, {conversations:[{id:'old',archived_at:10}]}), null);
});
test('clients keep independent selection and capture association through reload', () => {
 const a=storage(), b=storage();
 const state={selectedConversation:'today',quickNote:{text:'Idea',project:'p',title:'',pending:{id:'stable'}}};
 assert.equal(saveClientState(state,a),true);
 assert.deepEqual(loadClientState(a),state);
 assert.deepEqual(loadClientState(b),{});
});
test('broken or unavailable storage does not prevent startup', () => {
 assert.deepEqual(loadClientState({getItem(){throw Error();}}),{});
 assert.deepEqual(loadClientState({getItem(){return 'null';}}),{});
 assert.equal(saveClientState({}, {setItem(){throw Error();}}),false);
});

test('product templates and built-in translations do not introduce decorative emoji', async () => {
 const {readFile,readdir} = await import('node:fs/promises');
 const root = new URL('../src/', import.meta.url);
 async function visit(dir) {
  for(const e of await readdir(dir,{withFileTypes:true})) {
   const file=new URL(e.name+(e.isDirectory()?'/':''),dir);
   if(e.isDirectory()) await visit(file);
   else if(e.name.endsWith('.vue') || (dir.href.includes('/locales/') && e.name.endsWith('.json'))) {
    const source=await readFile(file,'utf8');
    assert.equal(/[\u{1F000}-\u{1FAFF}]/u.test(source),false,file.pathname);
   }
  }
 }
 await visit(root);
});
