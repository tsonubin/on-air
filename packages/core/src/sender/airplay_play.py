#!/usr/bin/env python3
"""Push a local or HTTP audio URL to an AirPlay receiver via pyatv (RAOP)."""
from __future__ import annotations

import argparse
import asyncio
import os
import sys
import threading


async def apply_volume(atv, volume: float) -> None:
    try:
        await atv.audio.set_volume(volume)
    except Exception as exc:  # noqa: BLE001 — keep the active stream alive
        print(f"volume change failed: {exc}", file=sys.stderr, flush=True)


async def apply_initial_volume(atv, volume: float) -> None:
    # RAOP receivers commonly reject volume until the stream session exists.
    await asyncio.sleep(0.5)
    await apply_volume(atv, volume)


def parse_volume(line: str) -> float | None:
    try:
        return min(100.0, max(0.0, float(line.strip())))
    except ValueError:
        print(f"invalid volume command: {line.strip()}", file=sys.stderr, flush=True)
        return None


def read_volume_from_pipe(
    loop: asyncio.AbstractEventLoop, atv, pending: bytearray
) -> None:
    chunk = os.read(sys.stdin.fileno(), 4096)
    if not chunk:
        loop.remove_reader(sys.stdin.fileno())
        return
    pending.extend(chunk)
    while (newline := pending.find(b"\n")) >= 0:
        line = bytes(pending[:newline]).decode("ascii", errors="replace")
        del pending[: newline + 1]
        if (volume := parse_volume(line)) is not None:
            loop.create_task(apply_volume(atv, volume))


def read_volume_commands(loop: asyncio.AbstractEventLoop, atv) -> None:
    for line in sys.stdin:
        if (volume := parse_volume(line)) is not None:
            asyncio.run_coroutine_threadsafe(apply_volume(atv, volume), loop)


def install_volume_control(loop: asyncio.AbstractEventLoop, atv) -> None:
    try:
        loop.add_reader(
            sys.stdin.fileno(), read_volume_from_pipe, loop, atv, bytearray()
        )
    except (AttributeError, NotImplementedError):
        threading.Thread(target=read_volume_commands, args=(loop, atv), daemon=True).start()


async def play(host: str, url: str, volume: float | None) -> None:
    from pyatv import connect, scan

    loop = asyncio.get_running_loop()
    found = await scan(loop, hosts=[host], timeout=8)
    if not found:
        raise SystemExit(f"no AirPlay receiver at {host}")
    conf = found[0]
    atv = await connect(conf, loop)
    install_volume_control(loop, atv)
    initial_volume_task = (
        asyncio.create_task(apply_initial_volume(atv, volume))
        if volume is not None
        else None
    )
    try:
        print(f"streaming {url} -> {conf.name} {host}", flush=True)
        await atv.stream.stream_file(url)
    finally:
        if initial_volume_task is not None:
            initial_volume_task.cancel()
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
