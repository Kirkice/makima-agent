"""Thin re-export of shared Fish Audio helpers from makima_common.

The actual implementation lives in packages/common/src/makima_common/fish_audio.py.
This file exists so that local imports (e.g. `from fish_audio import transcribe`)
continue to work without path changes.
"""

from makima_common.fish_audio import (  # noqa: F401
    pcm_to_wav,
    wav_to_pcm,
    transcribe,
    synthesize,
    synthesize_sync,
)