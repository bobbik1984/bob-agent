"""Lightweight provisioning checks. Does not compile or exercise Rust HTTP routes."""
import hashlib, importlib.util, json, tempfile, unittest
from pathlib import Path

spec=importlib.util.spec_from_file_location('provision_bot',Path(__file__).with_name('provision-bot-account.py'))
mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)

class ProvisionTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name)
        self.scope={'projects':['test-project'],'sessions':[],'organize':False}
    def tearDown(self): self.tmp.cleanup()
    def create(self): return mod.provision(self.root/'data',self.scope,self.root/'private-token')
    def test_default_readonly_secret_is_not_in_grants(self):
        result=self.create();token=(self.root/'private-token').read_text(encoding='utf-8').strip()
        raw=(self.root/'data/bot-grants.json').read_text(encoding='utf-8');grant=json.loads(raw)[0]
        self.assertEqual(result['username'],'grok_bot');self.assertFalse(result['organize']);self.assertFalse(result['execution'])
        self.assertTrue(token.startswith('bob_bot_'));self.assertNotIn(token,raw)
        self.assertEqual(grant['token_hash'],hashlib.sha256(token.encode()).hexdigest())
    def test_todo_permissions_default_denied(self):
        self.create();grant=json.loads((self.root/'data/bot-grants.json').read_text(encoding='utf-8'))[0]
        self.assertFalse(grant['todos_read']);self.assertFalse(grant['todos_create'])
    def test_todo_permissions_explicit(self):
        self.scope.update(todos_read=True,todos_create=True);self.create()
        grant=json.loads((self.root/'data/bot-grants.json').read_text(encoding='utf-8'))[0]
        self.assertTrue(grant['todos_read']);self.assertTrue(grant['todos_create'])
    def test_todo_permission_string_rejected(self):
        self.scope['todos_create']='true'
        with self.assertRaises(ValueError): self.create()
    def test_existing_account_not_overwritten(self):
        self.create();old=(self.root/'data/bot-grants.json').read_bytes()
        with self.assertRaises(FileExistsError): self.create()
        self.assertEqual(old,(self.root/'data/bot-grants.json').read_bytes())
    def test_no_wildcard_projects(self):
        self.scope['projects']=['*']
        with self.assertRaises(ValueError): self.create()
    def test_no_execution_scope(self):
        self.scope['execution']=True
        with self.assertRaises(ValueError): self.create()
    def test_no_implicit_organize(self):
        self.scope['organize']='true'
        with self.assertRaises(ValueError): self.create()
    def test_no_wildcard_sessions(self):
        self.scope['sessions']=[{'connection_id':'*','harness':'dsh','session_id':'a'}]
        with self.assertRaises(ValueError): self.create()
    def test_bounded_lifetime(self):
        with self.assertRaises(ValueError): mod.provision(self.root/'data',self.scope,self.root/'token',hours=99999)
    def test_explicit_no_expiry(self):
        mod.provision(self.root/'data',self.scope,self.root/'private-token',hours=None)
        grant=json.loads((self.root/'data/bot-grants.json').read_text(encoding='utf-8'))[0]
        self.assertTrue(grant['no_expiry']);self.assertEqual(grant['expires_at'],0)
    def test_explicit_organize(self):
        self.scope['organize']=True
        self.assertTrue(self.create()['organize'])

if __name__=='__main__': unittest.main()
