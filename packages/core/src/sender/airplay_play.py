#!/usr/bin/env python3
"""Push a local or HTTP audio URL to an AirPlay receiver via pyatv (RAOP)."""
from __future__ import annotations

import argparse
import asyncio
import sys


async def play(host: str, url: str, volume: float | None) -> None:
    from pyatv import connect, scan

    loop = asyncio.get_running_loop()
    found = await scan(loop, hosts=[host], timeout=8)
    if not found:
        raise SystemExit(f"no AirPlay receiver at {host}")
    conf = found[0]
    atv = await connect(conf, loop)
    try:
        if volume is not None:
            try:
                await atv.audio.set_volume(volume)
            except Exception as exc:  # noqa: BLE001 — volume is best-effort
                print(f"volume skipped: {exc}", file=sys.stderr, flush=True)
        print(f"streaming {url} -> {conf.name} {host}", flush=True)
        await atv.stream.stream_file(url)
    finally:
        atv.close()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", required=True)
    parser.add_argument("--url", required=True)
    parser.add_argument("--volume", type=float, default=None)
    args = parser.parse_args()
    try:
        asyncio.run(play(args.host, args.url, args.volume))
    except KeyboardInterrupt:
        return 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
