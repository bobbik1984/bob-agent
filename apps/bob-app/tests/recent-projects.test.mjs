import test from 'node:test';
import assert from 'node:assert/strict';
import {markRecentProject,isRecentProject,RECENT_PROJECT_TTL} from '../src/recent-projects.mjs';
test('old catalog projects never become recent on refresh',()=>{
 assert.equal(isRecentProject('existing',100),false);
});
test('explicit creation remains across drawer remount and expires',()=>{
 markRecentProject('new',100);
 assert.equal(isRecentProject('new',101),true);
 assert.equal(isRecentProject('new',100+RECENT_PROJECT_TTL-1),true);
 assert.equal(isRecentProject('new',100+RECENT_PROJECT_TTL),false);
});
test('clock reversal does not keep a temporary project indefinitely',()=>{
 markRecentProject('clock',100);
 assert.equal(isRecentProject('clock',99),false);
});
