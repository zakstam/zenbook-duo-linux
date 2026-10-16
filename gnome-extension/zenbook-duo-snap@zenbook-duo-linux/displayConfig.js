// Reading and applying the screen setup through Mutter's DisplayConfig D-Bus
// interface (what GNOME Settings › Displays uses), and saving the Duo panels'
// scale through the Zenbook Duo daemon so attach/detach keeps it.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const BUS_NAME = 'org.gnome.Mutter.DisplayConfig';
const OBJECT_PATH = '/org/gnome/Mutter/DisplayConfig';
const INTERFACE = 'org.gnome.Mutter.DisplayConfig';
// ApplyMonitorsConfig methods.
const PERSISTENT = 2;

const DAEMON_SOCKET = '/var/lib/zenbook-duo/daemon.sock';
const DUO_SETTINGS = GLib.build_filenamev([GLib.get_user_config_dir(), 'zenbook-duo', 'settings.json']);

function call(method, parameters, replyType) {
    return new Promise((resolve, reject) => {
        Gio.DBus.session.call(BUS_NAME, OBJECT_PATH, INTERFACE, method, parameters,
            replyType ? new GLib.VariantType(replyType) : null,
            Gio.DBusCallFlags.NONE, -1, null, (connection, result) => {
                try {
                    resolve(connection.call_finish(result));
                } catch (error) {
                    reject(error);
                }
            });
    });
}

/**
 * The connected screens: connector, name, built-in, all modes, the mode in
 * use (or the preferred one for a screen that is off) with its supported
 * scales, and, when on, position, scale, rotation, primary, and the other
 * screens it shares its place with (mirroring).
 */
export async function readDisplays() {
    const reply = await call('GetCurrentState', null,
        '(ua((ssss)a(siiddada{sv})a{sv})a(iiduba(ssss)a{sv})a{sv})');
    const [serial, monitors, logicalMonitors] = reply.recursiveUnpack();
    const logicalOf = new Map();
    for (const [x, y, scale, transform, primary, specs] of logicalMonitors) {
        const group = specs.map(([connector]) => connector);
        for (const connector of group)
            logicalOf.set(connector, {x, y, scale, transform, primary, group});
    }
    const screens = monitors.map(([[connector], modes, props]) => {
        const mode = modes.find(m => m[6]['is-current']) ??
            modes.find(m => m[6]['is-preferred']) ?? modes[0];
        const preferred = modes.find(m => m[6]['is-preferred']) ?? modes[0];
        const logical = logicalOf.get(connector);
        return {
            connector,
            name: props['display-name'] ?? connector,
            builtin: props['is-builtin'] === true,
            modes: modes.map(([id, width, height, refresh, preferredScale, supportedScales, modeProps]) => ({
                id, width, height, refresh, preferredScale, supportedScales,
                variable: modeProps['refresh-rate-mode'] === 'variable',
            })),
            preferredModeId: preferred[0],
            modeId: mode[0],
            modeWidth: mode[1],
            modeHeight: mode[2],
            preferredScale: mode[4],
            supportedScales: mode[5],
            on: Boolean(logical),
            x: logical?.x ?? 0,
            y: logical?.y ?? 0,
            scale: logical?.scale ?? mode[4],
            transform: logical?.transform ?? 0,
            primary: logical?.primary ?? false,
            sharedWith: logical?.group.filter(c => c !== connector) ?? [],
        };
    });
    return {serial, screens};
}

/**
 * Applies the screens that are on, each with its x, y, scale, transform and
 * primary flag (exactly one should be primary). A screen with `mirrorOf`
 * joins that screen's place (same picture; its mode must be the same size).
 */
export async function applyDisplays(serial, screens) {
    const on = screens.filter(s => s.on);
    const logical = on.filter(s => !s.mirrorOf).map(s => [
        s.x, s.y, s.scale, s.transform, s.primary,
        [s, ...on.filter(m => m.mirrorOf === s.connector)].map(m => [m.connector, m.modeId, {}]),
    ]);
    await call('ApplyMonitorsConfig',
        new GLib.Variant('(uua(iiduba(ssa{sv}))a{sv})', [serial, PERSISTENT, logical, {}]),
        null);
}

