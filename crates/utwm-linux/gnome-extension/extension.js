// The bridge between universal-tiling-window-manager and GNOME Shell.
//
// On Wayland no ordinary app may move another app's windows or grab keys of
// its own, so the app asks the shell, through this extension, over D-Bus. The
// extension only reports facts and carries out requests: the displays, the
// windows and where they are, moving one window, and grabbing the app's
// shortcuts. Every decision (which windows to manage, what a shortcut does,
// where a window goes) is the app's.
//
// Shortcuts are grabbed the way GNOME Shell grabs them for
// gnome-settings-daemon (`_grabAcceleratorForSender` in ui/shellDBus.js):
// Mutter's grab_accelerator, allowed in the normal action mode, and released
// when the app leaves the bus, so an app that crashed never keeps a key.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

// What the app checks before relying on the bridge. Bumped whenever the
// interface below changes; the app's own copy is `API_VERSION` in shell.rs.
const API_VERSION = 1;

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
        if (index >= 0)
            this._emit('Activated', new GLib.Variant('(uu)', [this._generation, index]));
    }

    _emit(signal, parameters) {
        if (this._client === null)
            return;
        this._impl.get_connection()?.emit_signal(this._client, OBJECT_PATH,
            INTERFACE_NAME, signal, parameters);
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
            this._emit('GrabsChanged',
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
