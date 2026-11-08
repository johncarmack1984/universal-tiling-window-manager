// The bridge between universal-tiling-window-manager and GNOME Shell.
//
// On Wayland no ordinary app may move another app's windows or grab keys of
// its own, so the app asks the shell, through this extension, over D-Bus. The
// extension only reports facts and carries out requests: the displays, the
// windows and where they are, moving one window, grabbing the app's
// shortcuts, and drawing the app's menu in the top bar as the app describes
// it. Every decision (which windows to manage, what a shortcut does, where a
// window goes, what the menu says) is the app's.
//
// Shortcuts are grabbed the way GNOME Shell grabs them for
// gnome-settings-daemon (`_grabAcceleratorForSender` in ui/shellDBus.js):
// Mutter's grab_accelerator, allowed in the normal action mode, and released
// when the app leaves the bus, so an app that crashed never keeps a key. The
// menu goes when the app leaves the bus too.

import Clutter from 'gi://Clutter';
import GObject from 'gi://GObject';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

// What the app checks before relying on the bridge. Bumped whenever the
// interface below changes; the app's own copy is `API_VERSION` in shell.rs.
// Version 2 added the menu.
const API_VERSION = 2;

const BUS_NAME = 'com.johncarmack.UniversalTilingWindowManager.Shell';
const OBJECT_PATH = '/com/johncarmack/UniversalTilingWindowManager/Shell';
const INTERFACE_NAME = 'com.johncarmack.UniversalTilingWindowManager.Shell1';

const INTERFACE = `
<node>
  <interface name="${INTERFACE_NAME}">
    <property name="ApiVersion" type="u" access="read"/>
    <property name="Revision" type="s" access="read"/>
    <method name="Screens">
      <arg name="screens" type="a(iiiiiiiidb)" direction="out"/>
    </method>
    <method name="Focused">
      <arg name="window" type="t" direction="out"/>
    </method>
    <method name="Windows">
      <arg name="windows" type="a(ta{sv})" direction="out"/>
    </method>
    <method name="Describe">
      <arg name="window" type="t" direction="in"/>
      <arg name="facts" type="a{sv}" direction="out"/>
    </method>
    <method name="Place">
      <arg name="window" type="t" direction="in"/>
      <arg name="frame" type="(iiii)" direction="in"/>
      <arg name="raise" type="b" direction="in"/>
      <arg name="achieved" type="(iiii)" direction="out"/>
    </method>
    <method name="MoveTo">
      <arg name="window" type="t" direction="in"/>
      <arg name="x" type="i" direction="in"/>
      <arg name="y" type="i" direction="in"/>
      <arg name="achieved" type="(iiii)" direction="out"/>
    </method>
    <method name="Activate">
      <arg name="window" type="t" direction="in"/>
    </method>
    <method name="SetBindings">
      <arg name="generation" type="u" direction="in"/>
      <arg name="accelerators" type="as" direction="in"/>
      <arg name="grabbed" type="ab" direction="out"/>
    </method>
    <signal name="Activated">
      <arg name="generation" type="u"/>
      <arg name="index" type="u"/>
    </signal>
    <signal name="GrabsChanged">
      <arg name="generation" type="u"/>
      <arg name="grabbed" type="ab"/>
    </signal>
    <method name="SetMenu">
      <arg name="name" type="s" direction="in"/>
      <arg name="icon" type="ay" direction="in"/>
      <arg name="items" type="aa{sv}" direction="in"/>
    </method>
    <method name="RemoveMenu"/>
    <signal name="MenuActivated">
      <arg name="id" type="s"/>
    </signal>
  </interface>
</node>`;

// A Wayland app takes its new size when it next draws, so a move is done
// once the frame matches or stops changing. Stopping at the first change
// would report the in-between frame an unmaximize passes through.
const QUIET_BEFORE_CHANGE_MS = 400;
const QUIET_AFTER_CHANGE_MS = 150;
const SETTLE_LIMIT_MS = 1000;

// Settings that can free one of the app's shortcuts, or take one. Mutter
// rebuilds its bindings as soon as they change; gnome-settings-daemon lets
// go of its grabs over D-Bus a little later, hence the second look.
const KEYBINDING_SCHEMAS = [
    'org.gnome.desktop.wm.keybindings',
    'org.gnome.mutter.keybindings',
    'org.gnome.shell.keybindings',
    'org.gnome.settings-daemon.plugins.media-keys',
];
const RETRY_DELAYS_MS = [200, 1000];

