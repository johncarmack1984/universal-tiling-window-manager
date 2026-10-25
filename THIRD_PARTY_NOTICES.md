# Third-party notices

## Rectangle

The window calculations in `crates/utwm-layout` (which rect each action produces, how repeated presses cycle, gaps, display transfer, and placing windows that refuse their size) are ported from Rectangle's Swift source at [rxhanson/Rectangle@7c5c9c5](https://github.com/rxhanson/Rectangle/tree/7c5c9c56b447dad4e3720b805e54b1d115b1a90e) (v2.0.4). The action names and the settings export format in `crates/utwm-config` follow the same source, and so does the way `crates/utwm-macos` moves windows (size, then position, then size again, with the app's enhanced user interface switched off for the move, as in `AccessibilityElement.swift`) and registers shortcuts (Carbon hot keys, as Rectangle's MASShortcut does). Rectangle is distributed under this license:

```
MIT License

Copyright (c) 2019-2026 Ryan Hanson
Based on the Spectacle app, Copyright (c) 2017 Eric Czarny eczarny@gmail.com

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Rectangle Pro

Rectangle Pro is a closed-source app by the same author. Nothing of it is copied here: this project only reads the settings file Rectangle Pro exports, for interoperability, and reproduces the behaviour of its Pro-only actions from its public documentation and from observing it. It is not affiliated with or endorsed by Rectangle, Rectangle Pro, or Knollsoft.

## PowerToys FancyZones

The techniques in `crates/utwm-windows/src/window.rs` for placing other apps' windows (measuring and compensating for invisible DWM borders, applying a placement a second time when a window crosses to a display with a different scale, restoring a maximized window through its placement) follow the approach of [PowerToys FancyZones](https://github.com/microsoft/PowerToys/tree/main/src/modules/fancyzones), which is MIT-licensed, Copyright (c) Microsoft Corporation. No FancyZones code is included.

## RustDesk

The table that maps macOS key codes to Windows scan codes in `crates/utwm-config/src/keys.rs` mirrors the mapping RustDesk applies in its Map keyboard mode ([rustdesk-org/rdev](https://github.com/rustdesk-org/rdev)), so that a Mac keyboard driving Windows through RustDesk lands on the expected keys. Only the facts of the mapping are used.
