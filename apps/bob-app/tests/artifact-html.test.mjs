import test from 'node:test';
import assert from 'node:assert/strict';
import {artifactHtmlDocument,artifactHtmlSandbox} from '../src/artifact-html.mjs';
test('HTML is rendered as a document with inline CSS and interactive script preserved',()=>{
 const source='<html><head><style>h1{font-size:2em}</style></head><body><h1>Report</h1><button onclick="this.textContent=123">Run</button></body></html>';
 const doc=artifactHtmlDocument(source);
 assert.ok(doc.endsWith(source));
 assert.ok(doc.indexOf('Content-Security-Policy')<doc.indexOf('<html>'));
 assert.match(doc,/script-src 'unsafe-inline'/);
});
test('HTML sandbox cannot access parent origin, submit forms, or open windows',()=>{
 assert.equal(artifactHtmlSandbox,'allow-scripts');
 const doc=artifactHtmlDocument('<script>fetch("https://example.invalid")</script>');
 for(const rule of ["connect-src 'none'","form-action 'none'","frame-src 'none'","base-uri 'none'","object-src 'none'"])assert.ok(doc.includes(rule));
});

test('only exact authorized resource URLs are embedded without parsing in parent DOM',()=>{
 const html=artifactHtmlDocument('<img src="../image.png"><script src="app.js"></script><img src="https://example.invalid/other.png">',[
  {url:'../image.png',mime:'image/jpeg',data:'YWJj'},
  {url:'app.js',mime:'text/javascript',data:'YWJj'},
 ]);
 assert.ok(html.includes('src="data:image/jpeg;base64,YWJj"'));
 assert.ok(html.includes('src="data:text/javascript;base64,YWJj"'));
 assert.ok(html.includes('src="https://example.invalid/other.png"'));
});
test('invalid bundle media types and malformed base64 are not embedded',()=>{
 const html=artifactHtmlDocument('<img src="a.png"><img src="b.png">',[
  {url:'a.png',mime:'text/html',data:'YWJj'},
  {url:'b.png',mime:'image/png',data:'" onerror="bad()'},
 ]);
 assert.ok(html.includes('src="a.png"'));assert.ok(html.includes('src="b.png"'));
});