// What the app sends: modifiers in a fixed order, then an XKB keycode.
const ACCELERATOR = /^(<Control>)?(<Alt>)?(<Shift>)?(<Super>)?0x([0-9a-f]{2,3})$/;
const MAX_BINDINGS = 200;
// The keycodes of F1 to F24, the only keys grabbed without Ctrl, Alt or
// Super. A grab of a plain typing key would see what people type.
const FUNCTION_KEYS = new Set([
    67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 95, 96,
    191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202,
]);

function isAllowed(accelerator) {
    const match = ACCELERATOR.exec(accelerator);
    if (!match)
        return false;
    const [, control, alt, , superKey, keycode] = match;
    return Boolean(control || alt || superKey) ||
        FUNCTION_KEYS.has(Number.parseInt(keycode, 16));
}

function dbusError(code, message) {
    return new GLib.Error(Gio.DBusError, code, message);
}

function gone(id) {
    return dbusError(Gio.DBusError.UNKNOWN_OBJECT, `window ${id} no longer exists`);
}

function returnError(invocation, error) {
    if (error instanceof GLib.Error) {
        invocation.return_gerror(error);
        return;
    }
    logError(error, `${INTERFACE_NAME}.${invocation.get_method_name()}`);
    invocation.return_gerror(dbusError(Gio.DBusError.FAILED, String(error)));
}

function rect(frame) {
    return [frame.x, frame.y, frame.width, frame.height];
}

function sameFrame(a, b) {
    return a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height;
}

function activeWorkspace() {
    return global.workspace_manager.get_active_workspace();
}

// What the app decides with: everything it needs to know whether to manage a
// window, where it is, and the ids `ignored_apps` names apps by. No titles.
function facts(window) {
    const app = Shell.WindowTracker.get_default().get_window_app(window);
    const b = value => new GLib.Variant('b', Boolean(value));
    const s = value => new GLib.Variant('s', value ?? '');
    return {
        'frame': new GLib.Variant('(iiii)', rect(window.get_frame_rect())),
        'pid': new GLib.Variant('i', window.get_pid()),
        'window-type': new GLib.Variant('u', window.get_window_type()),
        'attached-dialog': b(window.is_attached_dialog()),
        'override-redirect': b(window.is_override_redirect()),
        'minimized': b(window.minimized),
        'fullscreen': b(window.is_fullscreen()),
        'maximized': new GLib.Variant('u', window.get_maximize_flags()),
        'allows-move': b(window.allows_move()),
        'allows-resize': b(window.allows_resize()),
        'skip-taskbar': b(window.is_skip_taskbar()),
        'on-active-workspace': b(window.located_on_workspace(activeWorkspace())),
        'app-id': s(app?.get_id()),
        'sandboxed-app-id': s(window.get_sandboxed_app_id()),
        'gtk-application-id': s(window.get_gtk_application_id()),
        'wm-class': s(window.get_wm_class()),
    };
}

// The app's menu, in the top bar where the AppIndicator extension would put
// its tray icon. GNOME's menus don't open sideways, and that extension shows
// a submenu as a collapsed line with no picture, so here each group of
// actions is a row of their pictures instead, the name and shortcut of each
// showing when it has the pointer or the focus.
const MENU_ROLE = 'universal-tiling-window-manager';
// What the app sends at most: items, and bytes in a picture or the icon.
const MAX_MENU_ITEMS = 500;
const MAX_IMAGE_BYTES = 65536;
// Pictures show 32 pixels across, a row of up to eight to a group.
const PICTURE_SIZE = 32;
const PICTURES_PER_ROW = 8;
// A picture's label shows after the dash's hover delay, then at once for
// each picture the pointer moves on to.
const TOOLTIP_DELAY_MS = 300;
const TOOLTIP_LINGER_MS = 100;
const TOOLTIP_GAP = 6;

function imageBytes(variant) {
    const bytes = variant?.get_data_as_bytes();
    return bytes && bytes.get_size() > 0 && bytes.get_size() <= MAX_IMAGE_BYTES ? bytes : null;
}