/** One request to the Zenbook Duo daemon (a JSON line over its socket). */
function daemonRequest(payload) {
    return new Promise((resolve, reject) => {
        const client = new Gio.SocketClient();
        client.connect_async(Gio.UnixSocketAddress.new(DAEMON_SOCKET), null, (source, result) => {
            try {
                const connection = source.connect_finish(result);
                const line = `${JSON.stringify({protocol_version: 1, payload})}\n`;
                connection.get_output_stream().write_all(new TextEncoder().encode(line), null);
                const input = new Gio.DataInputStream({base_stream: connection.get_input_stream()});
                input.read_line_async(GLib.PRIORITY_DEFAULT, null, (stream, res) => {
                    try {
                        const [bytes] = stream.read_line_finish_utf8(res);
                        connection.close(null);
                        resolve(JSON.parse(bytes ?? 'null')?.payload);
                    } catch (error) {
                        reject(error);
                    }
                });
            } catch (error) {
                reject(error);
            }
        });
    });
}

const BACKLIGHT_ROOT = '/sys/class/backlight';

/** A Duo panel's backlight directory ('top' or 'bottom'), like the daemon finds it. */
function backlightDir(panel) {
    const names = [];
    try {
        const enumerator = Gio.File.new_for_path(BACKLIGHT_ROOT)
            .enumerate_children('standard::name', Gio.FileQueryInfoFlags.NONE, null);
        for (let info; (info = enumerator.next_file(null));)
            names.push(info.get_name());
    } catch {
        return null;
    }
    const lower = name => name.toLowerCase();
    const bottom = name => lower(name).includes('edp-2') || lower(name).includes('edp2');
    const name = panel === 'bottom'
        ? names.find(bottom)
        : names.find(n => n === 'intel_backlight') ?? names.find(n => lower(n).includes('edp-1')) ??
          names.find(n => !bottom(n));
    return name ? `${BACKLIGHT_ROOT}/${name}` : null;
}

function readNumber(path) {
    try {
        const [, bytes] = Gio.File.new_for_path(path).load_contents(null);
        return parseInt(new TextDecoder().decode(bytes), 10);
    } catch {
        return NaN;
    }
}

/** A Duo panel's brightness in percent, or null without its own backlight. */
export function readPanelBrightness(panel) {
    const dir = backlightDir(panel);
    if (!dir)
        return null;
    const level = readNumber(`${dir}/brightness`);
    const max = readNumber(`${dir}/max_brightness`);
    return max > 0 && level >= 0 ? Math.round(level / max * 100) : null;
}

/**
 * The user's graphical session in logind. GNOME Shell runs as a service
 * outside it, where "session/auto" is unknown.
 */
function sessionPath() {
    try {
        const reply = Gio.DBus.system.call_sync('org.freedesktop.login1', '/org/freedesktop/login1/user/self',
            'org.freedesktop.DBus.Properties', 'Get',
            new GLib.Variant('(ss)', ['org.freedesktop.login1.User', 'Display']),
            new GLib.VariantType('(v)'), Gio.DBusCallFlags.NONE, -1, null);
        const [id, path] = reply.recursiveUnpack()[0];
        if (id)
            return path;
    } catch {
        // Fall back to logind's own guess.
    }
    return '/org/freedesktop/login1/session/auto';
}

/** Sets one backlight through logind, as GNOME's own brightness slider does. */
function setBacklightLevel(name, level) {
    return new Promise((resolve, reject) => {
        Gio.DBus.system.call('org.freedesktop.login1', sessionPath(),
            'org.freedesktop.login1.Session', 'SetBrightness',
            new GLib.Variant('(ssu)', ['backlight', name, level]), null,
            Gio.DBusCallFlags.NONE, -1, null, (connection, result) => {
                try {
                    connection.call_finish(result);
                    resolve();
                } catch (error) {
                    reject(error);
                }
            });
    });
}

/**
 * Sets the top panel through GNOME's own brightness manager, so GNOME's
 * slider shows the level. GNOME keeps a brightness per panel, but with Mutter
 * 50 both drive the top panel's backlight (it picks intel_backlight for eDP-2
 * as well, though sysfs links card1-eDP-2-backlight to eDP-2); when they
 * differ they fight over it. So every panel's GNOME value gets the same
 * level: the others quietly, the last through its setter, which makes GNOME
 * apply them once (as dragging its slider does, since that keeps them equal).
 * False when GNOME has no brightness manager.
 */
