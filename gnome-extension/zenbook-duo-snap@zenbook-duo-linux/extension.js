// Zenbook Duo Snap: while a window is dragged, a popup shows the top screen,
// the bottom screen, "both screens" and any external screens as small layout
// icons. Dropping on a
// cell of an icon places (tiles) the window there; "Free" puts it on that
// screen as a normal window at its pre-tiling size. Turned-off screens are
// greyed out. Tiled neighbours resize together, and dragging a tiled window
// away restores its size. Keybindings swap the windows between the screens
// (F8 on the Duo keyboard sends XF86Launch2) or move the focused window to the
// other screen.

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as KeyboardUI from 'resource:///org/gnome/shell/ui/keyboard.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import {SCREEN_LAYOUTS, SPAN_LAYOUTS, zoneRect, spanRect, hitTest, stackedPair, popupPosition, edgeConstraints, linkedResize, freeRect, restoredUnderPointer, monitorAt, keyboardScreenFor, externalScreens, micVolumeIcon, namedScreens, defaultSwapPair, isExternal, keyboardScreenForTouch, swipeScreen, FingerHold} from './zones.js';
import {ScreenPickDialog} from './pickDialog.js';
import {DisplayDialog, changeDisplays} from './displayDialog.js';
import {setTopThroughGnome} from './displayConfig.js';
import {AlertBubble} from './alertBubble.js';

// Called by the Zenbook Duo services for on-screen pop-ups like GNOME's
// volume and brightness ones: mic volume (mic key + Up/Down), Fn lock,
// keyboard backlight, the screen key, and app keys without a shortcut.
const DBUS_INTERFACE = `<node>
  <interface name="org.gnome.Shell.Extensions.ZenbookDuoSnap">
    <method name="ShowMicVolume">
      <arg type="d" direction="in" name="level"/>
      <arg type="b" direction="in" name="muted"/>
    </method>
    <method name="ShowOsd">
      <arg type="s" direction="in" name="icon"/>
      <arg type="s" direction="in" name="label"/>
      <arg type="d" direction="in" name="level"/>
    </method>
    <method name="ShiftF7Pressed"/>
    <method name="SetTopBrightness">
      <arg type="d" direction="in" name="percent"/>
      <arg type="b" direction="out" name="done"/>
    </method>
    <method name="ShowAlert">
      <arg type="s" direction="in" name="id"/>
      <arg type="s" direction="in" name="icon"/>
      <arg type="s" direction="in" name="title"/>
      <arg type="s" direction="in" name="message"/>
    </method>
    <method name="AppKeyPressed">
      <arg type="s" direction="in" name="key"/>
      <arg type="s" direction="in" name="accelerator"/>
    </method>
  </interface>
</node>`;

// Keybinding settings that may hold a shortcut for an app key.
const KEYBINDING_SCHEMAS = [
    'org.gnome.settings-daemon.plugins.media-keys',
    'org.gnome.desktop.wm.keybindings',
    'org.gnome.shell.keybindings',
    'org.gnome.mutter.keybindings',
    'org.gnome.mutter.wayland.keybindings',
];
const CUSTOM_KEYBINDING_SCHEMA = 'org.gnome.settings-daemon.plugins.media-keys.custom-keybinding';

/** Whether some GNOME shortcut (or this extension) uses exactly this accelerator. */
function acceleratorAssigned(accelerator, ownSettings) {
    const wanted = accelerator.toLowerCase();
    const has = value => (Array.isArray(value) ? value : [value])
        .some(v => typeof v === 'string' && v.toLowerCase() === wanted);
    const source = Gio.SettingsSchemaSource.get_default();
    for (const id of KEYBINDING_SCHEMAS) {
        const schema = source.lookup(id, true);
        if (!schema)
            continue;
        const settings = new Gio.Settings({schema_id: id});
        if (schema.list_keys().some(key => has(settings.get_value(key).recursiveUnpack())))
            return true;
        if (id === KEYBINDING_SCHEMAS[0] && source.lookup(CUSTOM_KEYBINDING_SCHEMA, true)) {
            for (const path of settings.get_strv('custom-keybindings')) {
                const custom = new Gio.Settings({schema_id: CUSTOM_KEYBINDING_SCHEMA, path});
                if (has(custom.get_string('binding')))
                    return true;
            }
        }
    }
    return ownSettings.list_keys().some(key => has(ownSettings.get_value(key).recursiveUnpack()));
}
const DBUS_PATH = '/org/gnome/Shell/Extensions/ZenbookDuoSnap';

// The Zenbook Duo daemon's state; "status.keyboardAttached" tells whether the
// keyboard sits on the bottom screen.
const DAEMON_STATE = '/var/lib/zenbook-duo/state.json';

const TOP_CONNECTOR = 'eDP-1';
const BOTTOM_CONNECTOR = 'eDP-2';
// How long Super+P (detached F7) waits for the daemon's word that it was Shift+F7.
const SHIFT_F7_WINDOW_MS = 200;
// Pointer distance before the popup appears, so clicking a title bar does not flash it.
const SHOW_THRESHOLD = 24;
// Pointer distance before a dragged tiled window gets its free size back.
const RESTORE_THRESHOLD = 10;
// Pointer polling interval while dragging; no input events reach the shell UI
// during a window grab.
const POLL_MS = 16;