// One item of the menu, from the dictionary the app sends. A newer app may
// say more than this reads.
function menuItem(dict) {
    const value = (key, type) => dict.lookup_value(key, new GLib.VariantType(type));
    const text = key => value(key, 's')?.unpack() ?? '';
    const flag = (key, unset) => value(key, 'b')?.unpack() ?? unset;
    return {
        id: text('id'),
        kind: text('kind'),
        label: text('label'),
        group: text('group'),
        shortcut: text('shortcut'),
        enabled: flag('enabled', true),
        checked: flag('checked', false),
        picture: imageBytes(value('picture', 'ay')),
    };
}

// SetMenu's arguments: the menu's name for screen readers, the icon in the
// top bar, and the items in order.
function menuFrom(parameters) {
    const items = parameters.get_child_value(2);
    if (items.n_children() > MAX_MENU_ITEMS) {
        throw dbusError(Gio.DBusError.INVALID_ARGS,
            `a menu of at most ${MAX_MENU_ITEMS} items`);
    }
    return {
        name: parameters.get_child_value(0).unpack(),
        icon: imageBytes(parameters.get_child_value(1)),
        items: Array.from({length: items.n_children()},
            (_, index) => menuItem(items.get_child_value(index))),
    };
}

// The name and shortcut of the picture with the pointer or the focus, in the
// label the dash shows for its icons.
class Tooltip {
    constructor() {
        this._label = new St.Label({style_class: 'dash-label', visible: false});
        Main.layoutManager.uiGroup.add_child(this._label);
        this._owner = null;
        this._pending = null;
        this._showId = 0;
        this._hideId = 0;
    }

    // The pointer came onto `owner`.
    hover(owner, text) {
        if (!this._label)
            return;
        this._clear();
        if (this._label.visible) {
            this._show(owner, text);
            return;
        }
        this._pending = owner;
        this._showId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, TOOLTIP_DELAY_MS, () => {
            this._showId = 0;
            this._pending = null;
            this._show(owner, text);
            return GLib.SOURCE_REMOVE;
        });
    }

    // `owner` took the keyboard focus.
    focus(owner, text) {
        if (!this._label)
            return;
        this._clear();
        this._show(owner, text);
    }

    // The pointer or the focus left `owner`. The label lingers a moment, so
    // it follows the pointer to the next picture without the wait.
    leave(owner) {
        if (this._pending === owner)
            this._clear();
        if (this._owner !== owner || this._hideId)
            return;
        this._hideId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, TOOLTIP_LINGER_MS, () => {
            this._hideId = 0;
            this.hide();
            return GLib.SOURCE_REMOVE;
        });
    }

    hide() {
        this._clear();
        this._label?.hide();
        this._owner = null;
    }

    destroy() {
        this.hide();
        this._label.destroy();
        this._label = null;
    }

    // Under `owner`, or over it at the bottom of the screen.
    _show(owner, text) {
        this._owner = owner;
        this._label.text = text;
        this._label.show();
        Main.layoutManager.uiGroup.set_child_above_sibling(this._label, null);
        const [x, y] = owner.get_transformed_position();
        const [width, height] = owner.get_transformed_size();
        const monitor = Main.layoutManager.findMonitorForActor(owner) ??
            Main.layoutManager.primaryMonitor;
        const labelWidth = this._label.width;
        const labelHeight = this._label.height;
        const left = Math.clamp(Math.round(x + (width - labelWidth) / 2),
            monitor.x, monitor.x + monitor.width - labelWidth);
        const below = Math.round(y + height + TOOLTIP_GAP);
        const top = below + labelHeight <= monitor.y + monitor.height
            ? below
            : Math.round(y - labelHeight - TOOLTIP_GAP);
        this._label.set_position(left, top);
    }

    _clear() {
        if (this._showId)
            GLib.source_remove(this._showId);
        if (this._hideId)
            GLib.source_remove(this._hideId);
        this._showId = 0;
        this._hideId = 0;
        this._pending = null;
    }
}

