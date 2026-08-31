import asyncio


class _Config:
    name = "Test AirPlay Receiver"


class _Audio:
    async def set_volume(self, volume):
        print(f"volume:{int(volume)}", flush=True)


class _Stream:
    async def stream_file(self, _url):
        await asyncio.sleep(60)


class _Device:
    audio = _Audio()
    stream = _Stream()

    def close(self):
        pass


async def scan(_loop, hosts, timeout):
    del hosts, timeout
    return [_Config()]


async def connect(_config, _loop):
    return _Device()
