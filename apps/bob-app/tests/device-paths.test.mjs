import test from 'node:test';
import assert from 'node:assert/strict';
import { deviceGroups, deviceFilterKeys } from '../src/device-paths.mjs';
import { directoryGroups } from '../src/conversation-directory.mjs';
import { createKernelBridge } from '../src/kernel-bridge.mjs';

test('same names or addresses do not merge devices without explicit identity', () => {
  const nodes = [{id:'a',name:'Laptop',baseUrl:'https://node.example'}, {id:'b',name:'Laptop',baseUrl:'https://node.example'}];
  assert.equal(deviceGroups(nodes).length, 2);
});
test('explicit device membership groups paths without changing transport IDs or credentials', () => {
  const nodes=[{id:'public',deviceId:'machine',label:'Desktop'}, {id:'vpn',deviceId:'machine',label:'VPN'}];
  const before=JSON.stringify(nodes);
  const groups=deviceGroups(nodes);
  assert.equal(groups.length,1);
  assert.deepEqual(groups[0].paths.map(n=>n.id),['public','vpn']);
  assert.equal(JSON.stringify(nodes),before);
});
test('directory groups by device but retains session identities and sources', () => {
  const rows=[{remote:true,node:{id:'a',deviceId:'machine',label:'Desktop'},id:'s',uid:'a:s',workspace:'/repo',title:'first'},
    {remote:true,node:{id:'b',deviceId:'machine',label:'Desktop'},id:'s',uid:'b:s',workspace:'/repo',title:'second'}];
  const groups=directoryGroups(rows,'device');
  assert.equal(groups.length,1);
  assert.deepEqual(new Set(groups[0].items.map(s=>s.uid)),new Set(['a:s','b:s']));
});
test('bridge sends device membership separately from the transport ID',async()=>{
  const calls=[];const bridge=createKernelBridge(async(command,args)=>{calls.push({command,args});return {};},()=>true);
  await bridge.remoteNodeSave({id:'vpn',deviceId:'machine',baseUrl:'http://node.internal'});
  assert.equal(calls[0].args.id,'vpn');assert.equal(calls[0].args.deviceId,'machine');
  assert.equal(calls[0].command,'app_remote_node_save');
});

test('saved path filters follow explicit device grouping without dropping local filters',()=>{
 assert.deepEqual(deviceFilterKeys(['remote:public','remote:vpn','local:phone'],[{id:'public',deviceId:'machine'},{id:'vpn',deviceId:'machine'}]),['remote:machine','local:phone']);
});