// One action: its picture, highlighted as a menu item is under the pointer
// or with the focus. An action without a picture shows its name.
function pictureButton(action, tooltip, activate) {
    const text = action.shortcut ? `${action.label} (${action.shortcut})` : action.label;
    const button = new St.Button({
        style_class: 'popup-menu-item',
        style: 'padding: 4px; border-radius: 8px;',
        can_focus: true,
        track_hover: true,
        accessible_name: text,
        child: action.picture
            ? new St.Icon({gicon: Gio.BytesIcon.new(action.picture), icon_size: PICTURE_SIZE})
            : new St.Label({text: action.label, y_align: Clutter.ActorAlign.CENTER}),
    });
    button.connect('notify::hover', () => {
        if (button.hover)
            tooltip.hover(button, text);
        else
            tooltip.leave(button);
    });
    button.connect('key-focus-in', () => {
        button.add_style_pseudo_class('selected');
        tooltip.focus(button, text);
    });
    button.connect('key-focus-out', () => {
        button.remove_style_pseudo_class('selected');
        tooltip.leave(button);
    });
    button.connect('clicked', () => activate(action.id));
    return button;
}

// A group of actions: its name, then its pictures.
const PictureRow = GObject.registerClass(
class PictureRow extends PopupMenu.PopupBaseMenuItem {
    _init(group, actions, tooltip, activate) {
        super._init({reactive: false, can_focus: false});
        this.style = 'padding-top: 2px; padding-bottom: 2px;';
        this.add_child(new St.Label({
            text: group.label,
            y_align: Clutter.ActorAlign.CENTER,
            style: 'min-width: 6.5em;',
        }));
        const rows = new St.BoxLayout({orientation: Clutter.Orientation.VERTICAL});
        for (let start = 0; start < actions.length; start += PICTURES_PER_ROW) {
            const row = new St.BoxLayout({style: 'spacing: 2px;'});
            for (const action of actions.slice(start, start + PICTURES_PER_ROW))
                row.add_child(pictureButton(action, tooltip, activate));
            rows.add_child(row);
        }
        this.add_child(rows);
    }
});

// The app's icon in the top bar and the menu under it. It stays off the lock
// screen, which shows the top bar too.
class AppMenu {
    constructor(activate) {
        this._activate = activate;
        this._button = new PanelMenu.Button(0.0, MENU_ROLE);
        this._icon = new St.Icon({style_class: 'system-status-icon'});
        this._button.add_child(this._icon);
        this._tooltip = new Tooltip();
        // A menu that changes while it's open waits until it closes.
        this._waiting = null;
        this._button.menu.connect('open-state-changed', (menu, open) => {
            if (open)
                return;
            this._tooltip.hide();
            if (this._waiting) {
                this._fill(this._waiting);
                this._waiting = null;
            }
        });
        Main.panel.addToStatusArea(MENU_ROLE, this._button, 1, 'right');
        this._sessionId = Main.sessionMode.connect('updated', () => this._sync());
        this._sync();
    }

    set(menu) {
        this._button.accessible_name = menu.name;
        this._icon.gicon = menu.icon ? Gio.BytesIcon.new(menu.icon) : null;
        if (this._button.menu.isOpen)
            this._waiting = menu.items;
        else
            this._fill(menu.items);
    }

    destroy() {
        Main.sessionMode.disconnect(this._sessionId);
        this._button.destroy();
        this._tooltip.destroy();
    }

    _fill(items) {
        const menu = this._button.menu;
        menu.removeAll();
        const actions = new Map();
        for (const item of items.filter(candidate => candidate.kind === 'action')) {
            if (!actions.has(item.group))
                actions.set(item.group, []);
            actions.get(item.group).push(item);
        }
        for (const item of items) {
            const shown = this._item(item, actions.get(item.id) ?? []);
            if (shown)
                menu.addMenuItem(shown);
        }
    }

    _item(item, actions) {
        switch (item.kind) {
        case 'separator':
            return new PopupMenu.PopupSeparatorMenuItem();
        case 'group':
            return new PictureRow(item, actions, this._tooltip, id => {
                this._button.menu.itemActivated();
                this._activate(id);
            });
        case 'check': {
            // Space flips a switch without closing the menu, so the switch
            // reports flipping, not activating.
            const toggle = new PopupMenu.PopupSwitchMenuItem(item.label, item.checked);
            toggle.setSensitive(item.enabled);
            toggle.connect('toggled', () => this._activate(item.id));
            return toggle;
        }
        case 'text': {
            const line = new PopupMenu.PopupMenuItem(item.label);
            line.setSensitive(item.enabled);
            line.connect('activate', () => this._activate(item.id));
            return line;
        }
        default:
            return null;
        }
    }