/** Monitor indices of the top and bottom panel, or -1 when a panel is off. */
function internalScreens() {
    const manager = global.backend.get_monitor_manager();
    const top = manager.get_monitor_for_connector(TOP_CONNECTOR);
    const bottom = manager.get_monitor_for_connector(BOTTOM_CONNECTOR);
    if (top >= 0 || bottom >= 0)
        return {top, bottom};
    // Not the Duo's panels (e.g. a nested test session): a pair of monitors
    // stacked on top of each other.
    return stackedPair(Main.layoutManager.monitors);
}

/** Where the on-screen keyboard's top edge lies when shown on the bottom screen. */
/**
 * The monitor index of a popup screen: "top", "bottom" or "external-<n>"
 * (the n-th external screen from the left), or -1 when it is not there.
 */
function screenMonitor(screen) {
    const screens = internalScreens();
    if (screen in screens)
        return screens[screen];
    const n = Number(screen.replace('external-', ''));
    return externalScreens(Main.layoutManager.monitors, screens)[n] ?? -1;
}

function keyboardTopOnBottomScreen(screens) {
    const monitor = Main.layoutManager.monitors[screens.bottom];
    if (!monitor)
        return Infinity;
    // Before the keyboard has been shown its height is 0: assume a typical one.
    const height = Main.layoutManager.keyboardBox.height || Math.round(monitor.height * 0.4);
    return monitor.y + monitor.height - height;
}

function workArea(monitor) {
    const area = Main.layoutManager.getWorkAreaForMonitor(monitor);
    return {x: area.x, y: area.y, width: area.width, height: area.height};
}

function actorRect(actor) {
    const [x, y] = actor.get_transformed_position();
    const [width, height] = actor.get_transformed_size();
    return {x, y, width, height};
}

// Every resize grab op (pointer and keyboard, any edge or corner).
const RESIZE_OPS = new Set(Object.entries(Meta.GrabOp)
    .filter(([name]) => name.includes('RESIZING'))
    .map(([, value]) => value));

// Mutter adds this flag to grab ops that ignore the usual constraints
// (e.g. MOVING_UNCONSTRAINED = MOVING | 1024).
const GRAB_OP_UNCONSTRAINED = 1024;

function isResizeOp(op) {
    return RESIZE_OPS.has(op & ~GRAB_OP_UNCONSTRAINED);
}

/**
 * Where the window is being dragged from: a finger on the touchscreen when one
 * is down (the mouse pointer does not move during a touch drag), otherwise the
 * mouse pointer.
 */
function dragPosition() {
    let touch = null;
    global.stage.foreach_sprite((_stage, sprite) => {
        if (!touch && sprite.get_role() === Clutter.SpriteRole.TOUCHPOINT)
            touch = sprite;
        return true;
    });
    if (touch) {
        const point = touch.get_coords();
        return [point.x, point.y];
    }
    const [x, y] = global.get_pointer();
    return [x, y];
}

function frameRect(window) {
    const r = window.get_frame_rect();
    return {x: r.x, y: r.y, width: r.width, height: r.height};
}

function isMoveOp(op) {
    return op === Meta.GrabOp.MOVING || op === Meta.GrabOp.MOVING_UNCONSTRAINED;
}

function isNormalWindow(window) {
    return window && window.get_window_type() === Meta.WindowType.NORMAL;
}

class SnapPopup {
    constructor(screens, externals) {
        // Drop targets: the cells of the layout icons.
        this._targets = [];
        this._hovered = null;

        this.actor = new St.BoxLayout({style_class: 'duo-snap-popup', reactive: false});
        const column = new St.BoxLayout({
            style_class: 'duo-snap-column',
            orientation: Clutter.Orientation.VERTICAL,
        });
        column.add_child(this._screenBox('top', 'Top screen', screens.top >= 0));
        column.add_child(this._screenBox('bottom', 'Bottom screen', screens.bottom >= 0));
        this.actor.add_child(column);
        this.actor.add_child(this._spanBox(screens.top >= 0 && screens.bottom >= 0));
        // External screens to the right, two per column like the panels.
        for (let i = 0; i < externals.length; i += 2) {
            const externalColumn = new St.BoxLayout({
                style_class: 'duo-snap-column',
                orientation: Clutter.Orientation.VERTICAL,
            });
            for (let n = i; n < Math.min(i + 2, externals.length); n++)
                externalColumn.add_child(this._screenBox(`external-${n}`, `External ${n + 1}`, true));
            this.actor.add_child(externalColumn);
        }
    }

    _screenBox(screen, label, enabled) {
        const box = this._box(label, enabled);
        const grid = new St.BoxLayout({
            style_class: 'duo-snap-layouts',
            orientation: Clutter.Orientation.VERTICAL,
        });
        // Three layout icons per row.
        for (let i = 0; i < SCREEN_LAYOUTS.length; i += 3) {
            const line = new St.BoxLayout({style_class: 'duo-snap-layouts-row'});
            for (const layout of SCREEN_LAYOUTS.slice(i, i + 3)) {
                line.add_child(this._layoutIcon(layout, 'duo-snap-layout', box, enabled,
                    zone => zone === 'free'
                        ? {kind: 'free', screen}
                        : {kind: 'zone', screen, zone}));
            }
            grid.add_child(line);
        }
        box.add_child(grid);
        return box;
    }

    _spanBox(enabled) {
        const box = this._box('Both', enabled);
        box.add_style_class_name('duo-snap-span');
        const icons = new St.BoxLayout({
            style_class: 'duo-snap-layouts',
            orientation: Clutter.Orientation.VERTICAL,
            y_expand: true,
        });
        // The icons fill the box's height, matching the two screen boxes.
        for (const layout of SPAN_LAYOUTS) {
            const icon = this._layoutIcon(layout, 'duo-snap-layout duo-snap-span-layout',
                box, enabled, zone => ({kind: 'span', zone}));
            icon.y_expand = true;
            icons.add_child(icon);
        }
        box.add_child(icons);
        return box;
    }

