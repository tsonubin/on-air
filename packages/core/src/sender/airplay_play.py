#!/usr/bin/env python3
"""Push a local or HTTP audio URL to an AirPlay receiver via pyatv (RAOP)."""
from __future__ import annotations

import argparse
import asyncio
import os
import socket
import sys
import threading

AIRPLAY_UDP_PORT_MIN = 48000
AIRPLAY_UDP_PORT_MAX = 48127


def reserve_airplay_ports():
    """Reserve a distinct timing/control slot for each concurrent sender.

    A TCP bind holds the slot across helpers without occupying the UDP
    sockets pyatv opens on the same port numbers.
    """
    for control in range(AIRPLAY_UDP_PORT_MIN, AIRPLAY_UDP_PORT_MAX, 2):
        guard = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            guard.bind(("127.0.0.1", control))
            for port in (control, control + 1):
                with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as probe:
                    probe.bind(("", port))
            return guard, control, control + 1
        except OSError:
            guard.close()
    raise RuntimeError("No free on-air AirPlay timing/control ports (48000–48127)")


async def report_readiness(atv):
    # RAOP metadata comes from its own packet sender. Wait for its playhead
    # to advance, rather than treating connect/SETUP or an old receiver song
    # as proof that this stream started.
    while True:
        playing = await atv.metadata.playing()
        if playing.device_state.name == "Playing" and (playing.position or 0) >= 1:
            print("ON_AIR_READY", flush=True)
            return
        await asyncio.sleep(0.1)


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
    loop: asyncio.AbstractEventLoop, atv, pending: bytearray, stopped: asyncio.Event
) -> None:
    chunk = os.read(sys.stdin.fileno(), 4096)
    if not chunk:
        loop.remove_reader(sys.stdin.fileno())
        stopped.set()
        return
    pending.extend(chunk)
    while (newline := pending.find(b"\n")) >= 0:
        line = bytes(pending[:newline]).decode("ascii", errors="replace")
        del pending[: newline + 1]
        if line.strip() == "STOP":
            stopped.set()
        elif (volume := parse_volume(line)) is not None:
            loop.create_task(apply_volume(atv, volume))


def read_volume_commands(loop: asyncio.AbstractEventLoop, atv, stopped: asyncio.Event) -> None:
    for line in sys.stdin:
        if line.strip() == "STOP":
            loop.call_soon_threadsafe(stopped.set)
        elif (volume := parse_volume(line)) is not None:
            asyncio.run_coroutine_threadsafe(apply_volume(atv, volume), loop)
    loop.call_soon_threadsafe(stopped.set)


def install_volume_control(loop: asyncio.AbstractEventLoop, atv, stopped: asyncio.Event) -> None:
    try:
        loop.add_reader(
            sys.stdin.fileno(), read_volume_from_pipe, loop, atv, bytearray(), stopped
        )
    except (AttributeError, NotImplementedError):
        threading.Thread(target=read_volume_commands, args=(loop, atv, stopped), daemon=True).start()


async def play(host: str, url: str, volume: float | None) -> None:
    from pyatv import connect, scan, const

    loop = asyncio.get_running_loop()
    found = await scan(loop, hosts=[host], timeout=8)
    if not found:
        raise SystemExit(f"no AirPlay receiver at {host}")
    conf = found[0]
    atv = await connect(conf, loop, protocol=const.Protocol.RAOP)
    guard, control_port, timing_port = reserve_airplay_ports()
    atv.settings.protocols.raop.control_port = control_port
    atv.settings.protocols.raop.timing_port = timing_port
    stopped = asyncio.Event()
    install_volume_control(loop, atv, stopped)
    readiness_task = asyncio.create_task(report_readiness(atv))
    initial_volume_task = (
        asyncio.create_task(apply_initial_volume(atv, volume))
        if volume is not None
        else None
    )
    stream = asyncio.create_task(atv.stream.stream_file(url))
    stop_requested = asyncio.create_task(stopped.wait())
    try:
        print(f"connecting to {conf.name} {host}", flush=True)
        done, _pending = await asyncio.wait(
            [stream, stop_requested], return_when=asyncio.FIRST_COMPLETED
        )
        if stream in done:
            await stream
    finally:
        stream.cancel()
        await asyncio.gather(stream, return_exceptions=True)
        stop_requested.cancel()
        guard.close()
        readiness_task.cancel()
        if initial_volume_task is not None:
            initial_volume_task.cancel()
        pending = atv.close()
        if pending:
            await asyncio.gather(*pending, return_exceptions=True)


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
