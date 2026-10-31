import test from 'node:test';
import assert from 'node:assert/strict';
import {workspaceName,directoryGroups} from '../src/conversation-directory.mjs';
test('short path labels support POSIX, Windows and trailing separators',()=>{
 assert.equal(workspaceName('/home/user/projects/winter-shelter/'),'winter-shelter');
 assert.equal(workspaceName('C:\\projects\\winter-shelter'),'winter-shelter');
});
const session=(node,path)=>({remote:true,node:{id:node,label:node},workspace:path,title:'Session'});
test('compact labels preserve full path and node identity, without mixing equal basenames',()=>{
 const groups=directoryGroups([session('PC','/a/demo'),session('PC','/b/demo'),session('VPS','/a/demo')],'project');
 assert.equal(groups.length,3);
 assert.equal(groups[0].name,'PC / demo');
 assert.equal(groups[0].fullName,'PC / /a/demo');
 assert.equal(new Set(groups.map(g=>g.key)).size,3);
});
test('group order follows recent rows, independent sessions stay outside named groups',()=>{
 const rows=[session('PC','/a/demo'),{device:'Local',title:'Independent'},session('PC','/a/demo'),{legacy:true,device:'Local'}];
 const groups=directoryGroups(rows,'project');
 assert.deepEqual(groups.map(g=>g.items.length),[2,1]);
 assert.equal(groups[1].name,'');
 assert.equal(directoryGroups(rows,'device').length,2);
});
