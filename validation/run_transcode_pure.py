"""Run only pure transcode cases against the installed wheel, blocking networking."""
from pathlib import Path
import runpy
import sys

def block_network(event: str, args: tuple[object, ...]) -> None:
    """Reject socket connections if a selected test unexpectedly attempts networking."""
    if event in {'socket.connect', 'socket.getaddrinfo', 'socket.bind'}:
        raise RuntimeError('Database/network access is forbidden during this check')

sys.addaudithook(block_network)
namespace = runpy.run_path(str(Path(__file__).resolve().parents[1] / 'src' / 'test_transcode.py'))
namespace['TestEscape']().test_all()
namespace['TestDecode']().test_all()
print('Explicit pure TestEscape/TestDecode cases passed; networking blocked')
