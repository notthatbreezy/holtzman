# Nano compatibility source

`tts_turbo.py` and `llama_configs.py` are unmodified upstream files from
[Resemble AI Chatterbox](https://github.com/resemble-ai/chatterbox), commit
`5de7a54aa4e5e2baadb0182dde554908b48b85c2`. The upstream MIT license is included.

The installed package remains `chatterbox-tts==0.1.6`. These two files add the
official Nano loader and GPT2-small configuration without replacing the working
AMD PyTorch packages or upgrading Transformers from 4.46.3. The runtime imports
them under a separate module name, adds the small configuration, and selects the
Nano class only for `model.repo_id: chatterbox-nano`. Turbo/original selectors
retain their existing implementations. This is a backport, not a full upstream
0.1.7 installation. Strict checkpoint loading and synthesis are tested locally.
