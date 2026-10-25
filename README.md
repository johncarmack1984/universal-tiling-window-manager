# universal-tiling-window-manager

Rectangle Pro's keyboard shortcuts on Windows and the Mac, configured by the export Rectangle Pro writes.

The export is the configuration on both machines. Put it in your dotfiles and the same keys throw windows to the same places everywhere: on the Mac, exactly the shortcuts Rectangle Pro bound; on Windows, the keys in the same positions, so ⌃⌥← is Ctrl+Alt+Left. A second press grows the half to two thirds and a third shrinks it to one third, as Rectangle does. Window positions are computed by a port of Rectangle's own calculations, so a left third is the same third, down to the pixel rounding.

On the Mac it runs in Rectangle Pro's place. It lives in the tray on Windows and in the menu bar on the Mac, and it has no settings window: the export is the settings, `config.toml` holds whatever differs between the machines, and Rectangle Pro, if you keep it installed, can still edit the shortcuts and export them again.

## Install

On Windows:

```powershell
cargo install --locked --git https://github.com/johncarmack1984/universal-tiling-window-manager universal-tiling-window-manager
universal-tiling-window-manager
```

The second line starts it in the tray. To have it start with Windows, tick **Start at login** in its tray menu, which adds it to your user's startup apps (Task Manager lists it there too). Building needs Rust 1.95 or newer and the MSVC build tools. It is developed and tested on Windows 11 x64.

On the Mac:

```sh
cargo install --locked --git https://github.com/johncarmack1984/universal-tiling-window-manager universal-tiling-window-manager
universal-tiling-window-manager
```

The second line starts it in the menu bar. Building needs Rust 1.95 or newer and the Xcode command line tools.

Moving other apps' windows takes Accessibility access. Until the app has it, the first line of its menu says so; clicking that line, or pressing any shortcut, brings up macOS's prompt and opens System Settings → Privacy & Security → Accessibility with the app in the list, ready to switch on. macOS grants the access to whatever started the app, which is your terminal if you ran it from one. To grant it to the app alone, tick **Start at login** in its menu, quit it, and start it the way login will: log out and back in, or run `launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.johncarmack.universal-tiling-window-manager.plist`. System Settings → General → Login Items lists it under Allow in the Background.

To update, stop the running copy first, since Windows won't replace a running executable: `universal-tiling-window-manager quit`, then the `cargo install` line again with `--force`. macOS takes the rebuilt executable for a new app, so its Accessibility switch stops counting: remove it from the list and let the app ask again. Signing each build with a code-signing certificate from your keychain (a self-signed one made in Keychain Access will do) keeps the access across updates: `codesign -fs "<certificate name>" "$(command -v universal-tiling-window-manager)"`.

## Switching from Rectangle Pro on the Mac

1. Rectangle Pro → Settings → General → Export, and put the file at `~/.config/rectangle-pro/RectangleProConfig.json`.
2. Check what it binds: `universal-tiling-window-manager check`. On the Mac every shortcut binds as it is, keypad Clear included.
3. Quit Rectangle Pro and turn off its launch at login. While both run they answer the same shortcuts, and this app's menu says so until Rectangle Pro quits.
4. Start this app, give it Accessibility access, and tick **Start at login**.

Everything the export binds and this app supports carries over: the shortcuts and their cycles, the repeat behaviour, gaps, custom positions, and the apps Rectangle ignores (`disabledApps`, by bundle id). Drag-to-snap doesn't; macOS's own window tiling covers dragging (System Settings → Desktop & Dock). To change a shortcut later, export again from Rectangle Pro, which can stay installed as the editor without launching at login, or set it under `[mac.shortcuts]` in `config.toml`.

## The export

