import importlib.util, tempfile, unittest, zipfile, os
from pathlib import Path
from packaging.tags import Tag
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('release_check',Path(__file__).with_name('check_release.py'))
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
class ReleaseChecks(unittest.TestCase):
 def test_tag_scope(self):
  m.validate_tags({Tag('cp313','cp313','macosx_26_0_arm64')})
  for t in [Tag('cp314','cp314','macosx_26_0_arm64'),Tag('cp313','abi3','macosx_26_0_arm64'),Tag('cp313','cp313','macosx_15_0_arm64'),Tag('cp313','cp313','macosx_26_1_arm64'),Tag('cp313','cp313','macosx_26_0_universal2'),Tag('cp313','cp313','macosx_26_0_x86_64')]:
   with self.subTest(tag=str(t)),self.assertRaises(AssertionError): m.validate_tags({t})
 def run_fixture(self,internal='cp313-cp313-macosx_26_0_arm64',distribution='sqlaero',version='0.1.0a1'):
  with tempfile.TemporaryDirectory() as d:
   p=Path(d); (p/'LICENSE').write_bytes(b'license')
   f=p/f'{distribution}-{version}-cp313-cp313-macosx_26_0_arm64.whl'
   with zipfile.ZipFile(f,'w') as z:
    pre=f'{distribution}-{version}.dist-info/'
    z.writestr(pre+'WHEEL',f'Wheel-Version: 1.0\nRoot-Is-Purelib: false\nTag: {internal}\n')
    z.writestr(pre+'METADATA','Name: SQLAero\nVersion: 0.1.0a1\nRequires-Python: >=3.13,<3.14\n')
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
 def test_internal_mismatch(self):
  with self.assertRaisesRegex(AssertionError,'tags differ'): self.run_fixture(internal='cp313-cp313-macosx_15_0_arm64')
 def test_wrong_distribution(self):
  with self.assertRaisesRegex(AssertionError,'distribution'): self.run_fixture(distribution='sqlcycli')
 def test_wrong_version(self):
  with self.assertRaisesRegex(AssertionError,'version'): self.run_fixture(version='0.1.0a2')
if __name__ == '__main__':
 unittest.main()
