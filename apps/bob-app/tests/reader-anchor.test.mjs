import test from 'node:test';
import assert from 'node:assert/strict';
import {captureReaderAnchor,restoreReaderAnchor} from '../src/reader-anchor.mjs';
test('image growth retains the paragraph being read',()=>{
 let y=20;const anchor={isConnected:true,getBoundingClientRect:()=>({top:y,bottom:y+200})};
 const reader={isConnected:true,scrollTop:300,scrollHeight:1000,clientHeight:200,getBoundingClientRect:()=>({top:10}),querySelectorAll:()=>[anchor]};
 const saved=captureReaderAnchor(reader);y+=180;restoreReaderAnchor(saved);assert.equal(reader.scrollTop,480);
});
test('reader at newest message follows image growth; detached reader is ignored',()=>{
 const reader={isConnected:true,scrollTop:800,scrollHeight:1000,clientHeight:200,getBoundingClientRect:()=>({top:0}),querySelectorAll:()=>[]};
 const saved=captureReaderAnchor(reader);reader.scrollHeight=1300;restoreReaderAnchor(saved);assert.equal(reader.scrollTop,1300);
 reader.isConnected=false;reader.scrollHeight=1500;restoreReaderAnchor(saved);assert.equal(reader.scrollTop,1300);
});