    _sync() {
        const locked = Main.sessionMode.isLocked;
        if (locked)
            this._button.menu.close();
        this._button.container.visible = !locked;
    }
}

class Bridge {
    constructor(revision) {
        this._revision = revision;
        // One app holds the shortcuts at a time: its unique bus name, and each
        // accelerator it asked for with Mutter's action for it (NONE when
        // refused), by the app's index.
        this._client = null;
        this._clientWatchId = 0;
        this._generation = 0;
        this._grabs = [];
        this._retryIds = [];
        // Moves still settling, so turning the extension off can answer them.
        this._pending = new Set();
        // One app shows its menu at a time: its unique bus name, and the
        // menu in the top bar.
        this._menuOwner = null;
        this._menuOwnerWatchId = 0;
        this._menu = null;

        this._activatedId = global.display.connect('accelerator-activated',
            (display, action) => this._onAcceleratorActivated(action));
        this._keymapId = global.backend.connect('keymap-changed',
            () => this._scheduleRetry());
        const schemas = Gio.SettingsSchemaSource.get_default();
        this._settings = KEYBINDING_SCHEMAS
            .filter(id => schemas.lookup(id, true) !== null)
            .map(id => {
                const settings = new Gio.Settings({schema_id: id});
                const changedId = settings.connect('changed', () => this._scheduleRetry());
                // GSettings only promises change signals for keys read once.
                for (const key of settings.settings_schema.list_keys())
                    settings.get_value(key);
                return [settings, changedId];
            });

        this._impl = Gio.DBusExportedObject.wrapJSObject(INTERFACE, this);
        this._impl.export(Gio.DBus.session, OBJECT_PATH);
        this._nameId = Gio.DBus.session.own_name(BUS_NAME,
            Gio.BusNameOwnerFlags.NONE, null, null);
    }

    destroy() {
        // The name goes first, so the app sees the bridge leave before
        // anything stops answering.
        Gio.DBus.session.unown_name(this._nameId);
        this._release();
        this._removeMenu();
        this._clearRetries();
        for (const [settings, changedId] of this._settings)
            settings.disconnect(changedId);
        this._settings = [];
        global.backend.disconnect(this._keymapId);
        global.display.disconnect(this._activatedId);
        for (const finish of [...this._pending])
            finish(dbusError(Gio.DBusError.FAILED, 'the extension was turned off'));
        this._impl.unexport();
    }

    get ApiVersion() {
        return API_VERSION;
    }

    get Revision() {
        return this._revision;
    }

    Screens() {
        const display = global.display;
        const workspace = activeWorkspace();
        const primary = display.get_primary_monitor();
        const screens = [];
        for (let monitor = 0; monitor < display.get_n_monitors(); monitor++) {
            const frame = display.get_monitor_geometry(monitor);
            const work = workspace.get_work_area_for_monitor(monitor);
            screens.push([
                ...rect(frame),
                ...rect(work),
                display.get_monitor_scale(monitor),
                monitor === primary,
            ]);
        }
        return screens;
    }

    Focused() {
        this._checkUnlocked();
        const window = global.display.get_focus_window();
        if (!window)
            return 0;
        // A dialog attached to its window moves with it.
        return (window.is_attached_dialog() ? window.find_root_ancestor() : window).get_id();
    }

    Windows() {
        this._checkUnlocked();
        const workspace = activeWorkspace();
        const windows = global.display.list_all_windows()
            .filter(window => window.located_on_workspace(workspace));
        return global.display.sort_windows_by_stacking(windows)
            .reverse()
            .map(window => [window.get_id(), facts(window)]);
    }

    Describe(id) {
        this._checkUnlocked();
        return facts(this._window(id));
    }

    async PlaceAsync([id, [x, y, width, height], raise], invocation) {
        try {
            this._checkUnlocked();
            const window = this._window(id);
            // A maximized window, or one GNOME has tiled to half the screen,
            // is held there until it is back to normal.
            if (window.get_maximize_flags() !== 0)
                window.unmaximize();
            // Some apps (GNOME Terminal among them) ignore a move that comes
            // with a resize, so the move goes first on its own, as Ubuntu's
            // tiling assistant does it.
            window.move_frame(true, x, y);
            window.move_resize_frame(true, x, y, width, height);
            const achieved = await this._settle(window, {x, y, width, height});
            if (raise)
                window.raise();
            invocation.return_value(new GLib.Variant('((iiii))', [rect(achieved)]));
        } catch (error) {
            returnError(invocation, error);
        }
    }

