# KDE Plasma 6 Wayland input

`computer-use` discovers windows through KWin's D-Bus scripting interface.
No effect plugin, root access, or additional permission dialog is required.
The existing optional read-only KWin helper and native capture adapters remain
available as discovery fallbacks.

Script load/run/unload is serialized within the daemon. KWin allocates script
IDs from the current script count, so removing scripts out of order can leave
an ID pointing at an existing script. The adapter checks the scripting object
paths and reserves inert slots until it gets an unoccupied ID; it runs only
that request script and unloads only its own unique names. Reservation and
cleanup have deadlines. Concurrent third-party script lifecycle changes can
still race KWin's API; enumeration or activation failures must be inspected.

Window IDs belong to the daemon that enumerated them. They identify KWin's
internal UUID and unique D-Bus owner, together with the window's PID. They
survive title changes, but a compositor or daemon restart requires enumeration
again. Native Wayland and XWayland windows share this route.

Use `delivery_mode: "foreground"` for pointer events and keyboard events:

```json
{"pid":1234,"window_id":3489660928,"x":120,"y":80,"delivery_mode":"foreground","restore_focus":false}
```

This is an example `click` argument object; use IDs returned by `list_windows`.
The same delivery mode works with `type_text`, `press_key`, `hotkey`,
`double_click`, `right_click`, `scroll`, and `drag`. Window coordinates refer
to the frame shown in the window screenshot, including decorations. AT-SPI
element indices/tokens are resolved within the selected window.
Coordinate tools retain the existing screenshot-grounding contract: obtain a
`get_window_state` screenshot for the current snapshot first. A new tree-only
snapshot needs a fresh screenshot before subsequent coordinate input.

Background mode retains the shared tool contract: accessible actions and
editable-text operations can run without activating a window, but raw
focus-bound events require foreground delivery. A background refusal includes
the existing foreground escalation advice. An omitted mode does not silently
authorize focus changes.

Foreground delivery activates the exact UUID/PID, reads the active window
back, and uses the daemon's existing shared RemoteDesktop/libei session. It
checks focus and geometry immediately before and after dispatch. Text is
checked in bounded chunks; scroll ticks are checked individually. Pointer
coordinates must remain inside the target, and KWin's `workspace.windowAt`
must confirm that the target is the topmost window at the point. Both drag
endpoints are checked before the gesture.

`restore_focus` overrides the tool's normal focus policy on KDE. By default,
typing, keys and single clicks restore the previous window; menu clicks and
gestures leave the target focused. Set `true` to request restoration or
`false` to leave it active. Restoration is verified and never steals focus
back after an observed external focus change.

This route is **not atomic target-bound input**: libei delivers to whichever
surface the compositor considers focused when it processes an event. A brief
focus change entirely between checks may be missed. A failed post-check
returns an error with an unverifiable effect; some input may already have
landed. Inspect the target before retrying. Successful focus checks also
report `verified: false` for the application effect; use accessible readback
or a window screenshot to confirm the result.

`kwin_delivery_unverified` preserves the worker or focus-check error. Delivery
can be partial even when the final focus check passes. `focus_after` describes
cleanup separately: `restored`, `left_focused`, `no_previous_window`, or
`restore_failed`. A restoration failure does not erase delivery evidence or
claim that input was never sent. A worker's failed target check remains latched
even if the target regains focus before the final check; restoration is skipped.

KWin frame and client geometry are global logical pixels. Window screenshots
are normalized to that logical frame before cropping; output scale must not
be applied a second time. Negative output origins are translated through the
logical desktop layout for capture. libei receives global logical coordinates
and its existing region router selects the device. An unmappable portal
stream or a window partly outside the capture fails explicitly.

Window screenshots use the existing shared monitor stream. The target must
be visible and unoccluded, and its geometry and occlusion state are checked
before and after capture. Another window may hold keyboard focus.
This is a desktop crop: always-on-top surfaces or compositor decorations can
appear over the target. No separate ScreenCast or Screenshot consent session
is created.

`doctor` reports the scripting enumeration route, foreground delivery route,
and actual AT-SPI bus/registry reachability. It uses native D-Bus calls and
does not need `gdbus` on PATH. If the existing accessibility launcher reports
`IsEnabled=false`, the driver enables that property without a UI prompt.
It does not change `ScreenReaderEnabled` or the system configuration.

If AT-SPI is missing on NixOS, add this to the system configuration yourself
and log in again after applying it:

```nix
services.gnome.at-spi2-core.enable = true;
```

That service is also used with Plasma. Qt applications that expose no tree
may need `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1` in their launch environment;
the driver cannot change the environment of an already running application.
Coordinate input and screenshots remain independent of AT-SPI.

The disposable validation fixture is
`tests/fixtures/apps/linux/kde-input.py`. Run it inside the repository
`nix-shell` with `GDK_BACKEND=wayland`, a unique `--title`, and a private
`--log` path. Its JSON event log contains only its own input and state.
Events include monotonic timestamps, key codes/modifiers, key releases, and
focus changes. For exact typing validation, assert that the initial entry is
empty and compare the entire final value with a unique string, including
punctuation. Replacing a selection can leave existing, unselected
punctuation in the entry; checking only whether the value contains the new
string would miss this contamination. Fixture events alone cannot identify
which desktop input client sent unexpected keys.
