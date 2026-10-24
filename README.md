# universal-tiling-window-manager

Rectangle Pro's keyboard shortcuts on Windows, configured by the export Rectangle Pro writes on the Mac.

Rectangle Pro stays the place shortcuts are edited. Export its settings on the Mac, put the file on the Windows machine (a dotfiles repo is the easy way), and the same keys throw windows to the same places: ⌃⌥← is Ctrl+Alt+Left, a second press grows the half to two thirds, a third shrinks it to one third, exactly as on the Mac. Window positions are computed by a port of Rectangle's own calculations, so a left third is the same third, down to the pixel rounding.

It runs in the tray. It has no settings window, because Rectangle Pro on the Mac is the settings window.

## Install

```powershell
cargo install --locked --git https://github.com/johncarmack1984/universal-tiling-window-manager universal-tiling-window-manager
universal-tiling-window-manager
```

The second line starts it in the tray. To have it start with Windows, tick **Start at login** in its tray menu, which adds it to your user's startup apps (Task Manager lists it there too). Building needs Rust 1.95 or newer and the MSVC build tools. It is developed and tested on Windows 11 x64.

To update, stop the running copy first, since Windows won't replace a running executable: `universal-tiling-window-manager quit`, then the `cargo install` line again with `--force`.

## Configure it from the Mac

1. On the Mac: Rectangle Pro → Settings → General → Export. Free Rectangle's export works too.
2. On Windows, put the file at `~/.config/rectangle-pro/RectangleProConfig.json` (free Rectangle's goes in `~/.config/rectangle/RectangleConfig.json`).
3. Check what it binds: `universal-tiling-window-manager check`.

The running app notices a new export within a couple of seconds; **Reload configuration** in the tray menu does it immediately. With no export at all it uses Rectangle's recommended shortcuts, so it works out of the box.

`check` prints every shortcut with its Windows chord, everything it could not bind and why, and the behaviour settings it read:

```
Bindings (41 chords):
  ⌃⌥←          Ctrl+Alt+Left       leftHalf
  ⌃⌥E          Ctrl+Alt+E          firstTwoThirds → fillLeft (cycles on repeat)
  ⌃⌥ Keypad 7  Ctrl+Alt+Num7       bottomLeftEighth
  ⌃⌥X          Ctrl+Alt+X          custom position "Left 1/3 Below Movie"

Not bound on Windows (1):
  topLeftEighth  ⌃⌥ Keypad Clear  lands on NumLock, a lock key; lock keys are never bound (RustDesk taps NumLock to sync lock state)
```

The export is looked for in this order: `--config` or `UTWM_CONFIG`; `rectangle_config` in the overrides file below; `~/.config/rectangle-pro/RectangleProConfig.json`; `~/.config/rectangle/RectangleConfig.json`; Rectangle's built-in defaults.

## How keys translate

Each Mac shortcut binds to the Windows key in the same physical position, not the key with the same label, which is also how macOS key codes work. Control becomes Ctrl, Option becomes Alt, Command becomes Win, and Shift stays Shift: what a Mac keyboard sends to Windows, and what RustDesk delivers in its default Map mode when a Mac drives the Windows machine. Keypad keys stay keypad keys whatever the NumLock state, and Home stays distinct from keypad 7; matching is by scan code, which virtual-key codes can't do once NumLock is off.

Keys in the NumLock, CapsLock and ScrollLock positions are never bound: a binding would toggle the lock, and RustDesk taps NumLock on its own to keep lock state in sync. On a Mac keyboard that is the keypad's Clear key.

`universal-tiling-window-manager probe` logs every key event for a minute without swallowing anything, which shows what a keyboard or a remote-control app actually delivers.

## Windows-only overrides

Anything that only makes sense on Windows goes in `~/.config/universal-tiling-window-manager/config.toml`. The file is optional; every key in it is.

```toml
# Read the export from somewhere else.
rectangle_config = "~/Dropbox/RectangleProConfig.json"

# Windows apps to leave alone, by executable name (Rectangle's "Ignore app").
ignored_apps = ["mstsc.exe"]

# What each Mac modifier becomes: ctrl, alt, shift, win or none.
[modifiers]
command = "win"

# Rebind an action on Windows only, or unbind it with "".
[shortcuts]
topLeftEighth = "ctrl+alt+num0"
```

## What it does

All of Rectangle's single-window actions up to eighths, with Rectangle's repeat behaviour for each:
- halves (½ → ⅔ → ⅓ with the default cycle sizes) and center half;
- corners;
- thirds and two thirds;
- fourths and three fourths;
- sixths and eighths;
- maximize, almost maximize, maximize height, center;
- make larger and smaller, restore;
- next and previous display, keeping the edges a window was against.

From Rectangle Pro:
- **Fill Left and Fill Right**: the largest empty area against that side of the screen, or the half when there is none.
- **Tile N×M** (`tile2x3` and friends): every window on the display in a grid, the focused one first.
- **Custom Size & Position**: including its repetitions.

Several actions on one shortcut cycle on repeated presses, as in Rectangle 2.0 and Rectangle Pro. The settings that shape all this are read from the export: repeat mode and cycle sizes, gaps and screen-edge gaps, almost-maximize size, step size, minimum window size, display order, and what happens to windows that refuse a size. `check` lists any it reads but does not apply.

Not supported yet; `check` names them when an export binds them:
- the remaining grid actions (ninths, twelfths, sixteenths, vertical thirds and sixths), the move actions, Specified, and move-to-display-N;
- multi-window actions other than tiling (tile all, cascade, reverse);
- Todo mode, Stash and Layouts;
- drag-to-snap areas (Windows Snap covers dragging).

Limits worth knowing:
- **Elevated windows.** Windows only lets an app move an administrator window if the app runs elevated too, so those windows don't move.
- **App shortcuts.** Ctrl+Alt shortcuts are global, so they take precedence over apps that use the same keys, such as VS Code's Ctrl+Alt+Up and Down.

## Command line

```
universal-tiling-window-manager [--config <export.json>] [command]

  (none)                run in the tray
  check [export.json]   show what each Mac shortcut becomes on Windows
  probe [seconds]       log every key the keyboard delivers (default 60s)
  reload                make the running instance re-read its configuration
  pause                 pause or resume the running instance's shortcuts
  quit                  stop the running instance
  version               print the version
```

The log is at `%LOCALAPPDATA%\universal-tiling-window-manager\log.txt`; set `UTWM_LOG=debug` to see every shortcut and where each window landed.

## Development

| Crate | What it is |
|---|---|
| `crates/utwm-config` | The export format, macOS key code to Windows scan code translation, bindings, and the key matcher the hook runs |
| `crates/utwm-layout` | Rectangle's window calculations, ported; pure, so its tests run anywhere |
| `crates/utwm-windows` | Win32: the low-level keyboard hook, finding and placing windows, displays, start at login |
| `apps/desktop` | The Tauri 3 tray app and the command line |

`just check` runs formatting, clippy and the tests. `just run` starts the app from source with debug logging, and `just e2e` then runs the end-to-end check: it opens a window of its own, presses shortcuts the way RustDesk delivers them, and checks where the window lands to the pixel. It expects a 1920×1080 display with a bottom taskbar and the export from the author's Mac, so treat it as a template for other setups.

## License

MIT. The window calculations are ported from [Rectangle](https://github.com/rxhanson/Rectangle) (MIT); see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Not affiliated with Rectangle, Rectangle Pro or Knollsoft.