    async MoveToAsync([id, x, y], invocation) {
        try {
            this._checkUnlocked();
            const window = this._window(id);
            const {width, height} = window.get_frame_rect();
            window.move_frame(true, x, y);
            const achieved = await this._settle(window, {x, y, width, height});
            invocation.return_value(new GLib.Variant('((iiii))', [rect(achieved)]));
        } catch (error) {
            returnError(invocation, error);
        }
    }

    Activate(id) {
        this._checkUnlocked();
        this._window(id).activate(global.get_current_time());
    }

    SetBindingsAsync([generation, accelerators], invocation) {
        const sender = invocation.get_sender();
        if (this._client !== null && this._client !== sender) {
            invocation.return_gerror(dbusError(Gio.DBusError.ACCESS_DENIED,
                'another app holds the shortcuts'));
            return;
        }
        if (accelerators.length > MAX_BINDINGS || !accelerators.every(isAllowed)) {
            invocation.return_gerror(dbusError(Gio.DBusError.INVALID_ARGS,
                'only Ctrl, Alt or Super chords, or function keys, by keycode'));
            return;
        }
        if (accelerators.length === 0) {
            this._release();
            invocation.return_value(new GLib.Variant('(ab)', [[]]));
            return;
        }
        if (this._client === null) {
            this._client = sender;
            this._clientWatchId = Gio.bus_watch_name(Gio.BusType.SESSION, sender,
                Gio.BusNameWatcherFlags.NONE, null, () => this._release());
        }
        this._generation = generation;
        // A shortcut that stays keeps its grab, so a reload never drops a key.
        const kept = new Map();
        for (const {accelerator, action} of this._grabs) {
            if (action === Meta.KeyBindingAction.NONE)
                continue;
            if (accelerators.includes(accelerator) && !kept.has(accelerator))
                kept.set(accelerator, action);
            else
                this._ungrab(action);
        }
        this._grabs = accelerators.map(accelerator => ({
            accelerator,
            action: kept.get(accelerator) ?? this._grab(accelerator),
        }));
        invocation.return_value(new GLib.Variant('(ab)', [this._grabbed()]));
    }

    // Puts the app's menu in the top bar, or replaces the one there. It
    // stays until the app removes it or leaves the bus.
    SetMenuAsync(params, invocation) {
        try {
            const sender = invocation.get_sender();
            if (this._menuOwner !== null && this._menuOwner !== sender) {
                throw dbusError(Gio.DBusError.ACCESS_DENIED,
                    'another app shows its menu');
            }
            const menu = menuFrom(invocation.get_parameters());
            if (this._menuOwner === null) {
                this._menuOwner = sender;
                this._menuOwnerWatchId = Gio.bus_watch_name(Gio.BusType.SESSION, sender,
                    Gio.BusNameWatcherFlags.NONE, null, () => this._removeMenu());
            }
            this._menu ??= new AppMenu(id => this._emitTo(this._menuOwner,
                'MenuActivated', new GLib.Variant('(s)', [id])));
            this._menu.set(menu);
            invocation.return_value(null);
        } catch (error) {
            returnError(invocation, error);
        }
    }

    RemoveMenuAsync(params, invocation) {
        if (this._menuOwner === invocation.get_sender())
            this._removeMenu();
        invocation.return_value(null);
    }

    _checkUnlocked() {
        if (Main.sessionMode.isLocked)
            throw dbusError(Gio.DBusError.ACCESS_DENIED, 'the screen is locked');
    }

    _window(id) {
        const window = global.display.list_all_windows()
            .find(candidate => candidate.get_id() === id);
        if (!window)
            throw gone(id);
        return window;
    }

