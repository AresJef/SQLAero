import importlib.util, tempfile, unittest, zipfile, os
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from release_policy import read_package, release_allowed
from packaging.version import Version, InvalidVersion
from packaging.tags import Tag
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('release_check',Path(__file__).with_name('check_release.py'))
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
class ReleaseChecks(unittest.TestCase):
 def test_tag_scope(self):
  m.validate_tags({Tag('cp313','cp313','macosx_26_0_arm64')})
  for t in [Tag('cp314','cp314','macosx_26_0_arm64'),Tag('cp313','abi3','macosx_26_0_arm64'),Tag('cp313','cp313','macosx_15_0_arm64'),Tag('cp313','cp313','macosx_26_1_arm64'),Tag('cp313','cp313','macosx_26_0_universal2'),Tag('cp313','cp313','macosx_26_0_x86_64')]:
   with self.subTest(tag=str(t)),self.assertRaises(AssertionError): m.validate_tags({t})
 def run_fixture(self,internal='cp313-cp313-macosx_26_0_arm64',distribution='sqlaero',version='0.1.0a1',expected='0.1.0a1'):
  with tempfile.TemporaryDirectory() as d:
   p=Path(d); (p/'LICENSE').write_bytes(b'license')
   (p/'setup.cfg').write_text(f'[metadata]\nname = SQLAero\nversion = {expected}\n[options]\npython_requires = >=3.13,<3.14\n')
   f=p/f'{distribution}-{version}-cp313-cp313-macosx_26_0_arm64.whl'
   with zipfile.ZipFile(f,'w') as z:
    pre=f'{distribution}-{version}.dist-info/'
    z.writestr(pre+'WHEEL',f'Wheel-Version: 1.0\nRoot-Is-Purelib: false\nTag: {internal}\n')
    z.writestr(pre+'METADATA',f'Name: SQLAero\nVersion: {version}\nRequires-Python: >=3.13,<3.14\n')
    z.writestr(pre+'licenses/LICENSE',b'license')
    for i in range(19): z.writestr(f'sqlaero/mod{i}.so',b'fixture')
    for f in ['transcode.pxd','utils.pxd','connection.pyi']: z.writestr('sqlaero/'+f,b'fixture')
   old=os.getcwd()
   try:
    os.chdir(p)
    with patch.object(m,"validate_binary"): m.check_release(p)
   finally: os.chdir(old)
 def test_binary_checks(self):
  good='Load command 1\n cmd LC_BUILD_VERSION\n platform 1\n minos 26.0\n ntools 1\n tool 3\n version 27037.1\nLoad command 2\n cmd LC_SOURCE_VERSION\n version 0.0\n'
  with patch.object(m.subprocess,'check_output',side_effect=['arm64\n',good]): m.validate_binary(Path('fixture.so'))
  for outputs in [['arm64 x86_64\n'],['arm64\n',good.replace('minos 26.0','minos 15.0')],['arm64\n',good.replace('platform 1','platform 2')]]:
   with patch.object(m.subprocess,'check_output',side_effect=outputs),self.assertRaises(AssertionError): m.validate_binary(Path('fixture.so'))
 def test_valid(self): self.run_fixture()
 def test_future_wheels(self):
  for v in ['0.1.0a2','0.1.0rc1','0.1.0']:
   with self.subTest(version=v): self.run_fixture(version=v,expected=v)
 def test_internal_mismatch(self):
  with self.assertRaisesRegex(AssertionError,'tags differ'): self.run_fixture(internal='cp313-cp313-macosx_15_0_arm64')
 def test_wrong_distribution(self):
  with self.assertRaisesRegex(AssertionError,'distribution'): self.run_fixture(distribution='sqlcycli')
 def test_wrong_version(self):
  with self.assertRaisesRegex(AssertionError,'version'): self.run_fixture(version='0.1.0a2')
class ReleasePolicyChecks(unittest.TestCase):
 def allowed(self,v='0.1.0a2',tag=None,pre=None,event_name='release',actor='AresJef',trigger='AresJef',repo='AresJef/SQLAero',action='published',draft=False):
  version=Version(v)
  event={'action':action,'release':{'tag_name':tag if tag is not None else 'v'+v,'prerelease':version.is_prerelease if pre is None else pre,'draft':draft}}
  return release_allowed(version,event_name,actor,trigger,repo,event)
 def test_alpha_rc_stable(self):
  for v in ['0.1.0a2','0.1.0rc1','0.1.0','0.2.0.dev1','0.1.0.post1']:
   with self.subTest(version=v): self.assertTrue(self.allowed(v))
 def test_bad_tags(self):
  for tag in ['v0.2.0','0.1.0a2','vnot-a-version','v0.1.0-alpha.2']:
   with self.subTest(tag=tag),self.assertRaises((AssertionError,InvalidVersion)):self.allowed(tag=tag)
 def test_label_and_draft(self):
  for kwargs in [dict(pre=False),dict(v='0.1.0',pre=True),dict(draft=True)]:
   with self.subTest(kwargs=kwargs),self.assertRaises(AssertionError):self.allowed(**kwargs)
 def test_nonpublish_events_and_identity(self):
  for kwargs in [dict(event_name='workflow_dispatch'),dict(event_name='push'),dict(event_name='pull_request'),dict(event_name=''),dict(actor='other'),dict(trigger='other'),dict(repo='other/SQLAero'),dict(action='created'),dict(action='edited')]:
   with self.subTest(kwargs=kwargs):self.assertFalse(self.allowed(**kwargs))
  self.assertFalse(release_allowed(Version('0.1.0'),'release','AresJef','AresJef','AresJef/SQLAero',{}))
 def test_package_metadata(self):
  with tempfile.TemporaryDirectory() as d:
   p=Path(d)/'setup.cfg'
   for name,version in [('SQLAero','0.1.0a2'),('SQLCyCli','0.1.0a2'),('SQLAero','invalid'),('SQLAero','v0.1.0'),('SQLAero','0.1.0+local')]:
    p.write_text(f'[metadata]\nname = {name}\nversion = {version}\n[options]\npython_requires = >=3.13,<3.14\n')
    if name=='SQLAero' and version=='0.1.0a2':self.assertEqual(read_package(p),Version(version))
    else:
     with self.subTest(name=name,version=version),self.assertRaises((AssertionError,InvalidVersion)):read_package(p)
if __name__ == '__main__':
 unittest.main()
