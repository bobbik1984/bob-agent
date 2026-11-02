import test from 'node:test';
import assert from 'node:assert/strict';
import {workspaceName,directoryGroups,extractFilterOptions,filterAndSortRows} from '../src/conversation-directory.mjs';
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

test('explicit owner/project links group across devices without merging native identities',()=>{
 const rows=[
 {uid:'a:s',remote:true,node:{id:'a',label:'A'},id:'s',workspace:'/a/game',project:'Game',project_id:'p',project_owner:'owner'},
 {uid:'b:s',remote:true,node:{id:'b',label:'B'},id:'s',workspace:'/b/game',project:'Game',project_id:'p',project_owner:'owner'},
 {uid:'c:s',remote:true,node:{id:'c',label:'C'},id:'s',workspace:'/c/game',project:'Game',project_id:'p',project_owner:'different-owner'}
 ];
 const projects=directoryGroups(rows,'project');
 assert.equal(projects.length,2);assert.equal(projects[0].items.length,2);
 assert.equal(projects[0].name,'Game');assert.equal(directoryGroups(rows,'device').length,3);
 assert.equal(rows[0].uid,'a:s');assert.equal(rows[1].uid,'b:s');
});

test('smart mode clusters related topics across generic workspaces and separates unnamed UUID sessions',()=>{
 const rows=[
  {uid:'x1t:1',remote:true,node:{id:'x1t',label:'X1T'},workspace:'/home/bobbik/dsh_projects',title:'Sea2 地球渲染效果审查与优化方案'},
  {uid:'x1t:2',remote:true,node:{id:'x1t',label:'X1T'},workspace:'/home/bobbik/dsh_projects',title:'从DOS游戏EXE还原源代码'},
  {uid:'x1t:3',remote:true,node:{id:'x1t',label:'X1T'},workspace:'/home/bobbik/dsh_projects',title:'海路纪元项目设计审查报告'},
  {uid:'x1t:4',remote:true,node:{id:'x1t',label:'X1T'},workspace:'/home/bobbik/dsh_projects',title:'session-a09f1adb-cb47-4604-a7c5-025615f'},
  {uid:'x1t:5',remote:true,node:{id:'x1t',label:'X1T'},workspace:'/home/bobbik/winter-shelter',title:'Winter Shelter_M0'},
 ];
 const smart=directoryGroups(rows,'smart',['x1t:5']);
 assert.equal(smart[0].name,'winter-shelter');
 const seaGroup=smart.find(g=>g.key==='smart:topic:sea2-hailu');
 assert.ok(seaGroup);
 assert.equal(seaGroup.items.length,2);
 const uuidGroup=smart.find(g=>g.key==='smart:unnamed-uuid');
 assert.ok(uuidGroup);
 assert.equal(uuidGroup.items.length,1);
});

test('extractFilterOptions and filterAndSortRows support multi-node, multi-harness, topic, and sort modes',()=>{
 const rows=[
  {uid:'x1t:1',remote:true,node:{id:'x1t',label:'X1T'},harness:'claude',workspace:'/home/bobbik/winter-shelter',title:'Beta Session',updated_at:'2026-10-30T10:00:00Z'},
  {uid:'vps3:1',remote:true,node:{id:'vps3',label:'VPS3'},harness:'pi',workspace:'/home/ubuntu/.bob',title:'Alpha Session',updated_at:'2026-11-02T10:00:00Z'},
  {uid:'vps3:2',remote:true,node:{id:'vps3',label:'VPS3'},harness:'codex',workspace:'/home/ubuntu/.bob',title:'Gamma Session',updated_at:'2026-11-01T10:00:00Z',project:'Bob Next',project_id:'proj-bob',project_owner:'owner'}
 ];
 const opts=extractFilterOptions(rows);
 assert.deepEqual(opts.nodes.map(n=>n.key),['remote:x1t','remote:vps3']);
 assert.deepEqual(opts.harnesses.map(h=>h.key),['claude','pi','codex']);
 assert.ok(opts.topics.some(t=>t.key==='project:proj-bob'));
 assert.ok(opts.topics.some(t=>t.key==='smart:topic:winter-shelter'));

 const onlyVps3=filterAndSortRows(rows,{selectedNodes:['remote:vps3'],sortBy:'title_asc'});
 assert.deepEqual(onlyVps3.map(r=>r.uid),['vps3:1','vps3:2']);

 const multiHarness=filterAndSortRows(rows,{selectedHarnesses:['claude','codex'],sortBy:'time_desc'});
 assert.deepEqual(multiHarness.map(r=>r.uid),['vps3:2','x1t:1']);

 const topicFiltered=directoryGroups(rows,'smart',[],{selectedTopic:'project:proj-bob'});
 assert.equal(topicFiltered.length,1);
 assert.equal(topicFiltered[0].items.length,1);
 assert.equal(topicFiltered[0].items[0].uid,'vps3:2');
});