1. On the Mac: Rectangle Pro → Settings → General → Export. Free Rectangle's export works too.
2. On each machine, put the file at `~/.config/rectangle-pro/RectangleProConfig.json` (free Rectangle's goes in `~/.config/rectangle/RectangleConfig.json`). A dotfiles repo is the easy way.
3. Check what it binds: `universal-tiling-window-manager check`.

The running app notices a new export within a couple of seconds; **Reload configuration** in its menu does it immediately. With no export at all it uses Rectangle's recommended shortcuts, so it works out of the box.

`check` prints every shortcut with the chord it becomes on this machine, everything it could not bind and why, and the behaviour settings it read. On Windows:

```
Bindings (41 chords):
  ⌃⌥←          Ctrl+Alt+Left       leftHalf
  ⌃⌥E          Ctrl+Alt+E          firstTwoThirds → fillLeft (cycles on repeat)
  ⌃⌥ Keypad 7  Ctrl+Alt+Num7       bottomLeftEighth
  ⌃⌥X          Ctrl+Alt+X          custom position "Left 1/3 Below Movie"

Not bound on Windows (1):
  topLeftEighth  ⌃⌥ Keypad Clear  lands on NumLock, a lock key; lock keys are never bound (RustDesk taps NumLock to sync lock state)
```

On the Mac the same export binds all 42, each as Rectangle Pro shows it:

```
Bindings (42 chords):
  ⌃⌥←              leftHalf
  ⌃⌥E              firstTwoThirds → fillLeft (cycles on repeat)
  ⌃⌥ Keypad Clear  topLeftEighth
  ⌃⌥X              custom position "Left 1/3 Below Movie"
```

The export is looked for in this order: `--config` or `UTWM_CONFIG`; `rectangle_config` in the overrides file below; `~/.config/rectangle-pro/RectangleProConfig.json`; `~/.config/rectangle/RectangleConfig.json`; Rectangle's built-in defaults.

## How keys translate

On the Mac nothing translates: a shortcut is the key code and modifiers the export stores, registered the way Rectangle registers its own (Carbon hot keys). Registering them takes no permission, and the app never sees any other keystroke.

On Windows each Mac shortcut binds to the Windows key in the same physical position, not the key with the same label, which is also how macOS key codes work. Control becomes Ctrl, Option becomes Alt, Command becomes Win, and Shift stays Shift: what a Mac keyboard sends to Windows, and what RustDesk delivers in its default Map mode when a Mac drives the Windows machine. Keypad keys stay keypad keys whatever the NumLock state, and Home stays distinct from keypad 7; matching is by scan code, which virtual-key codes can't do once NumLock is off.

Keys in the NumLock, CapsLock and ScrollLock positions are never bound on Windows: a binding would toggle the lock, and RustDesk taps NumLock on its own to keep lock state in sync. On a Mac keyboard that is the keypad's Clear key, which the Mac binds as usual.

`universal-tiling-window-manager probe` logs every key event for a minute without swallowing anything, which shows what a keyboard or a remote-control app actually delivers to Windows.

## Per-machine overrides

Whatever differs between the machines goes in `~/.config/universal-tiling-window-manager/config.toml`. The file is optional; every key in it is. The top-level keys are Windows', and the `[mac]` table is the Mac's, so one file in the dotfiles serves both.

```toml
# Read the export from somewhere else (both machines).
rectangle_config = "~/Dropbox/RectangleProConfig.json"

# Windows apps to leave alone, by executable name (Rectangle's "Ignore app").
ignored_apps = ["mstsc.exe"]

# What each Mac modifier becomes on Windows: ctrl, alt, shift, win or none.
[modifiers]
command = "win"

# Rebind an action on Windows only, or unbind it with "".
[shortcuts]
topLeftEighth = "ctrl+alt+num0"

# Mac apps to leave alone, by bundle id, on top of the export's own list.
[mac]
ignored_apps = ["com.apple.Terminal"]

# Rebind an action on the Mac only, or unbind it with "". Keys are named by
# position as on a PC keyboard (Backspace is ⌫, NumLock or clear is keypad
# Clear), and ctrl, alt or option, shift, and cmd are ⌃, ⌥, ⇧ and ⌘.
[mac.shortcuts]
leftHalf = "ctrl+option+h"
```

## What it does

On both machines, all of Rectangle's single-window actions up to eighths, with Rectangle's repeat behaviour for each:
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
- drag-to-snap areas (Windows Snap and macOS's own window tiling cover dragging).

Limits worth knowing:
- **Elevated windows.** Windows only lets an app move an administrator window if the app runs elevated too, so those windows don't move.
- **App shortcuts.** The shortcuts are global, so they take precedence over apps that use the same keys, such as VS Code's Ctrl+Alt+Up and Down on Windows.
- **Full screen.** On the Mac, windows in full screen (in a Space of their own), minimized windows and sheets are left alone.

## Command line

```
universal-tiling-window-manager [--config <export.json>] [command]

  (none)                run in the tray (the menu bar on the Mac)
  check [export.json]   show what each shortcut becomes on this machine
  probe [seconds]       log every key the keyboard delivers (default 60s; Windows)
  reload                make the running instance re-read its configuration
  pause                 pause or resume the running instance's shortcuts
  quit                  stop the running instance
  version               print the version
```

The log is at `%LOCALAPPDATA%\universal-tiling-window-manager\log.txt` on Windows and `~/Library/Logs/universal-tiling-window-manager/log.txt` on the Mac; set `UTWM_LOG=debug` to see every shortcut and where each window landed.

## Development

| Crate | What it is |
|---|---|
| `crates/utwm-config` | The export format, bindings for either platform, the macOS key code to Windows scan code translation, and the key matcher the Windows hook runs |
| `crates/utwm-layout` | Rectangle's window calculations, ported; pure, so its tests run anywhere |
| `crates/utwm-windows` | Win32: the low-level keyboard hook, finding and placing windows, displays, start at login |
| `crates/utwm-macos` | macOS: Carbon hot keys, finding and placing windows through the Accessibility API, displays, start at login (a LaunchAgent) |
| `apps/desktop` | The Tauri 3 tray app and the command line, one codebase for both platforms |
| `tools/e2e` | The end-to-end checks (`utwm-e2e`, one per platform), and `utwm-inject`, which presses keys by scan code on Windows the way RustDesk does |

`just check` runs formatting, clippy and the tests. `just run` starts the app from source with debug logging, and `just e2e` then runs the end-to-end check for that platform, which opens a window of its own and moves only that.

On Windows it presses shortcuts the way RustDesk delivers them and checks where the window lands to the pixel. It expects a 1920×1080 display with a bottom taskbar and the export from the author's Mac, so treat it as a template for other setups.

On the Mac it posts the configured shortcuts as key events and checks that each lands exactly where the layout engine says, on any display arrangement and with any export. It needs the app running with Accessibility access, Rectangle Pro quit, and Accessibility access for the terminal running the check, which posting key events requires.

## License

MIT. The window calculations are ported from [Rectangle](https://github.com/rxhanson/Rectangle) (MIT); see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Not affiliated with Rectangle, Rectangle Pro or Knollsoft.
