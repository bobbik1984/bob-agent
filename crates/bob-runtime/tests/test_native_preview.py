import unittest,tempfile,importlib.util,os,base64
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('probe',Path(__file__).parents[1]/'src/agy_index_probe.py')
p=importlib.util.module_from_spec(spec);spec.loader.exec_module(p)
@unittest.skipUnless(hasattr(os,'O_NOFOLLOW') and os.open in os.supports_dir_fd,'requires node POSIX filesystem')
class NativePreviewTests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name).resolve();(self.root/'docs').mkdir()
  (self.root/'docs/view.html').write_text('<h1>Review</h1>',encoding='utf-8')
  self.cfg={'sessionId':'codex:12345678-1234-1234-1234-123456789abc','path':'docs/view.html'}
  self.page={'workspace':str(self.root),'messages':[{'role':'assistant','text':'[Review](docs/view.html)'}]}
  self.mock=patch.object(p,'read_session_page',return_value=self.page);self.mock.start()
 def tearDown(self):self.mock.stop();self.tmp.cleanup()
 def test_relative_absolute_and_cache(self):
  result=p.preview_native_file(self.cfg)
  self.assertEqual(base64.b64decode(result['data']),b'<h1>Review</h1>')
  absolute=p.preview_native_file(dict(self.cfg,path=str(self.root/'docs/view.html')))
  self.assertEqual(absolute['sha256'],result['sha256'])
  cached=p.preview_native_file(dict(self.cfg,known={'version':result['version'],'bytes':result['bytes']}))
  self.assertTrue(cached['not_modified']);self.assertNotIn('data',cached)
 def test_assistant_local_file_uri_is_a_reference(self):
  self.page['messages'][0]['text']='[Review]('+ (self.root/'docs/view.html').as_uri()+')'
  self.assertEqual(base64.b64decode(p.preview_native_file(self.cfg)['data']),b'<h1>Review</h1>')
  (self.root/'docs/space name.txt').write_text('space',encoding='utf-8')
  self.page['messages'][0]['text']='[File]('+(self.root/'docs/space name.txt').as_uri()+')'
  self.assertEqual(base64.b64decode(p.preview_native_file(dict(self.cfg,path='docs/space name.txt'))['data']),b'space')
 def test_remote_or_ambiguous_file_uri_does_not_authorize(self):
  absolute=str(self.root/'docs/view.html')
  for ref in ['file://remote'+absolute,'file://user@host'+absolute,'file://'+absolute+'?token=x','file://'+absolute+'#fragment','file:///etc/passwd']:
   self.page['messages'][0]['text']='[Review]('+ref+')'
   with self.assertRaisesRegex(ValueError,'NOT_REFERENCED'):p.preview_native_file(self.cfg)
  self.page['messages'][0]={'role':'user','text':'[Review]('+(self.root/'docs/view.html').as_uri()+')'}
  with self.assertRaisesRegex(ValueError,'NOT_REFERENCED'):p.preview_native_file(self.cfg)
 def test_unreferenced_and_user_only_are_denied(self):
  self.page['messages'][0]['role']='user'
  with self.assertRaisesRegex(ValueError,'NOT_REFERENCED'):p.preview_native_file(self.cfg)
 def test_traversal_hidden_and_external_paths_denied(self):
  for path in ['../view.html','docs/../../view.html','.hidden.txt','/etc/passwd','docs/view.html?key=1']:
   with self.assertRaises(ValueError):p.preview_native_file(dict(self.cfg,path=path))
 def test_symlink_file_and_directory_denied(self):
  (self.root/'link.html').symlink_to(self.root/'docs/view.html')
  (self.root/'alias').symlink_to(self.root/'docs',target_is_directory=True)
  self.page['messages'][0]['text']='[a](link.html) [b](alias/view.html)'
  for path in ['link.html','alias/view.html']:
   with self.assertRaisesRegex(ValueError,'FILE_UNAVAILABLE'):p.preview_native_file(dict(self.cfg,path=path))
 def test_fifo_rejected_without_waiting(self):
  os.mkfifo(self.root/'docs/pipe.txt');self.page['messages'][0]['text']='[pipe](docs/pipe.txt)'
  with self.assertRaisesRegex(ValueError,'PATH_REJECTED'):p.preview_native_file(dict(self.cfg,path='docs/pipe.txt'))
 def test_size_budget(self):
  (self.root/'docs/view.html').write_bytes(b'x'*(2*1024*1024+1))
  with self.assertRaisesRegex(ValueError,'TOO_LARGE'):p.preview_native_file(self.cfg)
 def test_html_resources_relative_parent_and_content_version(self):
  (self.root/'shared').mkdir();(self.root/'shared/app.js').write_text('document.title="Review"',encoding='utf-8')
  (self.root/'docs/view.html').write_text('<script src="../shared/app.js"></script>',encoding='utf-8')
  first=p.preview_native_file(self.cfg)
  self.assertEqual(first['resources_unavailable'],[]);self.assertEqual(len(first['resources']),1)
  self.assertEqual(first['resources'][0]['mime'],'text/javascript')
  (self.root/'shared/app.js').write_text('document.title="Changed"',encoding='utf-8')
  second=p.preview_native_file(dict(self.cfg,known={'version':first['version'],'bytes':first['bytes']}))
  self.assertNotEqual(first['version'],second['version']);self.assertNotIn('not_modified',second)
 def test_html_resources_deny_external_hidden_escape_and_symlink(self):
  (self.root/'alias.css').symlink_to(self.root/'docs/view.html')
  urls=['https://example.invalid/a.css','../../escape.css','../.secret/a.css','../alias.css']
  body=''.join('<link rel="stylesheet" href="'+u+'">' for u in urls)
  resources,missing=p._html_resource_bundle(self.root,Path('docs/view.html'),body,2048)
  self.assertEqual(resources,[]);self.assertEqual(len(missing),4)
 def test_html_resource_budget_is_bounded(self):
  (self.root/'docs/style.css').write_text('x'*100,encoding='utf-8')
  resources,missing=p._html_resource_bundle(self.root,Path('docs/view.html'),'<link rel="stylesheet" href="style.css">',10)
  self.assertEqual(resources,[]);self.assertEqual(missing[0]['code'],'DSH_FILE_TOO_LARGE')
 def test_real_pdf_two_pages_and_original_bytes(self):
  import shutil
  if not shutil.which('pdftoppm') or not shutil.which('pdfinfo'):self.skipTest('local PDF renderer missing')
  objects=[b'<< /Type /Catalog /Pages 2 0 R >>',b'<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>',b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 5 0 R >>',b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 6 0 R >>']
  for command in [b'0 0 1 rg 10 10 100 100 re f',b'1 0 0 rg 20 20 80 80 re f']:
   objects.append(b'<< /Length '+str(len(command)).encode()+b' >>\nstream\n'+command+b'\nendstream')
  data=b'%PDF-1.4\n';offsets=[0]
  for i,obj in enumerate(objects,1):offsets.append(len(data));data+=str(i).encode()+b' 0 obj\n'+obj+b'\nendobj\n'
  xref=len(data);data+=b'xref\n0 7\n0000000000 65535 f \n'+b''.join(('%010d 00000 n \n'%n).encode() for n in offsets[1:])+b'trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n'+str(xref).encode()+b'\n%%EOF\n'
  (self.root/'docs/report.pdf').write_bytes(data);self.page['messages'][0]['text']='[Report](docs/report.pdf)'
  result=p.preview_native_file(dict(self.cfg,path='docs/report.pdf'))
  self.assertEqual(base64.b64decode(result['data']),data)
  self.assertEqual(result['document']['page_count'],2)
  with patch.object(p,'_document_preview',side_effect=AssertionError('unchanged document must not render twice')):
   cached=p.preview_native_file(dict(self.cfg,path='docs/report.pdf',known={'version':result['version'],'bytes':result['bytes']}))
   self.assertTrue(cached['not_modified']);self.assertNotIn('document',cached)
  self.assertEqual([x['number'] for x in result['document']['pages']],[1,2])
  for page in result['document']['pages']:self.assertTrue(base64.b64decode(page['data']).startswith(b'\xff\xd8'))
  with self.assertRaisesRegex(ValueError,'TOO_LARGE'):p._document_preview(data,'pdf',budget=1)
 def test_document_missing_renderer_and_malformed_pdf(self):
  import shutil
  with patch('shutil.which',return_value=None):
   with self.assertRaisesRegex(ValueError,'RENDERER_UNAVAILABLE'):p._document_preview(b'bad','pdf')
  if shutil.which('pdfinfo') and shutil.which('pdftoppm'):
   with self.assertRaisesRegex(ValueError,'RENDER_FAILED'):p._document_preview(b'not a pdf','pdf')
if __name__=='__main__':unittest.main()