export function setTopThroughGnome(percent) {
    const scales = Main.brightnessManager?.scales;
    if (!Array.isArray(scales) || scales.length === 0 ||
        !scales.every(s => typeof s.value === 'number' && '_value' in s))
        return false;
    const value = Math.min(1, Math.max(0, percent / 100));
    scales.slice(0, -1).forEach(s => {
        s._value = value;
    });
    scales[scales.length - 1].value = value;
    return true;
}

/** Sets a Duo panel's brightness ('top', 'bottom' or 'both'), never fully dark. */
export async function setPanelBrightness(panel, percent) {
    for (const which of panel === 'both' ? ['top', 'bottom'] : [panel]) {
        const dir = backlightDir(which);
        if (!dir)
            continue;
        const max = readNumber(`${dir}/max_brightness`);
        const level = Math.max(1, Math.round(max * Math.min(100, percent) / 100));
        // The top panel through GNOME's own slider logic when there is one;
        // otherwise, and for the bottom panel, through logind on the panel's
        // own backlight (Mutter 50 drives the top panel's backlight for eDP-2).
        if (which === 'top' && setTopThroughGnome(percent))
            continue;
        await setBacklightLevel(GLib.path_get_basename(dir), level);
    }
}

/**
 * Turns "same brightness for both Duo screens" on or off, in the same two
 * places the Control Panel saves it (settings file and daemon).
 */
export async function setSameBrightness(on) {
    try {
        const file = Gio.File.new_for_path(DUO_SETTINGS);
        const [, bytes] = file.load_contents(null);
        const local = JSON.parse(new TextDecoder().decode(bytes));
        local.syncBrightness = on;
        file.replace_contents(new TextEncoder().encode(JSON.stringify(local, null, 2)),
            null, false, Gio.FileCreateFlags.NONE, null);
    } catch {
        // No settings file (Control Panel never set up): the daemon is enough.
    }
    try {
        const reply = await daemonRequest({type: 'get_settings'});
        if (reply?.type === 'settings')
            await daemonRequest({type: 'save_settings', settings: {...reply.settings, syncBrightness: on}});
    } catch {
        // Daemon not running.
    }
}

/** Whether both Duo panels keep the same brightness (Control Panel setting). */
export function sameBrightness() {
    try {
        const [, bytes] = Gio.File.new_for_path(DUO_SETTINGS).load_contents(null);
        return JSON.parse(new TextDecoder().decode(bytes)).syncBrightness !== false;
    } catch {
        return true;
    }
}

/** The Duo panels' saved scale (Control Panel settings), if there is one. */
export function savedPanelScale() {
    try {
        const [, bytes] = Gio.File.new_for_path(DUO_SETTINGS).load_contents(null);
        const scale = JSON.parse(new TextDecoder().decode(bytes)).defaultScale;
        return typeof scale === 'number' && scale > 0 ? scale : null;
    } catch {
        return null;
    }
}

/**
 * Saves the Duo panels' scale as the default the daemon applies on attach
 * and detach, in the daemon and in the Control Panel's settings file (the
 * same two places the Control Panel's Save writes). Best effort: without the
 * Zenbook Duo services the scale still applies until the next attach/detach.
 */
export async function savePanelScale(scale) {
    try {
        const file = Gio.File.new_for_path(DUO_SETTINGS);
        const [, bytes] = file.load_contents(null);
        const local = JSON.parse(new TextDecoder().decode(bytes));
        local.defaultScale = scale;
        file.replace_contents(new TextEncoder().encode(JSON.stringify(local, null, 2)),
            null, false, Gio.FileCreateFlags.NONE, null);
    } catch {
        // No settings file (Control Panel never set up): the daemon is enough.
    }
    try {
        const reply = await daemonRequest({type: 'get_settings'});
        if (reply?.type !== 'settings')
            return;
        await daemonRequest({type: 'save_settings', settings: {...reply.settings, defaultScale: scale}});
    } catch {
        // Daemon not running.
    }
}
