# universal-tiling-window-manager

A tiling window manager you drive from the keyboard, on Windows and the Mac. A shortcut puts the window in a half, a third, a corner, or on another display.

One configuration serves both machines, with the same keys in the same places: ⌃⌥← on the Mac is Ctrl+Alt+Left on Windows. It lives in the tray on Windows and in the menu bar on the Mac, updates itself from signed releases, and imports Rectangle Pro configs.

## Install

On Windows, download `Window-Manager_<version>_x64-setup.exe` from the [latest release](https://github.com/johncarmack1984/universal-tiling-window-manager/releases/latest) and run it. It installs for your user only, with no administrator prompt, into `%LOCALAPPDATA%\Window Manager`, and offers to start the app in the tray when it finishes. The installer isn't code-signed, so SmartScreen questions it the first time: **More info**, then **Run anyway**. To have it start with Windows, tick **Start at login** in its tray menu, which adds it to your user's startup apps (Task Manager lists it there too). It is developed and tested on Windows 11 x64.

It keeps itself up to date as the Mac app does (below), except that a newer release's installer does the replacing: it runs on its own, showing only a progress bar, and starts the app again. The command line is the installed executable, `& "$env:LOCALAPPDATA\Window Manager\universal-tiling-window-manager.exe" check`; putting that folder on your PATH shortens it to `universal-tiling-window-manager check`.

To build it from source instead:

```powershell
cargo install --locked --git https://github.com/johncarmack1984/universal-tiling-window-manager universal-tiling-window-manager
universal-tiling-window-manager
```

The second line starts it in the tray. Building needs Rust 1.95 or newer and the MSVC build tools. A copy built this way doesn't update itself, since it isn't where the installer would put one. To update it, stop the running copy first, since Windows won't replace a running executable: `universal-tiling-window-manager quit`, then the `cargo install` line again with `--force`. To move from it to the installer, untick **Start at login** in its menu, quit it, and `cargo uninstall universal-tiling-window-manager` first.

On the Mac it is an app, Window Manager. Download `Window-Manager_<version>_universal.zip` from the [latest release](https://github.com/johncarmack1984/universal-tiling-window-manager/releases/latest), open it, drag Window Manager into /Applications, and open it from there; from then on it opens from Finder or Spotlight like any other app. It is signed with a Developer ID and notarized, and runs on Apple silicon and Intel Macs with macOS 13 or newer.

Moving other apps' windows takes Accessibility access. Until the app has it, the first line of its menu says so; clicking that line, or pressing any shortcut, brings up macOS's prompt and opens System Settings → Privacy & Security → Accessibility with the app in the list, ready to switch on. Tick **Start at login** in its menu to have it start with the Mac; System Settings → General → Login Items lists it under Open at Login.

It keeps itself up to date. A minute after it starts, and every six hours after that, it looks for a newer release; when there is one it downloads it, checks its signature against the key built into the app, puts it in place of the copy in /Applications and restarts, keeping its Accessibility access, since every release is signed with the same Developer ID. **Check for updates** in its menu, or `universal-tiling-window-manager update`, looks right away. While shortcuts are paused it waits to be asked, since the restart would resume them.

To build it from a clone instead, with [just](https://just.systems):

```sh
git clone https://github.com/johncarmack1984/universal-tiling-window-manager
cd universal-tiling-window-manager
just install
```

`just install` builds the app, puts it in /Applications and starts it in the menu bar. Building needs Rust 1.95 or newer and the Xcode command line tools. To update a build from source, `git pull` and `just install` again; it quits the running copy itself. macOS ties Accessibility access to the app's signature, and the build is signed ad hoc unless you name a certificate, so after each update the app asks for access again (the installer clears the stale grant first, so the switch in System Settings never shows on while doing nothing). Signing with a code-signing certificate from your keychain keeps the access across updates; a self-signed one made in Keychain Access → Certificate Assistant → Create a Certificate, with Code Signing as its type, will do: `UTWM_SIGN_IDENTITY="<certificate name>" just install`. A build from source also updates itself once a release newer than its version comes out, and asks for Accessibility access once more then, since the release's signature is a different one.

The command line is inside the app: `"/Applications/Window Manager.app/Contents/MacOS/universal-tiling-window-manager" check`. A symlink on your PATH shortens that to `universal-tiling-window-manager check`, as on Windows. Start the app itself from Finder, Spotlight or `open -a "Window Manager"` rather than from a terminal, or macOS counts it as part of the terminal and asks for the terminal's Accessibility access instead of its own.

## Shortcuts

| Action | Mac | Windows | Name in config.toml |
|---|---|---|---|
| Left, right, top, bottom half | ⌃⌥← ⌃⌥→ ⌃⌥↑ ⌃⌥↓ | Ctrl+Alt+Left, Right, Up, Down | `leftHalf` `rightHalf` `topHalf` `bottomHalf` |
| Top left, top right, bottom left, bottom right | ⌃⌥U ⌃⌥I ⌃⌥J ⌃⌥K | Ctrl+Alt+U, I, J, K | `topLeft` `topRight` `bottomLeft` `bottomRight` |
| First, center, last third | ⌃⌥D ⌃⌥F ⌃⌥G | Ctrl+Alt+D, F, G | `firstThird` `centerThird` `lastThird` |
| First, center, last two thirds | ⌃⌥E ⌃⌥R ⌃⌥T | Ctrl+Alt+E, R, T | `firstTwoThirds` `centerTwoThirds` `lastTwoThirds` |
| Maximize | ⌃⌥↩ | Ctrl+Alt+Enter | `maximize` |
| Maximize height | ⌃⌥⇧↑ | Ctrl+Alt+Shift+Up | `maximizeHeight` |
| Larger, smaller | ⌃⌥= ⌃⌥- | Ctrl+Alt+=, - | `larger` `smaller` |
| Center | ⌃⌥C | Ctrl+Alt+C | `center` |
| Restore | ⌃⌥⌫ | Ctrl+Alt+Backspace | `restore` |
| Next, previous display | ⌃⌥⌘→ ⌃⌥⌘← | Ctrl+Alt+Win+Right, Left | `nextDisplay` `previousDisplay` |

Press a half again and it cycles through two thirds and one third. Every action is in the menu too, with its shortcut if it has one.

These have no shortcut until you give them one:
- center half: `centerHalf`;
- almost maximize, and growing or shrinking one way: `almostMaximize`, `largerWidth`, `smallerWidth`, `largerHeight`, `smallerHeight`;
- fourths and three fourths: `firstFourth`, `secondFourth`, `thirdFourth`, `lastFourth`, `firstThreeFourths`, `centerThreeFourths`, `lastThreeFourths`;
- sixths: `topLeftSixth`, `topCenterSixth`, `topRightSixth`, `bottomLeftSixth`, `bottomCenterSixth`, `bottomRightSixth`;
- eighths: `topLeftEighth`, `topCenterLeftEighth`, `topCenterRightEighth`, `topRightEighth`, `bottomLeftEighth`, `bottomCenterLeftEighth`, `bottomCenterRightEighth`, `bottomRightEighth`;
- fill: `fillLeft` and `fillRight` take the largest empty area against that side of the screen, or the half when there is none;
- tile: `tile1x2`, `tile1x3`, `tile1x4`, `tile2x1`, `tile2x2` and `tile2x3` lay every window on the display out in a grid, the focused one first.

## Configuration

Everything lives in `~/.config/universal-tiling-window-manager/config.toml`. The file is optional, and so is every key in it. Top-level keys are for Windows and the `[mac]` table is for the Mac, so one file in your dotfiles serves both machines. The running app notices a change within a couple of seconds; **Reload configuration** in its menu does it right away.

```toml
# A Rectangle Pro config to import, if it isn't at the usual path (below).
import = "~/Dropbox/RectangleProConfig.json"

# Windows apps to leave alone, by executable name.
ignored_apps = ["mstsc.exe"]

# What each Mac modifier becomes on Windows: ctrl, alt, shift, win or none.
[modifiers]
command = "win"

# Rebind an action on Windows only, or unbind it with "".
[shortcuts]
topLeftEighth = "ctrl+alt+num0"

# Mac apps to leave alone, by bundle id.
[mac]
ignored_apps = ["com.apple.Terminal"]

# Rebind an action on the Mac only, or unbind it with "". Keys are named by
# position as on a PC keyboard (Backspace is ⌫, NumLock or clear is keypad
# Clear), and ctrl, alt or option, shift, and cmd are ⌃, ⌥, ⇧ and ⌘.
[mac.shortcuts]
leftHalf = "ctrl+option+h"
```

`universal-tiling-window-manager check` prints every shortcut as it binds on this machine, anything it couldn't bind and why, and the settings in effect.

### Importing a Rectangle Pro config

Export it from Rectangle Pro (Settings → General → Export) and put the file at `~/.config/rectangle-pro/RectangleProConfig.json`, or name it with `import` in config.toml. Its shortcuts, gaps, custom positions and ignored apps replace the defaults, and config.toml still applies on top. A shortcut it gives several actions cycles through them on repeated presses.

With an imported config, a repeated half can move on to the next display instead of cycling sizes: a second ⌃⌥← puts the window in the right half of the display to the left, a third press in that display's left half, and at the outermost display it stays put; →, ↑ and ↓ work the same way.

On the Mac, quit Rectangle Pro and turn off its launch at login, since both answer the same shortcuts; the menu says so while it runs.

## How keys translate

On the Mac nothing translates: a shortcut is exactly the key and modifiers configured, registered as a Carbon hot key. Registering them takes no permission, and the app never sees any other keystroke.

On Windows each Mac shortcut binds to the Windows key in the same physical position, not the key with the same label, which is also how macOS key codes work. Control becomes Ctrl, Option becomes Alt, Command becomes Win, and Shift stays Shift: what a Mac keyboard sends to Windows, and what RustDesk delivers in its default Map mode when a Mac drives the Windows machine. Keypad keys stay keypad keys whatever the NumLock state, and Home stays distinct from keypad 7; matching is by scan code, which virtual-key codes can't do once NumLock is off.

Keys in the NumLock, CapsLock and ScrollLock positions are never bound on Windows: a binding would toggle the lock, and RustDesk taps NumLock on its own to keep lock state in sync. On a Mac keyboard that is the keypad's Clear key, which the Mac binds as usual.

`universal-tiling-window-manager probe` logs every key event for a minute without swallowing anything, which shows what a keyboard or a remote-control app actually delivers to Windows.

## Not supported yet

`check` names these when an imported config binds them:
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
  update                make the running instance install the newest release
  version               print the version
```

The log is at `%LOCALAPPDATA%\universal-tiling-window-manager\log.txt` on Windows and `~/Library/Logs/universal-tiling-window-manager/log.txt` on the Mac; set `UTWM_LOG=debug` to see every shortcut and where each window landed.

## Development

| Crate | What it is |
|---|---|
| `crates/utwm-config` | config.toml and the import format, bindings for either platform, the macOS key code to Windows scan code translation, and the key matcher the Windows hook runs |
| `crates/utwm-layout` | Where each action puts a window; pure, so its tests run anywhere |
| `crates/utwm-windows` | Win32: the low-level keyboard hook, finding and placing windows, displays, start at login |
| `crates/utwm-macos` | macOS: Carbon hot keys, finding and placing windows through the Accessibility API, displays, start at login (a LaunchAgent) |
| `apps/desktop` | The Tauri 3 tray app and the command line, one codebase for both platforms |
| `tools/bundle` | `utwm-bundle`, which builds the Mac app bundle (`just bundle`), for both architectures with `--universal`, and installs it into /Applications (`just install`) |
| `tools/e2e` | The end-to-end checks (`utwm-e2e`, one per platform), and `utwm-inject`, which presses keys by scan code on Windows the way RustDesk does |

`just check` runs formatting, clippy and the tests. `just run` starts the app from source with debug logging, and `just e2e` then runs the end-to-end check for that platform, which opens a window of its own and moves only that.

On Windows it presses shortcuts the way RustDesk delivers them and checks where the window lands to the pixel. It expects a 1920×1080 display with a bottom taskbar and the author's imported config, so treat it as a template for other setups.

On the Mac it posts the configured shortcuts as key events and checks that each lands exactly where the layout engine says, on any display arrangement and with any export. It needs the app running with Accessibility access, no other window manager answering the same shortcuts, and Accessibility access for the terminal running the check, which posting key events requires.

### Releases

Commits follow [Conventional Commits](https://www.conventionalcommits.org): `feat:` and `fix:` are what a release is made of, and pull requests are squash-merged, so a pull request's title is its commit. [release-please](https://github.com/googleapis/release-please) keeps a release pull request open with the next version and its CHANGELOG. Merging it runs `.github/workflows/release.yml`, which tags the release, runs CI at the tag, builds the Mac app for both architectures, signs it with the Developer ID and notarizes it, builds the Windows installer, and attaches both to the GitHub release. The updater's `latest.json`, which installed copies read, goes up last, so a release that failed on the way never reaches them. The workflow reads the Developer ID and the updater's signing key from AWS Secrets Manager through GitHub's OIDC, as a role only main can assume (`infra/`, applied by hand).

## License

MIT. Some of the window calculations are ported from MIT-licensed work credited in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
