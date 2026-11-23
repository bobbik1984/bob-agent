import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createChatScroll} from '../src/chat-scroll.mjs';
function view(){
 let top=0;const offsets=[0,300,600,900];
 const el={clientHeight:300,scrollHeight:1200,get scrollTop(){return top},set scrollTop(v){top=Math.max(0,Math.min(v,this.scrollHeight-this.clientHeight))},
  getBoundingClientRect:()=>({top:0}),addEventListener(){},removeEventListener(){},
  querySelectorAll:()=>offsets.map((x,i)=>({getAttribute:()=>String(i),getBoundingClientRect:()=>({top:offsets[i]-top,bottom:offsets[i]+300-top})}))};
 return {el,offsets};
}
test('entry and late image growth stay at latest',()=>{const {el}=view();const c=createChatScroll();c.bind(el);assert.equal(el.scrollTop,900);el.scrollHeight+=500;c.resized();assert.equal(el.scrollTop,1400);});
test('reading an older message preserves its offset during prepend and resize',()=>{const {el,offsets}=view();const c=createChatScroll();c.bind(el);c.intent();el.scrollTop=320;c.scroll();assert.equal(c.following,false);offsets.forEach((v,i)=>offsets[i]+=400);el.scrollHeight+=400;c.resized();assert.equal(el.scrollTop,720);});
test('new response does not pull reader down and arrow returns to latest',()=>{const {el}=view();let away;const c=createChatScroll(v=>away=v);c.bind(el);c.intent();el.scrollTop=300;c.scroll();el.scrollHeight+=400;c.resized();assert.equal(el.scrollTop,300);assert.equal(away,true);c.bottom();assert.equal(el.scrollTop,1300);assert.equal(away,false);});
test('hidden tabs do not erase reading intent',()=>{const {el}=view();const c=createChatScroll();c.bind(el);c.intent();el.scrollTop=300;c.scroll();el.clientHeight=0;c.resized();el.clientHeight=300;c.resized();assert.equal(el.scrollTop,300);c.reset();c.resized();assert.equal(el.scrollTop,900);});
test('detaching does not act on the previous conversation',()=>{const {el}=view();const c=createChatScroll();c.bind(el);c.dispose();el.scrollHeight+=400;c.resized();assert.equal(el.scrollTop,900);});

test('snapshot shrink cannot turn a clamped reader into automatic following',()=>{
 const {el}=view();const c=createChatScroll();c.bind(el);c.intent();el.scrollTop=600;c.scroll();
 el.scrollHeight=800;el.scrollTop=500;c.scroll();assert.equal(c.following,false);
 el.scrollHeight=1600;c.resized();assert.equal(c.following,false);assert.notEqual(el.scrollTop,1300);
});