    // Resolves with the frame once it matches `target` or holds still.
    _settle(window, target) {
        return new Promise((resolve, reject) => {
            if (sameFrame(window.get_frame_rect(), target)) {
                resolve(window.get_frame_rect());
                return;
            }
            let changed = false;
            let done = false;
            let quietId = 0;
            let limitId = 0;
            const signalIds = [];
            const finish = error => {
                if (done)
                    return;
                done = true;
                for (const signalId of signalIds)
                    window.disconnect(signalId);
                if (quietId)
                    GLib.source_remove(quietId);
                if (limitId)
                    GLib.source_remove(limitId);
                this._pending.delete(finish);
                if (error)
                    reject(error);
                else
                    resolve(window.get_frame_rect());
            };
            const waitForQuiet = () => {
                if (quietId)
                    GLib.source_remove(quietId);
                const delay = changed ? QUIET_AFTER_CHANGE_MS : QUIET_BEFORE_CHANGE_MS;
                quietId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, delay, () => {
                    quietId = 0;
                    finish();
                    return GLib.SOURCE_REMOVE;
                });
            };
            const onChange = () => {
                changed = true;
                if (sameFrame(window.get_frame_rect(), target))
                    finish();
                else
                    waitForQuiet();
            };
            signalIds.push(window.connect('size-changed', onChange));
            signalIds.push(window.connect('position-changed', onChange));
            signalIds.push(window.connect('unmanaging', () => finish(gone(window.get_id()))));
            limitId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SETTLE_LIMIT_MS, () => {
                limitId = 0;
                finish();
                return GLib.SOURCE_REMOVE;
            });
            this._pending.add(finish);
            waitForQuiet();
        });
    }

    _grab(accelerator) {
        const action = global.display.grab_accelerator(accelerator,
            Meta.KeyBindingFlags.IGNORE_AUTOREPEAT);
        if (action !== Meta.KeyBindingAction.NONE) {
            Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action),
                Shell.ActionMode.NORMAL);
        }
        return action;
    }

    _ungrab(action) {
        if (global.display.ungrab_accelerator(action)) {
            Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action),
                Shell.ActionMode.NONE);
        }
    }

    _grabbed() {
        return this._grabs.map(({action}) => action !== Meta.KeyBindingAction.NONE);
    }

    // Drops every grab: the app asked for none, left the bus, or the
    // extension is turning off.
    _release() {
        for (const {action} of this._grabs) {
            if (action !== Meta.KeyBindingAction.NONE)
                this._ungrab(action);
        }
        this._grabs = [];
        if (this._clientWatchId)
            Gio.bus_unwatch_name(this._clientWatchId);
        this._clientWatchId = 0;
        this._client = null;
    }

    _onAcceleratorActivated(action) {
        const index = this._grabs.findIndex(grab => grab.action === action);
        if (index >= 0) {
            this._emitTo(this._client, 'Activated',
                new GLib.Variant('(uu)', [this._generation, index]));
        }
    }

    _emitTo(destination, signal, parameters) {
        if (destination === null)
            return;
        this._impl.get_connection()?.emit_signal(destination, OBJECT_PATH,
            INTERFACE_NAME, signal, parameters);
    }

    // Takes the menu out of the top bar: the app asked, left the bus, or the
    // extension is turning off.
    _removeMenu() {
        this._menu?.destroy();
        this._menu = null;
        if (this._menuOwnerWatchId)
            Gio.bus_unwatch_name(this._menuOwnerWatchId);
        this._menuOwnerWatchId = 0;
        this._menuOwner = null;
    }

    _scheduleRetry() {
        this._clearRetries();
        this._retryIds = RETRY_DELAYS_MS.map((delay, slot) =>
            GLib.timeout_add(GLib.PRIORITY_DEFAULT, delay, () => {
                this._retryIds[slot] = 0;
                this._retry();
                return GLib.SOURCE_REMOVE;
            }));
    }

    _clearRetries() {
        for (const retryId of this._retryIds) {
            if (retryId)
                GLib.source_remove(retryId);
        }
        this._retryIds = [];
    }

    // Grabs what was refused before, if a settings or keymap change freed it.
    _retry() {
        let changed = false;
        for (const grab of this._grabs) {
            if (grab.action !== Meta.KeyBindingAction.NONE)
                continue;
            grab.action = this._grab(grab.accelerator);
            changed ||= grab.action !== Meta.KeyBindingAction.NONE;
        }
        if (changed) {
            this._emitTo(this._client, 'GrabsChanged',
                new GLib.Variant('(uab)', [this._generation, this._grabbed()]));
        }
    }
}

export default class UniversalTilingWindowManagerExtension extends Extension {
    enable() {
        this._bridge = new Bridge(this.metadata.revision ?? '');
    }

    disable() {
        this._bridge?.destroy();
        this._bridge = null;
    }
}
