from pathlib import Path
import unittest,json,os
r=Path(__file__).resolve().parent
os.chdir(r/"repo")
def cases(suite):
 for x in suite:
  if isinstance(x,unittest.TestSuite):yield from cases(x)
  else:yield x
ids=[c.id() for c in cases(unittest.defaultTestLoader.discover(str(r/"repo/tests"),pattern="test_*.py"))]
(r/"discovered-test-ids.json").write_text(json.dumps(ids,indent=2)+"\n")
print(len(ids))
