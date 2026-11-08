import asyncio
import time
import sys
from types import SimpleNamespace


async def _read_tags(_buffer):
    raise AssertionError("live WAV must not be scanned for file tags")


_metadata = SimpleNamespace(EMPTY_METADATA=object())
_audio_source = SimpleNamespace(get_buffered_io_metadata=_read_tags)
sys.modules["pyatv.protocols"] = SimpleNamespace()
sys.modules["pyatv.protocols.raop"] = SimpleNamespace(audio_source=_audio_source)
sys.modules["pyatv.support"] = SimpleNamespace()
sys.modules["pyatv.support.metadata"] = _metadata


class _Config:
    name = "Test AirPlay Receiver"


class _Audio:
    async def set_volume(self, volume):
        print(f"volume:{int(volume)}", flush=True)


class _Stream:
    async def stream_file(self, _url):
        assert await _audio_source.get_buffered_io_metadata(object()) is _metadata.EMPTY_METADATA
        raop = _Device.settings.protocols.raop
        assert 48000 <= raop.control_port < raop.timing_port <= 48127
        _Device.metadata.started = time.monotonic()
        await asyncio.sleep(60)


class _Metadata:
    started = None

    async def playing(self):
        elapsed = int(time.monotonic() - self.started) if self.started else 0
        return SimpleNamespace(
            device_state=SimpleNamespace(name="Playing" if self.started else "Idle"),
            position=elapsed,
        )


class _Device:
    audio = _Audio()
    stream = _Stream()
    settings = SimpleNamespace(protocols=SimpleNamespace(raop=SimpleNamespace()))
    metadata = _Metadata()

    def close(self):
        pass


async def scan(_loop, hosts, timeout):
    del hosts, timeout
    return [_Config()]


const = SimpleNamespace(Protocol=SimpleNamespace(RAOP="raop"))


async def connect(_config, _loop, protocol):
    assert protocol == "raop"
    return _Device()