    _box(label, enabled) {
        const box = new St.BoxLayout({
            style_class: 'duo-snap-box',
            orientation: Clutter.Orientation.VERTICAL,
        });
        box.add_child(new St.Label({text: label, style_class: 'duo-snap-label'}));
        if (!enabled)
            box.opacity = 90; // greyed out: that screen is off
        return box;
    }

    /** A small picture of a layout; each of its cells is a drop target. */
    _layoutIcon(layout, styleClass, box, enabled, targetFor) {
        const icon = new St.BoxLayout({
            style_class: styleClass,
            orientation: Clutter.Orientation.VERTICAL,
        });
        for (const row of layout.rows) {
            const line = new St.BoxLayout({style_class: 'duo-snap-cells', y_expand: true});
            for (const zone of row) {
                const cell = new St.Bin({
                    style_class: 'duo-snap-zone',
                    x_expand: true,
                    y_expand: true,
                });
                if (zone === 'free')
                    cell.set_child(new St.Label({text: 'Free', style_class: 'duo-snap-free-label'}));
                line.add_child(cell);
                this._targets.push({target: targetFor(zone), actor: cell, enabled, box});
            }
            icon.add_child(line);
        }
        return icon;
    }

    /** Highlights and returns the drop target under the pointer, or null. */
    update(px, py) {
        const hit = hitTest(
            this._targets.map(e => ({target: e, rect: actorRect(e.actor), enabled: e.enabled})),
            px, py);
        if (hit !== this._hovered) {
            if (this._hovered) {
                this._hovered.actor.remove_style_pseudo_class('hover');
                this._hovered.box.remove_style_pseudo_class('active');
            }
            this._hovered = hit;
            if (hit) {
                hit.actor.add_style_pseudo_class('hover');
                hit.box.add_style_pseudo_class('active');
            }
        }
        return hit ? hit.target : null;
    }

    destroy() {
        this.actor.destroy();
    }
}

export default class ZenbookDuoSnapExtension extends Extension {
    enable() {
        this._settings = this.getSettings();
        this._drag = null;
        // Windows this extension tiled (edge constraints set) -> 'unmanaged' handler id.
        this._tiled = new Map();
        // The linked resize in progress: {window, sizeChangedId}.
        this._resize = null;
        // Each window's size from before this extension first tiled it.
        this._freeSizes = new WeakMap();
        this._grabBeginId = global.display.connect('grab-op-begin',
            (_display, window, op) => this._onGrabBegin(window, op));
        this._grabEndId = global.display.connect('grab-op-end',
            (_display, window, op) => this._onGrabEnd(window, op));

        const flags = Meta.KeyBindingFlags.NONE;
        const modes = Shell.ActionMode.NORMAL;
        Main.wm.addKeybinding('swap-windows', this._settings, flags, modes,
            () => this._swapWindows());
        Main.wm.addKeybinding('move-window-to-other-screen', this._settings, flags, modes,
            () => this._moveFocusedWindow());
        Main.wm.addKeybinding('display-controls', this._settings, flags, modes,
            () => this._openDisplayDialog());
        Main.wm.addKeybinding('main-display-switch', this._settings, flags, modes,
            () => this._askMainDisplay());
        this._superPId = this._settings.connect('changed::display-controls-on-super-p',
            () => this._syncSuperP());
        this._syncSuperP();

        this._keyboardAttached = false;
        this._watchDaemonState();
        this._lastTouchMonitor = -1;
        this._touchEventId = global.stage.connect('captured-event',
            (_stage, event) => this._onCapturedEvent(event));
        this._touchKeyboardId = this._settings.connect('changed::touch-keyboard',
            () => this._syncTouchKeyboard());
        this._syncTouchKeyboard();
        this._patchKeyboardScreen();
        this._restrictKeyboardSwipe();
        this._alerts = new Map();
        this._coveredKept = false;
        this._coveredMonitorsId = Main.layoutManager.connect('monitors-changed', () => this._checkCovered());
        this._dbus = Gio.DBusExportedObject.wrapJSObject(DBUS_INTERFACE, this);
        this._dbus.export(Gio.DBus.session, DBUS_PATH);
    }

    disable() {
        global.stage.disconnect(this._touchEventId);
        this._stateMonitor.cancel();
        this._stateMonitor = null;
        if (this._stateReadId)
            GLib.source_remove(this._stateReadId);
        this._stateReadId = 0;
        this._settings.disconnect(this._touchKeyboardId);
        this._restoreTouchKeyboard();
        this._restoreKeyboardScreen();
        this._releaseKeyboardSwipe();
        this._dbus.unexport();
        this._dbus = null;
        Main.layoutManager.disconnect(this._coveredMonitorsId);
        if (this._coveredTimeoutId)
            GLib.source_remove(this._coveredTimeoutId);
        this._coveredTimeoutId = 0;
        for (const alert of [...this._alerts.values()])
            alert.dismiss();
        this._alerts = null;
        this._pickDialog?.close();
        this._pickDialog = null;
        Main.wm.removeKeybinding('swap-windows');
        Main.wm.removeKeybinding('move-window-to-other-screen');
        Main.wm.removeKeybinding('display-controls');
        Main.wm.removeKeybinding('main-display-switch');
        this._clearTimer('_superPTimerId');
        this._clearTimer('_shiftF7TimerId');
        this._clearFingerHoldTimer();
        this._fingerHold = null;
        this._settings.disconnect(this._superPId);
        this._restoreSuperP();
        this._displayDialog?.close();
        this._displayDialog = null;
        global.display.disconnect(this._grabBeginId);
        global.display.disconnect(this._grabEndId);
        this._endDrag();
        this._endLinkedResize();
        for (const window of [...this._tiled.keys()])
            this._untile(window);
        this._tiled = null;
        this._freeSizes = null;
        this._settings = null;
    }

