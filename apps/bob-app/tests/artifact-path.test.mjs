import test from 'node:test';
import assert from 'node:assert/strict';
import {artifactPath} from '../src/artifact-path.mjs';
test('node-local file links normalize before preview and cache lookup',()=>{
 assert.equal(artifactPath('file:///workspace/space%20name.txt'),'/workspace/space name.txt');
 assert.equal(artifactPath('docs/test.md:12:4'),'docs/test.md');
 for(const value of ['file://host/workspace/a.txt','file:///workspace/a.txt?token=x','file:///workspace/a.txt#x','file:///workspace/%00.txt','file:///workspace/%XX.txt'])assert.equal(artifactPath(value),value);
});
