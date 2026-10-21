import asyncio
import time
from types import SimpleNamespace


class _Config:
    name = "Test AirPlay Receiver"


class _Audio:
    async def set_volume(self, volume):
        print(f"volume:{int(volume)}", flush=True)


class _Stream:
    async def stream_file(self, _url):
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
