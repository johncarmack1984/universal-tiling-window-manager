# universal-tiling-window-manager

A tiling window manager you drive from the keyboard, on Windows, the Mac and Linux (GNOME). A shortcut puts the window in a half, a third, a corner, or on another display.

One configuration serves every machine, with the same keys in the same places: ⌃⌥← on the Mac is Ctrl+Alt+Left on Windows and Linux. It lives in the tray on Windows and Linux and in the menu bar on the Mac, updates itself from signed releases, and imports Rectangle Pro configs.

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

On Linux it runs under GNOME on Wayland, the desktop Ubuntu and Fedora start in. Download `Window-Manager_<version>_x86_64` from the [latest release](https://github.com/johncarmack1984/universal-tiling-window-manager/releases/latest) and install it from a terminal:

```sh
sudo apt install libwebkit2gtk-4.1-0    # on Fedora: sudo dnf install webkit2gtk4.1
chmod +x ~/Downloads/Window-Manager_*_x86_64
~/Downloads/Window-Manager_*_x86_64 install
```

The first line installs WebKitGTK, the one library the app needs that a desktop may not have already. `install` puts the app in `~/.local/bin`, for your user only, with a desktop entry, so it opens from the app grid like any other app, and starts it in the tray; the download can go once it has. It is developed and tested on Ubuntu 26.04 with GNOME 50, on x86-64.

GNOME lets no app move other apps' windows or grab shortcuts of its own, so the app works through a small GNOME Shell extension that it carries and installs, `universal-tiling-window-manager@johncarmack.com`. GNOME Shell only starts a new extension when you log in, so log out and back in once after the first install. Until the extension runs, the first line of the app's menu says what's left, and clicking it takes the next step: installing the extension, switching it on, or logging out. The menu shows in the top bar through Ubuntu's AppIndicator support; on other GNOMEs, install the AppIndicator and KStatusNotifierItem Support extension first. Tick **Start at login** in the menu to have it start with GNOME.

GNOME keeps some of the same keys for its own shortcuts: Ctrl+Alt with an arrow switches workspaces, Ctrl+Alt+Shift+Up moves a window to the workspace above, Ctrl+Alt+D shows the desktop and Ctrl+Alt+T opens a terminal. While GNOME holds them, the menu's first line says how many of the app's shortcuts are GNOME's; clicking it takes just those keys out of GNOME's shortcuts and gives them to the app, leaving GNOME its other keys for the same things (Super+Page Up still switches workspaces). Settings → Keyboard → View and Customize Shortcuts puts any of them back, or `[linux.shortcuts]` in config.toml moves the app's action to another key instead.

It keeps itself up to date as the Mac app does (above): a newer release's executable, its signature checked, takes the place of the one in `~/.local/bin`, and the app starts again. When the update changes the extension, the new copy loads at the next log-in, and the menu says so. The command line is `~/.local/bin/universal-tiling-window-manager check`, or `universal-tiling-window-manager check` with `~/.local/bin` on your PATH.

To build it from a clone instead, with [just](https://just.systems):

```sh
sudo apt install libwebkit2gtk-4.1-dev libdbus-1-dev pkg-config
git clone https://github.com/johncarmack1984/universal-tiling-window-manager
cd universal-tiling-window-manager
just install
```

`just install` builds the app and installs it as `install` does a download. Building needs Rust 1.95 or newer and the system's GTK and WebKitGTK headers (on Fedora, `webkit2gtk4.1-devel` and `dbus-devel`); when Homebrew's pkg-config comes first on your PATH, the justfile uses the system's. To update a build from source, `git pull` and `just install` again; it quits the running copy itself. Like a download, it also updates itself once a release newer than its version comes out. `cargo install --locked --git https://github.com/johncarmack1984/universal-tiling-window-manager universal-tiling-window-manager` works too, with the same headers, but a copy built that way doesn't update itself, since it isn't in `~/.local/bin`.

## Shortcuts

| Action | Mac | Windows and Linux | Name in config.toml |
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
| Next, previous display | ⌃⌥⌘→ ⌃⌥⌘← | Ctrl+Alt+Win+Right, Left (Super on Linux) | `nextDisplay` `previousDisplay` |

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

Everything lives in `~/.config/universal-tiling-window-manager/config.toml`. The file is optional, and so is every key in it. Top-level keys are for Windows, the `[mac]` table is for the Mac and the `[linux]` table is for Linux, so one file in your dotfiles serves every machine. The running app notices a change within a couple of seconds; **Reload configuration** in its menu does it right away. Copies from before Linux support reject a `[linux]` table and fall back to the default shortcuts, so update every machine before adding one.

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

# GNOME apps to leave alone, by app id: the name of the app's desktop file
# without .desktop, like org.gnome.Terminal, or firefox_firefox for Ubuntu's
# Firefox.
[linux]
ignored_apps = ["org.gnome.Terminal"]

# Rebind an action on Linux only, or unbind it with "". Keys are named as for
# Windows, super is the Super key, and a shortcut needs ctrl, alt or super
# unless its key is F1 to F20.
[linux.shortcuts]
lastTwoThirds = "ctrl+alt+super+t"
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

On Linux each shortcut binds by position too: the app asks GNOME Shell to grab the key by its keycode, so a binding stays on the same key whatever the layout, and Command becomes Super. As on Windows, the lock-key positions are never bound. GNOME tells the app only about the shortcuts it grabbed, so `probe` is Windows only.

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
- **Full screen.** On the Mac, windows in full screen (in a Space of their own), minimized windows and sheets are left alone. On Linux, full-screen and minimized windows, and dialogs attached to their window, are left alone.
- **GNOME only.** On Linux the app works under GNOME Shell; other desktops get a line in its menu saying they aren't supported yet.
- **Apps that inhibit shortcuts.** On GNOME, an app that takes all keys while it has the focus, as remote desktop and virtual machine apps do, gets the shortcuts too.

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
  install               put this copy in ~/.local/bin and start it (Linux)
  version               print the version
```

The log is at `%LOCALAPPDATA%\universal-tiling-window-manager\log.txt` on Windows, `~/Library/Logs/universal-tiling-window-manager/log.txt` on the Mac and `~/.local/state/universal-tiling-window-manager/log.txt` on Linux; set `UTWM_LOG=debug` to see every shortcut and where each window landed.

## Development

| Crate | What it is |
|---|---|
| `crates/utwm-config` | config.toml and the import format, bindings for either platform, the macOS key code to Windows scan code translation, and the key matcher the Windows hook runs |
| `crates/utwm-layout` | Where each action puts a window; pure, so its tests run anywhere |
| `crates/utwm-windows` | Win32: the low-level keyboard hook, finding and placing windows, displays, start at login |
| `crates/utwm-macos` | macOS: Carbon hot keys, finding and placing windows through the Accessibility API, displays, start at login (a LaunchAgent) |
| `crates/utwm-linux` | Linux under GNOME: the GNOME Shell extension (`gnome-extension/`) that grabs the shortcuts and moves windows, the D-Bus side that drives it, displays, freeing GNOME's own shortcuts, start at login, and installing under `~/.local` |
| `apps/desktop` | The Tauri 3 tray app and the command line, one codebase for all three platforms |
| `tools/bundle` | `utwm-bundle`, which builds the Mac app bundle (`just bundle`), for both architectures with `--universal`, and installs it into /Applications (`just install`), or on Linux lays out and installs the app under `~/.local` |
| `tools/e2e` | The end-to-end checks (`utwm-e2e`, one per platform), and `utwm-inject`, which presses keys by scan code on Windows the way RustDesk does |

`just check` runs formatting, clippy and the tests. `just run` starts the app from source with debug logging, and `just e2e` then runs the end-to-end check for that platform, which opens a window of its own and moves only that.

On Windows it presses the configured shortcuts the way RustDesk delivers them, by scan code, and checks that each lands exactly where the layout engine says, on any display arrangement and with any configuration. Each key reaches the app as a scan code of its own, so it presses every shortcut the configuration binds, and the keypad's twice: NumLock on, then off, when Windows reports most keypad keys as the navigation keys that share their scan codes. It needs the app running, and puts NumLock back as it found it.

On the Mac it posts the configured shortcuts as key events and checks that each lands exactly where the layout engine says, on any display arrangement and with any export. It needs the app running with Accessibility access, no other window manager answering the same shortcuts, and Accessibility access for the terminal running the check, which posting key events requires.

On Linux it presses the shortcuts through GNOME's remote desktop interface, by key position, and checks each landing as the Mac does. It needs the app running with its extension active and GNOME's conflicting shortcuts freed. `just e2e-headless` needs neither: it runs the same check in a GNOME Shell of its own, with no screen and two virtual displays, its own bus, settings and folders, outside the login session, so the desktop you're using is left alone.

### Releases

Commits follow [Conventional Commits](https://www.conventionalcommits.org): `feat:` and `fix:` are what a release is made of, and pull requests are squash-merged, so a pull request's title is its commit. [release-please](https://github.com/googleapis/release-please) keeps a release pull request open with the next version and its CHANGELOG. Merging it runs `.github/workflows/release.yml`, which tags the release, runs CI at the tag, builds the Mac app for both architectures, signs it with the Developer ID and notarizes it, builds the Windows installer and the Linux executable, and attaches all three to the GitHub release. The updater's `latest.json`, which installed copies read, goes up last, so a release that failed on the way never reaches them. The workflow reads the Developer ID and the updater's signing key from AWS Secrets Manager through GitHub's OIDC, as a role only main can assume (`infra/`, applied by hand).

## License

MIT. Some of the window calculations are ported from MIT-licensed work credited in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
