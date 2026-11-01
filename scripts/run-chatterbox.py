"""Start the one managed Chatterbox server with pinned public checkpoints.

The published 0.1.6 loader unnecessarily requires a Hugging Face token and asks
for an unused second decoder. Keep that compatibility fix here rather than
editing the installed package. Other model selectors retain their normal loader.
"""
import argparse
import os
from pathlib import Path
import runpy
import sys

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server-script', type=Path, required=True)
    args = parser.parse_args()
    script = args.server_script.resolve(strict=True)
    os.chdir(script.parent)
    sys.path.insert(0, str(script.parent))
    os.environ.setdefault('HF_HUB_DISABLE_XET', '1')
    import engine
    from chatterbox_runtime import configure_engine
    configure_engine(engine)
    sys.argv = [str(script)]
    runpy.run_path(str(script), run_name='__main__')


if __name__ == '__main__':
    main()
