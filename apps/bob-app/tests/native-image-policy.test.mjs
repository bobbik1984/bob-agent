import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
test('native verified previews allow local blobs without broad remote images',()=>{
 const config=JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json',import.meta.url),'utf8'));
 const image=config.app.security.csp.split(';').find(x=>x.trim().startsWith('img-src')).trim().split(/\s+/).slice(1);
 assert.deepEqual(image,["'self'",'data:','blob:']);
});
