"""Pinned Chatterbox loaders; keep the working Windows ROCm dependencies intact."""
import importlib.util
import os
from pathlib import Path
import sys

TURBO_REPO = 'ResembleAI/chatterbox-turbo'
TURBO_REVISION = '749d1c1a46eb10492095d68fbcf55691ccf137cd'
NANO_REPO = 'ResembleAI/chatterbox-nano'
NANO_REVISION = '71ccd1d0081b430592cea481f4307e764e07bc64'
SHARED_FILES = ['ve.safetensors', 's3gen_meanflow.safetensors', 'conds.pt',
                'added_tokens.json', 'special_tokens_map.json',
                'tokenizer_config.json', 'vocab.json', 'merges.txt']
NANO_SELECTORS = ('nano', 'chatterbox-nano', 'resembleai/chatterbox-nano')


def checkpoint(variant):
    os.environ.setdefault('HF_HUB_DISABLE_XET', '1')
    from huggingface_hub import snapshot_download
    from huggingface_hub.errors import LocalEntryNotFoundError
    nano = variant == 'nano'
    files = SHARED_FILES + ['t3_nano_v1.safetensors' if nano else 't3_turbo_v1.safetensors']
    options = dict(repo_id=NANO_REPO if nano else TURBO_REPO,
                   revision=NANO_REVISION if nano else TURBO_REVISION,
                   allow_patterns=files, token=os.getenv('HF_TOKEN') or None)
    try:
        folder = Path(snapshot_download(**options, local_files_only=True))
        if not all((folder / name).is_file() for name in files):
            raise FileNotFoundError('Incomplete cached Chatterbox checkpoint')
    except (LocalEntryNotFoundError, FileNotFoundError):
        folder = Path(snapshot_download(**options))
    return folder


def _import_file(name, path):
    if name in sys.modules:
        return sys.modules[name]
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def nano_class():
    """Load the upstream Nano addition without upgrading unrelated dependencies."""
    folder = Path(__file__).resolve().parents[1] / 'integrations/sillytavern/chatterbox-nano-compat'
    configs = _import_file('holt_nano_configs', folder / 'llama_configs.py')
    from chatterbox.models.t3.llama_configs import LLAMA_CONFIGS
    LLAMA_CONFIGS['GPT2_small'] = configs.GPT2_SMALL_CONFIG
    upstream = _import_file('chatterbox.holt_nano', folder / 'tts_turbo.py')

    class ChatterboxNanoTTS(upstream.ChatterboxTurboTTS):
        @classmethod
        def from_pretrained(cls, device):
            return cls.from_local(checkpoint('nano'), device, nano=True)

    return ChatterboxNanoTTS


def configure_engine(engine):
    # Always retain the normal Turbo selector, including UI reloads after Nano.
    engine.ChatterboxTurboTTS.from_pretrained = classmethod(
        lambda cls, device: cls.from_local(checkpoint('turbo'), device))
    original_selector = engine._get_model_class

    def select(selector):
        if selector.lower().strip() in NANO_SELECTORS:
            return nano_class(), 'turbo'  # Reuse the server's Turbo cache/tag path.
        return original_selector(selector)

    engine._get_model_class = select
    original_info = engine.get_model_info

    def info():
        result = original_info()
        if engine.chatterbox_model is not None:
            result['variant'] = getattr(engine.chatterbox_model, 'model_label', result['type'])
        return result

    engine.get_model_info = info