    _onGrabBegin(window, op) {
        if (isResizeOp(op) && this._tiled.has(window)) {
            this._endLinkedResize();
            this._startLinkedResize(window);
            return;
        }
        if (!isMoveOp(op) || !isNormalWindow(window))
            return;
        // Dragging a window this extension tiled releases it again, and gives
        // it back its free size once it has really moved (see _poll).
        const restoreSize = this._tiled.has(window) ? this._freeSizes.get(window) : null;
        this._untile(window);
        this._endDrag();
        const [x, y] = dragPosition();
        this._drag = {
            window, startX: x, startY: y, popup: null, target: null, timerId: 0, restoreSize,
        };
        this._drag.timerId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, POLL_MS, () => this._poll());
    }

    _poll() {
        const drag = this._drag;
        if (!drag)
            return GLib.SOURCE_REMOVE;
        const [x, y] = dragPosition();
        if (drag.restoreSize && Math.hypot(x - drag.startX, y - drag.startY) > RESTORE_THRESHOLD) {
            const {x: rx, y: ry, width, height} =
                restoredUnderPointer(frameRect(drag.window), drag.restoreSize, x);
            if (drag.window.get_maximize_flags() !== 0)
                drag.window.unmaximize();
            drag.window.move_resize_frame(true, rx, ry, width, height);
            drag.restoreSize = null;
        }
        if (!drag.popup) {
            if (Math.hypot(x - drag.startX, y - drag.startY) < SHOW_THRESHOLD)
                return GLib.SOURCE_CONTINUE;
            this._showPopup(drag);
        }
        drag.target = drag.popup.update(x, y);
        return GLib.SOURCE_CONTINUE;
    }

    _showPopup(drag) {
        const screens = internalScreens();
        drag.popup = new SnapPopup(screens, externalScreens(Main.layoutManager.monitors, screens));
        Main.uiGroup.add_child(drag.popup.actor);
        // Next to the pointer, so it is a short move away, but not under it.
        const [px, py] = dragPosition();
        // The screen under the finger or pointer: Mutter's current monitor
        // follows only the mouse pointer, also during touch drags.
        const monitor = monitorAt(Main.layoutManager.monitors, px, py);
        const [, width] = drag.popup.actor.get_preferred_width(-1);
        const [, height] = drag.popup.actor.get_preferred_height(width);
        const {x, y} = popupPosition(px, py, width, height, workArea(monitor));
        drag.popup.actor.set_position(x, y);
    }

    _onGrabEnd(window, op) {
        if (this._resize?.window === window) {
            this._endLinkedResize();
            return;
        }
        const drag = this._drag;
        if (!drag || drag.window !== window || !isMoveOp(op))
            return;
        const target = drag.target;
        this._endDrag();
        if (!target)
            return;
        // Let Mutter finish the grab before moving the window.
        GLib.idle_add(GLib.PRIORITY_DEFAULT, () => {
            this._place(window, target);
            return GLib.SOURCE_REMOVE;
        });
    }

    _endDrag() {
        if (!this._drag)
            return;
        if (this._drag.timerId)
            GLib.source_remove(this._drag.timerId);
        this._drag.popup?.destroy();
        this._drag = null;
    }

    _place(window, target) {
        const screens = internalScreens();
        // Remember the size the user gave the window, before it is tiled.
        if (!this._tiled.has(window) && window.get_maximize_flags() === 0)
            this._freeSizes.set(window, frameRect(window));
        this._untile(window);
        if (target.kind === 'free') {
            const monitor = screenMonitor(target.screen);
            if (monitor < 0)
                return;
            const size = this._freeSizes.get(window) ?? frameRect(window);
            const rect = freeRect(size, workArea(monitor));
            if (window.get_maximize_flags() !== 0)
                window.unmaximize();
            window.move_resize_frame(true, rect.x, rect.y, rect.width, rect.height);
            return;
        }

        let rect, areas;
        if (target.kind === 'span') {
            if (screens.top < 0 || screens.bottom < 0)
                return;
            areas = [workArea(screens.top), workArea(screens.bottom)];
            rect = spanRect(target.zone, areas[0], areas[1]);
        } else {
            const monitor = screenMonitor(target.screen);
            if (monitor < 0)
                return;
            if (target.zone === 'full') {
                if (window.get_monitor() !== monitor)
                    window.move_to_monitor(monitor);
                window.maximize();
                return;
            }
            areas = [workArea(monitor)];
            rect = zoneRect(target.zone, areas[0]);
        }
        // Any maximized direction counts: a window maximized only vertically
        // (e.g. snapped to a screen half by edge tiling) is otherwise held at
        // the screen's height by Mutter's maximization constraint.
        if (window.get_maximize_flags() !== 0)
            window.unmaximize();
        // Tiled state first, so the app gets the new size together with it
        // (GTK keeps a tiled window's configured size).
        this._tile(window, rect, areas);
        window.move_resize_frame(true, rect.x, rect.y, rect.width, rect.height);
        this._ensureSize(window, rect);
    }

    /**
     * Marks the window's edges as constrained (Ubuntu's Mutter patch, also used
     * by Ubuntu's Tiling Assistant). The app then treats its size as tiled and
     * keeps it; GTK 4 apps otherwise shrink back to the height of one screen.
     */
    _tile(window, rect, areas) {
        if (!window.override_constraints)
            return;
        const edges = edgeConstraints(rect, areas);
        const constraint = edge => edge === 'monitor'
            ? Meta.WindowConstraint.MONITOR
            : Meta.WindowConstraint.WINDOW;
        window.override_constraints(constraint(edges.top), constraint(edges.left),
            constraint(edges.right), constraint(edges.bottom));
        if (!this._tiled.has(window)) {
            this._tiled.set(window,
                window.connect('unmanaged', () => this._tiled?.delete(window)));
        }
    }

    /** Clears the edge constraints of a window this extension tiled. */
    _untile(window) {
        const unmanagedId = this._tiled.get(window);
        if (unmanagedId === undefined)
            return;
        window.disconnect(unmanagedId);
        this._tiled.delete(window);
        const none = Meta.WindowConstraint.NONE;
        window.override_constraints(none, none, none, none);
    }

    /** If the app shrank back after placing, asks once more for the size. */
    _ensureSize(window, rect) {
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 700, () => {
            const r = window.get_frame_rect();
            if (this._tiled?.has(window) && (r.width !== rect.width || r.height !== rect.height))
                window.move_resize_frame(true, rect.x, rect.y, rect.width, rect.height);
            return GLib.SOURCE_REMOVE;
        });
    }

    /** Starts linked resizing when a window this extension tiled is resized. */
    _startLinkedResize(window) {
        const workspace = window.get_workspace();
        const monitor = window.get_monitor();
        // Same screen only: the other screen's dividers may line up by chance.
        const neighbours = [...this._tiled.keys()]
            .filter(w => w !== window && !w.minimized && w.get_workspace() === workspace &&
                w.get_monitor() === monitor)
            .map(w => ({id: w, rect: frameRect(w)}));
        if (neighbours.length === 0)
            return;
        const start = frameRect(window);
        const sizeChangedId = window.connect('size-changed', () => {
            for (const {id: neighbour, rect} of linkedResize(start, frameRect(window), neighbours))
                neighbour.move_resize_frame(true, rect.x, rect.y, rect.width, rect.height);
        });
        this._resize = {window, sizeChangedId};
    }

    _endLinkedResize() {
        if (!this._resize)
            return;
        this._resize.window.disconnect(this._resize.sizeChangedId);
        this._resize = null;
    }

    /**
     * Touch keyboard: GNOME shows its on-screen keyboard automatically only in
     * "touch mode", which requires that no physical keyboard is present, and
     * the Duo's keyboard always is (docked or over Bluetooth). Treating touch
     * mode as on makes GNOME's own rule apply: the keyboard is enabled while
     * the last input came from the touchscreen, so tapping a text field with a
     * finger opens it (as does swiping up from the bottom edge) while clicking
     * it with the touchpad or mouse does not.
     */
    _syncTouchKeyboard() {
        if (this._settings.get_boolean('touch-keyboard'))
            this._patchTouchKeyboard();
        else
            this._restoreTouchKeyboard();
    }

    _patchTouchKeyboard() {
        const manager = Main.keyboard;
        if (this._touchKeyboardPatched || !manager ||
            typeof manager._syncEnabled !== 'function' ||
            typeof manager._lastDeviceIsTouchscreen !== 'function')
            return; // GNOME's internals changed: leave the keyboard alone.
        const original = manager._syncEnabled;
        const extension = this;
        // With the keyboard attached the on-screen keyboard is only for touch
        // screens elsewhere (an external touch monitor).
        const touchMode = {
            get_touch_mode: () => !extension._keyboardAttached || extension._touchedExternal(),
        };
        manager._syncEnabled = function () {
            const seat = this._seat;
            this._seat = touchMode;
            try {
                original.call(this);
            } finally {
                this._seat = seat;
            }
            extension._defaultKeyboardScreen();
        };
        this._touchKeyboardPatched = true;
        manager._syncEnabled();
    }

    /**
     * Remembers the screen of each new touch: the on-screen keyboard opens on
     * an external touch screen that was touched, also with the keyboard
     * attached.
     */
    _onCapturedEvent(event) {
        this._trackFingerHold(event);
        if (event.type() !== Clutter.EventType.TOUCH_BEGIN)
            return Clutter.EVENT_PROPAGATE;
        const [x, y] = event.get_coords();
        const monitor = monitorAt(Main.layoutManager.monitors, x, y);
        if (monitor !== this._lastTouchMonitor) {
            const wasExternal = this._touchedExternal();
            this._lastTouchMonitor = monitor;
            if (wasExternal !== this._touchedExternal() && this._touchKeyboardPatched)
                Main.keyboard._syncEnabled();
        }
        return Clutter.EVENT_PROPAGATE;
    }

    _touchedExternal() {
        return isExternal(internalScreens(), this._lastTouchMonitor);
    }

    /** Follows whether the keyboard is attached, from the daemon's state file. */
    _watchDaemonState() {
        const file = Gio.File.new_for_path(DAEMON_STATE);
        this._stateMonitor = file.monitor_file(Gio.FileMonitorFlags.WATCH_MOVES, null);
        this._stateMonitor.connect('changed', () => {
            // Several events per save: read once they have settled.
            if (this._stateReadId)
                GLib.source_remove(this._stateReadId);
            this._stateReadId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 200, () => {
                this._stateReadId = 0;
                this._readDaemonState(file);
                return GLib.SOURCE_REMOVE;
            });
        });
        this._readDaemonState(file);
    }

    _readDaemonState(file) {
        file.load_contents_async(null, (source, result) => {
            let attached;
            try {
                const [, bytes] = source.load_contents_finish(result);
                attached = JSON.parse(new TextDecoder().decode(bytes))?.status?.keyboardAttached === true;
            } catch {
                return; // No daemon, or caught mid-write: keep the last state.
            }
            if (attached === this._keyboardAttached || !this._settings)
                return;
            this._keyboardAttached = attached;
            if (this._touchKeyboardPatched)
                Main.keyboard._syncEnabled();
            this._checkCovered();
        });
    }

    _restoreTouchKeyboard() {
        if (!this._touchKeyboardPatched)
            return;
        const manager = Main.keyboard;
        // The wrapper is an own property; removing it uses the class's method again.
        delete manager._syncEnabled;
        this._touchKeyboardPatched = false;
        manager._syncEnabled();
    }

    /**
     * With both screens on, shows the on-screen keyboard on the screen without
     * the text field, so it never covers it. GNOME tells the keyboard where the
     * text cursor is through setCursorLocation(); that is where the field's
     * screen becomes known.
     */
    _patchKeyboardScreen() {
        const proto = KeyboardUI.Keyboard?.prototype;
        if (this._originalSetCursorLocation || typeof proto?.setCursorLocation !== 'function')
            return; // GNOME's internals changed: leave the keyboard alone.
        const original = proto.setCursorLocation;
        this._originalSetCursorLocation = original;
        const settings = this._settings;
        const extension = this;
        proto.setCursorLocation = function (window, x, y, w, h) {
            if (window && settings.get_boolean('keyboard-on-other-screen')) {
                const screens = internalScreens();
                const field = monitorAt(Main.layoutManager.monitors, x + w / 2, y + h / 2);
                const target = keyboardScreenForTouch(screens, extension._lastTouchMonitor,
                    field, y + h, keyboardTopOnBottomScreen(screens));
                if (target >= 0) {
                    if (Main.layoutManager.keyboardIndex !== target)
                        Main.layoutManager.keyboardIndex = target;
                    // The field is on the other screen: nothing to slide out of the way.
                    if (target !== field)
                        return original.call(this, null);
                }
            }
            return original.call(this, window, x, y, w, h);
        };
    }

    /**
     * Makes the bottom screen the keyboard's screen while both are on, also
     * for apps that do not report where their text cursor is (setCursorLocation
     * is never called for them). Not while it is shown, so a keyboard moved to
     * the top screen for a covered field stays there.
     */
    _defaultKeyboardScreen() {
        const screens = internalScreens();
        if (!this._settings.get_boolean('keyboard-on-other-screen') || Main.keyboard?.visible)
            return;
        // After a touch on an external screen, the keyboard goes there.
        const target = this._touchedExternal() ? this._lastTouchMonitor : screens.bottom;
        if (target < 0 || (screens.top < 0 && target === screens.bottom))
            return;
        if (Main.layoutManager.keyboardIndex !== target)
            Main.layoutManager.keyboardIndex = target;
    }

    /**
     * GNOME's swipe-up gesture for the keyboard counts the bottom edge of every
     * screen, so with stacked screens the seam triggers it too. While both
     * panels are on, only swipes starting on the bottom panel count; a swipe
     * on an external touch screen opens the keyboard there (swipeScreen).
     */
    _restrictKeyboardSwipe() {
        const gesture = Main.keyboard?._bottomDragGesture;
        if (!gesture)
            return;
        this._swipeGesture = gesture;
        this._swipeId = gesture.connect('may-recognize', () => {
            // The gesture's own begin coordinates are garbage when this signal
            // is emitted; the finger has barely moved yet, so its current
            // position tells the screen just as well.
            const [x, y] = dragPosition();
            const target = swipeScreen(internalScreens(), monitorAt(Main.layoutManager.monitors, x, y));
            if (target < 0)
                return false;
            // GNOME opens the keyboard on its current screen: make that this one.
            if (!Main.keyboard?.visible && Main.layoutManager.keyboardIndex !== target)
                Main.layoutManager.keyboardIndex = target;
            return true;
        });
    }

    _releaseKeyboardSwipe() {
        if (this._swipeId)
            this._swipeGesture.disconnect(this._swipeId);
        this._swipeId = 0;
        this._swipeGesture = null;
    }

    _restoreKeyboardScreen() {
        if (!this._originalSetCursorLocation)
            return;
        KeyboardUI.Keyboard.prototype.setCursorLocation = this._originalSetCursorLocation;
        this._originalSetCursorLocation = null;
    }

    /** D-Bus method: the microphone volume pop-up, on every screen. */
    ShowMicVolume(level, muted) {
        const icon = Gio.ThemedIcon.new(micVolumeIcon(level, muted));
        Main.osdWindowManager.showAll(icon, null, muted ? 0 : level, 1);
    }

    /** D-Bus method: a pop-up with an icon, a label and a level bar (level < 0: none). */
    ShowOsd(icon, label, level) {
        Main.osdWindowManager.showAll(Gio.ThemedIcon.new(icon), label, level >= 0 ? level : null, 1);
    }

    /**
     * D-Bus method: a warning bubble that stays until OK is pressed. A newer
     * alert with the same id (e.g. the battery dropping further) replaces it.
     */
    ShowAlert(id, icon, title, message) {
        this._showAlert(id, icon, title, message);
    }

    _showAlert(id, icon, title, message, buttons) {
        this._alerts.get(id)?.dismiss();
        const alert = new AlertBubble(id, icon, title, message, closed => {
            if (this._alerts?.get(closed.alertId) === closed)
                this._alerts.delete(closed.alertId);
        }, buttons);
        this._alerts.set(id, alert);
    }

    /**
     * The bottom screen is on while the keyboard lies on it (turned on in
     * GNOME Settings, say): windows can end up where nobody sees them. Asks
     * once per attach whether to turn it off; turning it off brings its
     * windows back to the other screens. Checked a few seconds after a
     * change, since attaching turns the screen off by itself shortly after.
     */
    _checkCovered() {
        if (this._coveredTimeoutId)
            GLib.source_remove(this._coveredTimeoutId);
        if (!this._keyboardAttached)
            this._coveredKept = false;
        this._coveredTimeoutId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 3, () => {
            this._coveredTimeoutId = 0;
            if (!this._alerts)
                return GLib.SOURCE_REMOVE;
            const bottomOn = global.backend.get_monitor_manager().get_monitor_for_connector(BOTTOM_CONNECTOR) >= 0;
            const covered = this._keyboardAttached && bottomOn;
            if (!covered) {
                this._alerts.get('bottom-covered')?.dismiss();
            } else if (!this._coveredKept && !this._alerts.has('bottom-covered')) {
                this._showAlert('bottom-covered', 'video-display-symbolic',
                    'The bottom screen is on under the keyboard',
                    'Windows there are hidden by the keyboard. Turning it off moves them to the top screen.', [
                        {label: 'Turn it off', action: () => this._turnOffBottom()},
                        {label: 'Keep it on', action: () => (this._coveredKept = true)},
                    ]);
            }
            return GLib.SOURCE_REMOVE;
        });
    }

    /**
     * Super+P: detached, the keyboard's F7. With Shift held the keyboard
     * lets go of Shift before sending Super+P, so GNOME can't tell Shift+F7
     * apart; the Zenbook Duo daemon recognises it and calls ShiftF7Pressed.
     * Wait briefly for that before opening the display controls.
     */
    _onSuperP() {
        if (this._shiftF7TimerId) {
            // The daemon's word came first: this Super+P is Shift+F7.
            this._clearTimer('_shiftF7TimerId');
            this._askMainDisplay();
            return;
        }
        this._clearTimer('_superPTimerId');
        this._superPTimerId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SHIFT_F7_WINDOW_MS, () => {
            this._superPTimerId = 0;
            this._openDisplayDialog();
            return GLib.SOURCE_REMOVE;
        });
    }

    _clearTimer(name) {
        if (this[name])
            GLib.source_remove(this[name]);
        this[name] = 0;
    }

    /**
     * D-Bus method: the Control Panel sets the top panel's brightness through
     * GNOME's own slider logic, so GNOME's slider shows it. False when GNOME
     * has no brightness slider (the caller uses logind then).
     */
    SetTopBrightness(percent) {
        return setTopThroughGnome(percent);
    }

    /**
     * D-Bus method: the daemon saw the detached keyboard's Shift+F7. Its
     * Super+P may come before or after this (or not at all with Fn lock on,
     * which turns it into F7); either way it shows the main-display switch once.
     */
    ShiftF7Pressed() {
        if (this._superPTimerId) {
            this._clearTimer('_superPTimerId');
            this._askMainDisplay();
            return;
        }
        this._clearTimer('_shiftF7TimerId');
        this._shiftF7TimerId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SHIFT_F7_WINDOW_MS, () => {
            this._shiftF7TimerId = 0;
            this._askMainDisplay();
            return GLib.SOURCE_REMOVE;
        });
    }

    /**
     * Five fingers held still on one screen for a moment: the main-display
     * switch (as Shift+F7). Touches still reach the apps underneath.
     */
    _trackFingerHold(event) {
        const type = event.type();
        const touch = [Clutter.EventType.TOUCH_BEGIN, Clutter.EventType.TOUCH_UPDATE,
            Clutter.EventType.TOUCH_END, Clutter.EventType.TOUCH_CANCEL];
        if (!touch.includes(type) || !this._settings.get_boolean('five-finger-main-display'))
            return;
        const slot = event.get_event_sequence()?.get_slot?.();
        if (slot === undefined || slot === null)
            return;
        this._fingerHold ??= new FingerHold();
        const [x, y] = event.get_coords();
        let result;
        if (type === Clutter.EventType.TOUCH_BEGIN)
            result = this._fingerHold.begin(slot, x, y, monitorAt(Main.layoutManager.monitors, x, y));
        else if (type === Clutter.EventType.TOUCH_UPDATE)
            result = this._fingerHold.move(slot, x, y);
        else
            result = this._fingerHold.end(slot);
        if (result === 'armed') {
            this._clearFingerHoldTimer();
            this._fingerHoldId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 600, () => {
                this._fingerHoldId = 0;
                if (this._fingerHold?.armed)
                    this._askMainDisplay();
                return GLib.SOURCE_REMOVE;
            });
        } else if (result === 'cancelled') {
            this._clearFingerHoldTimer();
        }
    }

    _clearFingerHoldTimer() {
        if (this._fingerHoldId)
            GLib.source_remove(this._fingerHoldId);
        this._fingerHoldId = 0;
    }

    /**
     * Shift+F7: a bubble on every screen; pressing "Make this the main
     * display" on one makes that screen the main display (top bar, new
     * windows, notifications). On the current main display the button says so.
     */
    _askMainDisplay() {
        // Pressed again while the bubble is up: cancel it.
        if (this._alerts.has('main-display')) {
            this._alerts.get('main-display').dismiss();
            return;
        }
        if (Main.layoutManager.monitors.length < 2) {
            this.ShowOsd('video-display-symbolic', 'Only one screen is on', -1);
            return;
        }
        const primary = Main.layoutManager.primaryIndex;
        this._showAlert('main-display', 'video-display-symbolic', 'Main display switch',
            'Press the button on the screen that should be the main display.', [
                {
                    label: index => (index === primary ? 'This is the main display' : 'Make this the main display'),
                    sensitive: index => index !== primary,
                    action: index => this._makeMainDisplay(index),
                },
                {label: 'Cancel'},
            ]);
    }

    /** Makes the screen GNOME shows as monitor `index` the main display. */
    _makeMainDisplay(index) {
        const manager = global.backend.get_monitor_manager();
        changeDisplays(screens => {
            const target = screens.find(s => s.on && manager.get_monitor_for_connector(s.connector) === index);
            if (!target)
                return;
            for (const s of screens)
                s.primary = false;
            // A mirror shares its source's place: that one carries the flag.
            (screens.find(s => s.connector === target.mirrorOf) ?? target).primary = true;
        }).catch(error => log(`Zenbook Duo Snap: changing the main display failed: ${error.message}`));
    }

    _turnOffBottom() {
        changeDisplays(screens => {
            const bottom = screens.find(s => s.panel === 'bottom');
            if (bottom)
                bottom.on = false;
        }).catch(error => log(`Zenbook Duo Snap: turning the bottom screen off failed: ${error.message}`));
    }

    /** D-Bus method: an ASUS app key was pressed; says so when nothing uses it. */
    AppKeyPressed(key, accelerator) {
        if (acceleratorAssigned(accelerator, this._settings))
            return;
        this.ShowOsd('preferences-desktop-keyboard-shortcuts-symbolic',
            `${key} has no shortcut · Settings › Keyboard`, -1);
    }

    /**
     * F8: swap the windows of two screens. With exactly two screens on (both
     * panels, or a panel and an external screen) those two; with more, a
     * dialog picks them, starting from the last pair used.
     */
    _swapWindows() {
        const named = namedScreens(Main.layoutManager.monitors, internalScreens());
        if (named.length < 2) {
            this.ShowOsd('video-display-symbolic', 'Only one screen is on', -1);
            return;
        }
        if (named.length === 2) {
            this._swapBetween(named[0].index, named[1].index);
            return;
        }
        this._openPickDialog({
            named,
            source: -1,
            picked: defaultSwapPair(named, this._lastSwapNames),
            onDone: (a, b) => {
                this._lastSwapNames = [a, b].map(i => named.find(s => s.index === i).name);
                this._swapBetween(a, b);
            },
        });
    }

    _swapBetween(a, b) {
        const workspace = global.workspace_manager.get_active_workspace();
        // Read every window's screen before moving any of them.
        const moves = workspace.list_windows()
            .filter(w => isNormalWindow(w) && !w.minimized && !w.is_skip_taskbar())
            .map(w => [w, w.get_monitor()]);
        for (const [window, monitor] of moves) {
            if (monitor === a)
                window.move_to_monitor(b);
            else if (monitor === b)
                window.move_to_monitor(a);
        }
    }

    /**
     * Shift+F8: move the focused window to the other screen; with more than
     * two screens on, a dialog picks the target.
     */
    _moveFocusedWindow() {
        const window = global.display.get_focus_window();
        if (!isNormalWindow(window))
            return;
        const named = namedScreens(Main.layoutManager.monitors, internalScreens());
        const source = window.get_monitor();
        const others = named.filter(s => s.index !== source);
        if (others.length === 0)
            return;
        if (others.length === 1 && named.length === 2) {
            window.move_to_monitor(others[0].index);
            return;
        }
        this._openPickDialog({
            named,
            source,
            picked: [],
            onDone: (_from, to) => {
                // The window may have closed while the dialog was open.
                if (window.get_compositor_private())
                    window.move_to_monitor(to);
            },
        });
    }

    /**
     * Super+P (GNOME's 'switch-monitor' binding, also XF86Display) opens the
     * display controls instead of GNOME's switcher: detached, F7 sends it.
     */
    _syncSuperP() {
        if (this._settings.get_boolean('display-controls-on-super-p')) {
            Main.wm.setCustomKeybindingHandler('switch-monitor',
                Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW,
                () => this._onSuperP());
            this._superPTaken = true;
        } else {
            this._restoreSuperP();
        }
    }

    _restoreSuperP() {
        if (!this._superPTaken)
            return;
        // GNOME's own handler, as set in windowManager.js.
        Main.wm.setCustomKeybindingHandler('switch-monitor',
            Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW,
            Main.wm._startSwitcher.bind(Main.wm));
        this._superPTaken = false;
    }

    /** F7 opens the display controls; pressed again it closes them. */
    _openDisplayDialog() {
        if (this._displayDialog) {
            this._displayDialog.close();
            return;
        }
        const dialog = new DisplayDialog(() => this._keyboardAttached);
        dialog.connect('destroy', () => {
            if (this._displayDialog === dialog)
                this._displayDialog = null;
        });
        this._displayDialog = dialog;
        dialog.open();
    }

    _openPickDialog(params) {
        this._pickDialog?.close();
        const dialog = new ScreenPickDialog({monitors: Main.layoutManager.monitors, ...params});
        dialog.connect('destroy', () => {
            if (this._pickDialog === dialog)
                this._pickDialog = null;
        });
        this._pickDialog = dialog;
        dialog.open();
    }
}
