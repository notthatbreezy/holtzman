"""Start the one managed Chatterbox server with a pinned public Turbo checkpoint.

The published 0.1.6 loader unnecessarily requires a Hugging Face token and asks
for an unused second decoder. Keep that compatibility fix here rather than
editing the installed package. Other model selectors retain their normal loader.
"""
import argparse
import os
from pathlib import Path
import runpy
import sys

TURBO_REPO = 'ResembleAI/chatterbox-turbo'
TURBO_REVISION = '749d1c1a46eb10492095d68fbcf55691ccf137cd'
TURBO_FILES = ['ve.safetensors', 's3gen_meanflow.safetensors', 't3_turbo_v1.safetensors',
               'conds.pt', 'added_tokens.json', 'special_tokens_map.json',
               'tokenizer_config.json', 'vocab.json', 'merges.txt']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server-script', type=Path, required=True)
    args = parser.parse_args()
    script = args.server_script.resolve(strict=True)
    os.chdir(script.parent)
    sys.path.insert(0, str(script.parent))
    import yaml
    config = yaml.safe_load((script.parent / 'config.yaml').read_text(encoding='utf-8'))
    selector = str(config.get('model', {}).get('repo_id', '')).lower()
    if selector in ('turbo', 'chatterbox-turbo', 'resembleai/chatterbox-turbo'):
        os.environ.setdefault('HF_HUB_DISABLE_XET', '1')
        from chatterbox.tts_turbo import ChatterboxTurboTTS
        from huggingface_hub import snapshot_download
        from huggingface_hub.errors import LocalEntryNotFoundError

        def load_turbo(cls, device):
            options = dict(repo_id=TURBO_REPO, revision=TURBO_REVISION,
                           allow_patterns=TURBO_FILES, token=os.getenv('HF_TOKEN') or None)
            try:
                checkpoint = Path(snapshot_download(**options, local_files_only=True))
                if not all((checkpoint / name).is_file() for name in TURBO_FILES):
                    raise FileNotFoundError('Incomplete cached Turbo checkpoint')
            except (LocalEntryNotFoundError, FileNotFoundError):
                checkpoint = Path(snapshot_download(**options))
            return cls.from_local(checkpoint, device)

        ChatterboxTurboTTS.from_pretrained = classmethod(load_turbo)
    sys.argv = [str(script)]
    runpy.run_path(str(script), run_name='__main__')


if __name__ == '__main__':
    main()
