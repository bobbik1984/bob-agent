import test from 'node:test';
import assert from 'node:assert/strict';
import { bindViewport } from '../src/viewport.mjs';
test('keyboard resize and pan update shell bounds and dispose listeners', () => {
  const win = new EventTarget();
  const viewport = new EventTarget();
  Object.assign(viewport, {height:800, offsetTop:0});
  Object.assign(win, {innerHeight:850, visualViewport:viewport});
  const values = new Map();
  const root = {style:{setProperty:(k,v)=>values.set(k,v),removeProperty:k=>values.delete(k)}};
  const release = bindViewport(win, root);
  assert.equal(values.get('--viewport-height'),'800px');
  Object.assign(viewport, {height:410,offsetTop:30});
  viewport.dispatchEvent(new Event('resize'));
  assert.equal(values.get('--viewport-height'),'410px');
  assert.equal(values.get('--viewport-top'),'30px');
  release();
  viewport.dispatchEvent(new Event('scroll'));
  assert.equal(values.size,0);
});
test('desktop without visualViewport uses window resize', () => {
  const win = Object.assign(new EventTarget(),{innerHeight:700});
  const values = new Map();
  const release = bindViewport(win,{style:{setProperty:(k,v)=>values.set(k,v),removeProperty:k=>values.delete(k)}});
  win.innerHeight=500; win.dispatchEvent(new Event('resize'));
  assert.equal(values.get('--viewport-height'),'500px');
  release();
});
