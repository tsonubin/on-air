#!/usr/bin/env python3
"""Exercise the actual QML panel against the actual mock core, offscreen.

Requires PySide6. Pass --core target/debug/on-air-core. An optional --font
is a local Nerd Font for screenshot fidelity; font assets are not bundled.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
import urllib.request

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("QT_QUICK_BACKEND", "software")
from PySide6.QtCore import QPoint, QPointF, QUrl, qInstallMessageHandler
from PySide6.QtGui import QFontDatabase, QGuiApplication
from PySide6.QtQml import QQmlApplicationEngine, QQmlComponent, QQmlEngine, QQmlExpression
from PySide6.QtTest import QTest
from PySide6.QtQuick import QQuickItem, QQuickWindow
from PySide6.QtCore import Qt


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--core", required=True)
    parser.add_argument("--output", default="/tmp/on-air-panel.png")
    parser.add_argument("--font")
    args = parser.parse_args()
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    messages = []
    qInstallMessageHandler(lambda kind, context, message: messages.append(message))
    app = QGuiApplication([])
    font_family = "monospace"
    if args.font:
        font_id = QFontDatabase.addApplicationFont(args.font)
        assert font_id >= 0
        font_family = QFontDatabase.applicationFontFamilies(font_id)[0]
    core = subprocess.Popen([args.core], env={**os.environ, "ON_AIR_MOCK": "1", "BIND": "127.0.0.1", "PORT": str(port)},
                            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)

    def api():
        with urllib.request.urlopen(f"http://127.0.0.1:{port}/api/patchbay", timeout=5) as response:
            return json.load(response)

    def wait(predicate, description, timeout=15):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            app.processEvents()
            if predicate():
                return
            QTest.qWait(30)
        raise AssertionError(description + "\n" + "\n".join(messages))

    engine = None
    try:
        end = time.monotonic() + 10
        while True:
            try:
                api()
                break
            except OSError:
                if time.monotonic() >= end:
                    raise
                time.sleep(0.05)
        engine = QQmlApplicationEngine()
        cached_component = QQmlComponent(engine, QUrl.fromLocalFile(str(Path(__file__).resolve().parent.parent / "PatchbayView.qml")))
        cached_board = cached_component.createWithInitialProperties({"snapshot": api(), "width": 380, "height": 225})
        assert cached_board is not None, [error.toString() for error in cached_component.errors()]
        assert cached_board.property("ready"), "a lazy popup must accept an already loaded snapshot"
        cached_board.deleteLater()
        engine.setInitialProperties({"apiPort": port, "previewFontFamily": font_family})
        engine.load(QUrl.fromLocalFile(str(Path(__file__).resolve().parent.parent / "Preview.qml")))
        assert engine.rootObjects(), "\n".join(messages)
        window = engine.rootObjects()[0]
        # QML's root board is QQuickItem, not QQuickWindow.
        board = window.findChild(QQuickItem, "patchbay")
        assert board is not None

        def expression(text):
            expr = QQmlExpression(QQmlEngine.contextForObject(board), board, text)
            value, undefined = expr.evaluate()
            assert not expr.hasError(), expr.error().toString()
            return value

        def point(node):
            value = expression("portFor(" + json.dumps(node) + ")")
            assert value is not None, node
            return QPoint(round(value.x()), round(value.y()))

        def drag(start, finish, hold_midway=False):
            QTest.mousePress(window, Qt.LeftButton, Qt.NoModifier, start)
            for n in range(1, 11):
                QTest.mouseMove(window, start + (finish-start) * (n / 10), 18)
                if n == 5 and hold_midway:
                    # An external client changes a node during the gesture.
                    request = urllib.request.Request(f"http://127.0.0.1:{port}/api/patchbay",
                        data=json.dumps({"op":"set_volume","id":"source:mic","volume":79}).encode(),
                        headers={"Content-Type":"application/json"}, method="POST")
                    with urllib.request.urlopen(request, timeout=5):
                        pass
                    QTest.qWait(2300)  # a background poll must not destroy the grabbed port
            QTest.mouseRelease(window, Qt.LeftButton, Qt.NoModifier, finish)

        wait(lambda: expression("(snapshot.nodes || []).length") >= 5, "QML did not load the core's devices")
        assert window.width() == 380
        assert window.height() <= 260, window.height()
        # Drag row bodies on the input side: this must create a real mix.
        a, b = point("source:mic"), point("desktop")
        drag(QPoint(65, a.y()), QPoint(65, b.y()))
        wait(lambda: any(n["kind"] == "group" for n in api()["nodes"]), "body drag did not create a mix")
        group = next(n["id"] for n in api()["nodes"] if n["kind"] == "group")
        wait(lambda: expression("node(" + json.dumps(group) + ") !== null"), "mix was not shown")
        QTest.qWait(100)
        # Drag connectors across the gap: this must create a live route.
        drag(point(group), point("sink:headphones"), hold_midway=True)
        wait(lambda: len(api()["links"]) == 1, "port drag did not connect")
        assert api()["links"][0]["state"] == "connected"
        # Releasing a cable should select its destination for volume control.
        assert expression("selectedId") == "sink:headphones"
        slider = window.findChild(QQuickItem, "volumeSlider")
        center = slider.mapToScene(QPointF(slider.width() * 0.37, slider.height()/2))
        QTest.mouseClick(window, Qt.LeftButton, Qt.NoModifier, QPoint(round(center.x()), round(center.y())))
        wait(lambda: next(n for n in api()["nodes"] if n["id"] == "sink:headphones")["volume"] < 50,
             "volume did not reach the core")
        board.forceActiveFocus()
        QTest.keyClick(window, Qt.Key_Space)
        wait(lambda: next(n for n in api()["nodes"] if n["id"] == "sink:headphones")["muted"], "keyboard mute failed")
        QTest.keyClick(window, Qt.Key_Space)
        wait(lambda: not next(n for n in api()["nodes"] if n["id"] == "sink:headphones")["muted"], "keyboard unmute failed")
        QTest.qWait(3000)
        Path(args.output).parent.mkdir(parents=True, exist_ok=True)
        assert window.grabWindow().save(args.output)
        errors = [m for m in messages if any(s in m for s in ("ReferenceError", "TypeError", "Cannot assign", "Binding loop", "is not defined"))]
        assert not errors, "\n".join(errors)
        window.close()
        app.processEvents()
        assert not window.findChild(QQuickItem, "cableLayer").property("animatingAllowed")
        assert len(api()["links"]) == 1, "closing the UI must leave routing alive"
        print(f"passed: Qt body drag, port drag, volume, keyboard mute, UI close; screenshot {args.output}")
    finally:
        if engine is not None:
            engine.deleteLater()
            app.processEvents()
        core.terminate()
        try:
            core.wait(timeout=10)
        except subprocess.TimeoutExpired:
            core.kill()
            core.wait()


if __name__ == "__main__":
    main()
