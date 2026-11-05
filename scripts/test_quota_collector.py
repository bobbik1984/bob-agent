import importlib.util, unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('collector',Path(__file__).with_name('bob-quota-collector.py'))
c=importlib.util.module_from_spec(spec);spec.loader.exec_module(c)
class QuotaTests(unittest.TestCase):
 def test_official_codex_window(self):
  q=c.normalize('codex',{'rateLimits':{'primary':{'usedPercent':3,'windowDurationMins':10080,'resetsAt':5000}},'token':'secret'},'fixture',1000)
  self.assertEqual(q['windows'][0]['remaining_percent'],97)
  self.assertEqual(q['windows'][0]['id'],'weekly');self.assertNotIn('token',q)
 def test_agy_disabled_and_fraction(self):
  q=c.normalize('antigravity',{'quota':{'gemini-weekly':{'remaining_fraction':.25},'off':{'disabled':True,'remaining_fraction':1}}},'fixture')
  self.assertEqual(len(q['windows']),1);self.assertEqual(q['windows'][0]['remaining_percent'],25)
 def test_claude_requires_official_fields(self):
  with self.assertRaises(ValueError):c.normalize('claude',{'usage':52},'fixture')
  q=c.normalize('claude',{'rate_limits':{'seven_day':{'used_percentage':52,'resets_at':5000}}},'fixture',1000)
  self.assertEqual(q['windows'][0]['remaining_percent'],48);self.assertEqual(q['observed_at'],1000)
 def test_invalid_numbers(self):
  for value in [float('nan'),float('inf'),-1,101,True]:self.assertIsNone(c.window('weekly',value,None))
if __name__=='__main__':unittest.main()
