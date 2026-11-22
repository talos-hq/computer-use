#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Disposable native Wayland window; JSON events go only to the supplied log.

Run inside the repo nix-shell with GDK_BACKEND=wayland. Start two instances
with different --title and --log values to exercise focus isolation. Ctrl+L
records a hotkey, and the button supplies an accessibility action/readback.
"""
import argparse
import json
import os
import time

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, Gtk  # noqa: E402


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--title", required=True)
    parser.add_argument("--log", required=True)
    args = parser.parse_args()

    def record(kind, **fields):
        with open(args.log, "a", encoding="utf-8") as log:
            log.write(json.dumps({"event": kind, "monotonic_ns": time.monotonic_ns(), **fields}) + "\n")

    window = Gtk.Window(title=args.title)
    window.set_default_size(520, 360)
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
    box.set_border_width(20)
    entry = Gtk.Entry()
    entry.get_accessible().set_name("Task3 text input")
    entry.connect("changed", lambda widget: record("text", text=widget.get_text()))
    for event_name in ("button-press-event", "button-release-event"):
        entry.connect(event_name, lambda widget, event, name=event_name:
                      record("entry-" + name, button=event.button, x=round(event.x), y=round(event.y)))
    button = Gtk.Button(label="Task3 counter: 0")
    count = 0

    def increment(widget):
        nonlocal count
        count += 1
        widget.set_label(f"Task3 counter: {count}")
        record("button", count=count)

    button.connect("clicked", increment)
    area = Gtk.EventBox()
    area.add(Gtk.Label(label="Task3 scroll and drag area"))
    area.set_size_request(440, 180)
    area.add_events(Gdk.EventMask.BUTTON_PRESS_MASK | Gdk.EventMask.BUTTON_RELEASE_MASK
                    | Gdk.EventMask.POINTER_MOTION_MASK | Gdk.EventMask.SCROLL_MASK
                    | Gdk.EventMask.SMOOTH_SCROLL_MASK)
    area.get_accessible().set_name("Task3 pointer area")

    def pointer(widget, event, name):
        fields = {"x": round(event.x), "y": round(event.y)}
        if name == "scroll-event":
            smooth, dx, dy = event.get_scroll_deltas()
            fields.update(smooth=smooth, dx=dx, dy=dy, direction=int(event.direction))
        elif name in ("button-press-event", "button-release-event"):
            fields["button"] = event.button
        record(name, **fields)

    for signal in ("button-press-event", "button-release-event", "motion-notify-event", "scroll-event"):
        area.connect(signal, pointer, signal)

    def key(widget, event):
        record("key", key=Gdk.keyval_name(event.keyval), code=event.hardware_keycode,
               modifiers=int(event.state), ctrl=bool(event.state & Gdk.ModifierType.CONTROL_MASK))
        if event.state & Gdk.ModifierType.CONTROL_MASK and Gdk.keyval_name(event.keyval) == "l":
            record("hotkey", keys="ctrl+l")
            return True
        return False

    window.connect("key-press-event", key)
    window.connect("key-release-event", lambda widget, event:
                   record("key-release", key=Gdk.keyval_name(event.keyval), code=event.hardware_keycode))
    window.connect("focus-in-event", lambda *unused: record("focus"))
    window.connect("focus-out-event", lambda *unused: record("blur"))
    window.connect("destroy", lambda *unused: Gtk.main_quit())
    for child in (entry, button, area):
        box.pack_start(child, False, False, 0)
    window.add(box)
    window.show_all()
    entry.grab_focus()
    record("ready", pid=os.getpid(), title=args.title, text=entry.get_text())
    Gtk.main()


if __name__ == "__main__":
    main()
