// The display controls (F7, the projection key; detached the keyboard sends
// Super+P): a compact version of Settings › Displays for the Duo. Pick what
// is on (join / Duo only / external only), and per screen: on/off, rotation
// and position (external screens), and scale. Changes apply right away.
// An external screen's position is a side of another screen (the Duo, one
// panel, or another external screen) and an offset along that edge; choices
// that would overlap another screen are greyed out.

import Clutter from 'gi://Clutter';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as Dialog from 'resource:///org/gnome/shell/ui/dialog.js';
import * as ModalDialog from 'resource:///org/gnome/shell/ui/modalDialog.js';

import {EDGE_OFFSETS, OFFSETS, currentPlacements, layoutScreens, logicalSize, mirrorMode, mirrorSources, offeredScales, placedAgainst, screenMap} from './zones.js';
import {applyDisplays, readDisplays, readPanelBrightness, sameBrightness, savePanelScale, savedPanelScale, setPanelBrightness, setSameBrightness} from './displayConfig.js';
import * as Slider from 'resource:///org/gnome/shell/ui/slider.js';

const MAP_WIDTH = 380;
const MAP_HEIGHT = 190;
const TOP_CONNECTOR = 'eDP-1';
const BOTTOM_CONNECTOR = 'eDP-2';

// Mutter transforms with GNOME Settings' names.
const ROTATIONS = [
    {transform: 0, label: 'Landscape'},
    {transform: 1, label: 'Portrait Right'},
    {transform: 3, label: 'Portrait Left'},
    {transform: 2, label: 'Flipped'},
];
const SIDE_LABELS = {above: 'Above', left: 'Left', right: 'Right', below: 'Below'};
const ANCHOR_LABELS = {duo: 'Duo', top: 'Top screen', bottom: 'Bottom screen'};
// Offset chips: arrows for the direction, 100 = edges lined up.
// Offset chips: arrows for the direction, 100 = as far as GNOME allows;
// the edge chips line the two screens' edges up.
const OFFSET_LABELS = {
    vertical: {'-100': '▲100', '-50': '▲50', '0': '0', '50': '▼50', '100': '▼100',
        start: 'Top edges', end: 'Bottom edges'},
    horizontal: {'-100': '◀100', '-50': '◀50', '0': '0', '50': '▶50', '100': '▶100',
        start: 'Left edges', end: 'Right edges'},
};

// The placement last picked per screen, so the dialog shows that choice
// again (one place can often be described several ways).
const remembered = new Map();
// The mirror picked last per screen, for the direction between two external screens.
const rememberedMirrors = new Map();

// The tabs, and the one last open (kept while GNOME runs).
const TABS = [
    {id: 'projection', label: 'Projection'},
    {id: 'position', label: 'Position'},
    {id: 'scale', label: 'Screen'},
];
let lastTab = 'projection';

function panelOf(connector) {
    if (connector === TOP_CONNECTOR)
        return 'top';
    if (connector === BOTTOM_CONNECTOR)
        return 'bottom';
    return null;
}

/** The screens with what the layout needs: panel role, name, and placement. */
function describe(screens) {
    const described = screens.map(s => ({...s, panel: panelOf(s.connector)}));
    const sources = mirrorSources(described, rememberedMirrors);
    for (const s of described)
        s.mirrorOf = sources.get(s.connector) ?? null;
    // Mirrors sit where their source is: only the others have placements.
    const placements = currentPlacements(described.filter(s => !s.mirrorOf), remembered);
    const names = new Map();
    for (const s of described) {
        if (s.panel)
            s.label = s.panel === 'top' ? 'Top screen' : 'Bottom screen';
        else
            s.label = s.name;
        names.set(s.label, (names.get(s.label) ?? 0) + 1);
        s.placement = placements.get(s.connector) ?? null;
    }
    // Two monitors of the same model: tell them apart by connector.
    for (const s of described) {
        if (names.get(s.label) > 1)
            s.label = `${s.label} (${s.connector})`;
    }
    return described;
}

function chip(label, checked, onClick, sensitive = true) {
    const button = new St.Button({
        style_class: 'duo-snap-chip',
        label,
        can_focus: sensitive,
        reactive: sensitive,
    });
    if (checked)
        button.add_style_pseudo_class('checked');
    if (!sensitive)
        button.add_style_pseudo_class('insensitive');
    button.connect('clicked', onClick);
    return button;
}

// Chips per line; more wrap onto the next line so labels never get cut off.
const CHIPS_PER_LINE = 4;

/** A chip that only shows a state (checked, not clickable). */
function indicator(label) {
    const button = new St.Button({
        style_class: 'duo-snap-chip',
        label,
        can_focus: false,
        reactive: false,
    });
    button.add_style_pseudo_class('checked');
    return button;
}

/** A titled group of chips, at most `perLine` per line. */
function row(title, children, perLine = CHIPS_PER_LINE) {
    const box = new St.BoxLayout({
        style_class: 'duo-snap-display-row',
        orientation: Clutter.Orientation.VERTICAL,
    });
    box.add_child(new St.Label({text: title, style_class: 'duo-snap-display-row-title'}));
    for (let i = 0; i < children.length; i += perLine) {
        const line = new St.BoxLayout({style_class: 'duo-snap-chips'});
        children.slice(i, i + perLine).forEach(c => line.add_child(c));
        box.add_child(line);
    }
    return box;
}

/** Runs `screen` at mode `id` (its size too). */
function setMode(screen, id) {
    const mode = screen.modes.find(m => m.id === id);
    screen.modeId = id;
    screen.modeWidth = mode.width;
    screen.modeHeight = mode.height;
    screen.supportedScales = mode.supportedScales;
    return mode;
}

/**
 * Keeps mirroring consistent after a change (`before` → `screens`): a mirror
 * of a screen that became a mirror itself follows along to that one's
 * source; a mirror whose source is off stops mirroring; a group that is
 * new or changed gets a mode size and scale all its screens support; mirrors
 * follow their source's scale and rotation and are never the main display;
 * a screen that stopped mirroring gets its preferred mode back (a panel its
 * usual scale).
 */
function fitMirrors(before, screens) {
    const byConnector = c => screens.find(s => s.connector === c);
    for (const s of screens) {
        // Mirroring a screen that now mirrors another: mirror that one.
        let source = s.mirrorOf && byConnector(s.mirrorOf);
        if (source?.mirrorOf && source.mirrorOf !== s.connector) {
            s.mirrorOf = source.mirrorOf;
            source = byConnector(s.mirrorOf);
        }
        if (s.mirrorOf && (!s.on || !source?.on || source.mirrorOf))
            s.mirrorOf = null;
    }
    const membersOf = (list, source) => list.filter(m => m.mirrorOf === source.connector).map(m => m.connector).sort();
    for (const source of screens.filter(s => s.on && !s.mirrorOf)) {
        const mirrors = screens.filter(m => m.mirrorOf === source.connector);
        if (mirrors.length === 0)
            continue;
        const old = before.find(b => b.connector === source.connector);
        const changed = !old || membersOf(before, old).join() !== membersOf(screens, source).join();
        if (changed) {
            const fit = mirrorMode([source, ...mirrors]);
            if (!fit) {
                mirrors.forEach(m => (m.mirrorOf = null));
                continue;
            }
            for (const member of [source, ...mirrors])
                setMode(member, fit.modeIds.get(member.connector));
            source.scale = fit.scale;
        }
        for (const m of mirrors) {
            m.scale = source.scale;
            m.transform = source.transform;
            if (m.primary) {
                m.primary = false;
                source.primary = true;
            }
        }
    }
    // Stopped mirroring: back to the preferred mode.
    const mirrored = list => new Set(list.flatMap(s => (s.mirrorOf ? [s.connector, s.mirrorOf] : [])));
    const was = mirrored(before), is = mirrored(screens);
    for (const s of screens.filter(x => was.has(x.connector) && !is.has(x.connector))) {
        const mode = setMode(s, s.preferredModeId);
        const otherPanel = screens.find(o => o.panel && o !== s && o.on && !is.has(o.connector));
        s.scale = s.panel
            ? otherPanel?.scale ?? savedPanelScale() ?? mode.preferredScale
            : mode.preferredScale;
    }
}

/**
 * Applies `change` to a copy of the described screens `before`: mirrors kept
 * consistent, one main display, laid out again. False when it would turn
 * every screen off (then nothing is applied).
 */
async function applyChange(serial, before, change) {
    const screens = before.map(s => ({...s}));
    change(screens);
    fitMirrors(before, screens);
    const on = screens.filter(s => s.on && !s.mirrorOf);
    if (on.length === 0)
        return false; // Never turn every screen off.
    if (!on.some(s => s.primary)) {
        for (const s of screens)
            s.primary = false;
        (on.find(s => s.panel === 'top') ?? on.find(s => s.panel) ?? on[0]).primary = true;
    }
    for (const [connector, {x, y}] of layoutScreens(on).positions) {
        const screen = screens.find(s => s.connector === connector);
        screen.x = x;
        screen.y = y;
    }
    for (const m of screens.filter(s => s.on && s.mirrorOf)) {
        const source = screens.find(s => s.connector === m.mirrorOf);
        m.x = source.x;
        m.y = source.y;
    }
    await applyDisplays(serial, screens);
    return true;
}

/**
 * Changes the current screen setup outside the dialog, e.g. `screens =>
 * (screens.find(s => s.panel === 'bottom').on = false)`.
 */
export async function changeDisplays(change) {
    const {serial, screens} = await readDisplays();
    return applyChange(serial, describe(screens), change);
}

/** A line of explanation in place of controls. */
function note(text) {
    const label = new St.Label({text, style_class: 'duo-snap-display-note'});
    label.clutter_text.line_wrap = true;
    return label;
}

/** A titled cross of four chips: above, left and right, below. */
function crossRow(title, chips) {
    const box = new St.BoxLayout({
        style_class: 'duo-snap-display-row',
        orientation: Clutter.Orientation.VERTICAL,
    });
    box.add_child(new St.Label({text: title, style_class: 'duo-snap-display-row-title'}));
    const cross = new St.BoxLayout({
        style_class: 'duo-snap-cross',
        orientation: Clutter.Orientation.VERTICAL,
        x_align: Clutter.ActorAlign.START,
    });
    for (const line of [[chips.above], [chips.left, chips.right], [chips.below]]) {
        const lineBox = new St.BoxLayout({style_class: 'duo-snap-chips', x_align: Clutter.ActorAlign.CENTER});
        line.forEach(c => lineBox.add_child(c));
        cross.add_child(lineBox);
    }
    box.add_child(cross);
    return box;
}

/** Where `screens` end up with `change` applied: positions and the screens moved elsewhere. */
function tryLayout(screens, change) {
    const copy = screens.map(s => ({...s}));
    change?.(copy);
    return {screens: copy, ...layoutScreens(copy.filter(s => !s.mirrorOf))};
}

export const DisplayDialog = GObject.registerClass(
class DisplayDialog extends ModalDialog.ModalDialog {
    /** @param {() => boolean} keyboardAttached whether the keyboard covers the bottom panel now */
    _init(keyboardAttached = () => false) {
        super._init({styleClass: 'duo-snap-display-dialog'});
        this._keyboardAttached = keyboardAttached;
        // The title with a close button in the top-right corner.
        const header = new St.Widget({layout_manager: new Clutter.BinLayout(), x_expand: true});
        header.add_child(new Dialog.MessageDialogContent({
            title: 'Displays',
            description: 'Changes apply right away.',
            x_expand: true,
        }));
        const closeButton = new St.Button({
            style_class: 'duo-snap-close-button',
            child: new St.Icon({icon_name: 'window-close-symbolic', icon_size: 16}),
            can_focus: true,
            x_expand: true,
            x_align: Clutter.ActorAlign.END,
            y_align: Clutter.ActorAlign.START,
        });
        closeButton.connect('clicked', () => this.close());
        header.add_child(closeButton);
        this.contentLayout.add_child(header);
        this._body = new St.BoxLayout({
            style_class: 'duo-snap-display-body',
            orientation: Clutter.Orientation.VERTICAL,
        });
        this.contentLayout.add_child(this._body);
        this.setButtons([{
            label: 'Done',
            action: () => this.close(),
            default: true,
            key: Clutter.KEY_Escape,
        }]);
        this._selected = null;
        this._busy = false;

        // F7 again closes it. The dialog holds the keyboard, so GNOME's
        // shortcuts don't run meanwhile: F7 arrives here as Super+P.
        this.connect('key-press-event', (_actor, event) => {
            const symbol = event.get_key_symbol();
            const superHeld = (event.get_state() & (Clutter.ModifierType.MOD4_MASK | Clutter.ModifierType.SUPER_MASK)) !== 0;
            if (((symbol === Clutter.KEY_p || symbol === Clutter.KEY_P) && superHeld) ||
                symbol === Clutter.KEY_Launch1) {
                this.close();
                return Clutter.EVENT_STOP;
            }
            return Clutter.EVENT_PROPAGATE;
        });

        // Clicking or touching outside the dialog closes it.
        for (const signal of ['button-press-event', 'touch-event']) {
            this.connect(signal, (_actor, event) => {
                if (event.type() === Clutter.EventType.TOUCH_BEGIN ||
                    event.type() === Clutter.EventType.BUTTON_PRESS) {
                    if (!this._insideDialog(event))
                        this.close();
                }
                return Clutter.EVENT_PROPAGATE;
            });
        }

        // Follow changes made elsewhere: attach/detach, plugging a screen in.
        const monitorManager = global.backend.get_monitor_manager();
        const changedId = monitorManager.connect('monitors-changed', () => {
            if (!this._busy)
                this._refresh();
        });
        this.connect('destroy', () => {
            this._destroyed = true;
            monitorManager.disconnect(changedId);
        });
        this._refresh();
    }

    /** Whether a press landed on the dialog box itself (not the shade around it). */
    _insideDialog(event) {
        const actor = global.stage.get_event_actor(event);
        if (actor)
            return this.dialogLayout.contains(actor);
        const [x, y] = event.get_coords();
        const [left, top] = this.dialogLayout.get_transformed_position();
        const [width, height] = this.dialogLayout.get_transformed_size();
        return x >= left && x < left + width && y >= top && y < top + height;
    }

    /** Opens on the main display, and moves along when it changes. */
    open() {
        const opened = super.open();
        this._monitorConstraint.primary = true;
        return opened;
    }

    async _refresh() {
        try {
            const {serial, screens} = await readDisplays();
            if (this._destroyed)
                return;
            this._serial = serial;
            this._screens = describe(screens);
        } catch (error) {
            if (this._destroyed)
                return;
            this._body.destroy_all_children();
            this._body.add_child(new St.Label({text: `Could not read the screens: ${error.message}`}));
            return;
        }
        if (!this._screens.some(s => s.connector === this._selected))
            this._selected = (this._screens.find(s => !s.panel) ?? this._screens[0])?.connector ?? null;
        this._build();
    }

    /** Applies a change to a copy of the screens, laid out again. */
    async _apply(change, afterwards) {
        if (this._busy)
            return;
        this._busy = true;
        try {
            if (await applyChange(this._serial, this._screens, change))
                await afterwards?.();
        } catch (error) {
            log(`Zenbook Duo Snap: display change failed: ${error.message}`);
        } finally {
            this._busy = false;
            await this._refresh();
        }
    }

    _build() {
        this._body.destroy_all_children();
        const tabs = new St.BoxLayout({style_class: 'duo-snap-tabs', x_align: Clutter.ActorAlign.CENTER});
        for (const {id, label} of TABS) {
            const tab = new St.Button({style_class: 'duo-snap-tab', label, can_focus: true});
            if (id === lastTab)
                tab.add_style_pseudo_class('checked');
            tab.connect('clicked', () => {
                lastTab = id;
                this._build();
            });
            tabs.add_child(tab);
        }
        this._body.add_child(tabs);
        this._body.add_child(this._buildMap());

        const selected = this._screens.find(s => s.connector === this._selected);
        if (!selected)
            return;
        if (lastTab === 'projection')
            this._buildProjection(selected);
        else if (!selected.on)
            this._body.add_child(note(`${selected.label} is off. Turn it on under Projection.`));
        else if (lastTab === 'position')
            this._buildPositionTab(selected);
        else
            this._buildScaleRotation(selected);
    }

    /** What is shown (all, Duo only, external only), and per screen on/off and main display. */
    _buildProjection(selected) {
        const screens = this._screens;
        // With the keyboard attached it lies on the bottom panel: that one stays off.
        const covered = s => s.panel === 'bottom' && this._keyboardAttached();
        const usable = screens.filter(s => !covered(s));
        const externals = screens.filter(s => !s.panel);
        const panels = usable.filter(s => s.panel);
        if (externals.length > 0) {
            const allOn = usable.every(s => s.on);
            const duoOnly = panels.some(s => s.on) && externals.every(s => !s.on);
            const externalOnly = panels.every(s => !s.on) && externals.some(s => s.on);
            this._body.add_child(row('Show', [
                chip('Everything', allOn, () => this._apply(all => all.forEach(s => {
                    s.on = !covered(s);
                }))),
                chip('Duo only', duoOnly, () => this._apply(all => all.forEach(s => {
                    s.on = Boolean(s.panel) && !covered(s);
                }))),
                chip('External only', externalOnly, () => this._apply(all => all.forEach(s => {
                    s.on = !s.panel;
                    s.primary = s.on && s === all.find(x => !x.panel);
                }))),
                // Any other combination, set up per screen below: shown, not clickable.
                ...(allOn || duoOnly || externalOnly ? [] : [indicator('Custom')]),
            ]));
        }

        const onCount = screens.filter(s => s.on).length;
        const setOn = on => this._apply(all => {
            all.find(s => s.connector === selected.connector).on = on;
        });
        this._body.add_child(row(selected.label, [
            chip('On', selected.on, () => setOn(true), selected.on || !covered(selected)),
            // The last screen that is on cannot be turned off.
            chip('Off', !selected.on, () => setOn(false), !selected.on || onCount > 1),
            chip('Main display', selected.primary, () => this._apply(all => all.forEach(s => {
                s.primary = s.connector === selected.connector;
            })), selected.on && !selected.primary && !selected.mirrorOf),
        ]));
        this._buildPicture(selected, covered);
        this._buildTableMode(covered);
        if (covered(selected) && !selected.on)
            this._body.add_child(note('Detach the keyboard to turn on the bottom screen.'));
    }

    /**
     * An external screen: its own desktop, or a copy (mirror) of one Duo
     * panel or another external screen. Mirroring needs a resolution both support; the panel switches
     * to it while mirrored.
     */
    _buildPicture(selected, covered) {
        if (selected.panel)
            return;
        // A Duo panel or another external screen, as long as it is not a mirror itself.
        const sources = this._screens.filter(s => s !== selected && s.on && !s.mirrorOf && !covered(s));
        if (sources.length === 0)
            return;
        const mirrorOf = connector => all => {
            if (connector)
                rememberedMirrors.set(selected.connector, connector);
            else
                rememberedMirrors.delete(selected.connector);
            const screen = all.find(s => s.connector === selected.connector);
            screen.on = true;
            screen.mirrorOf = connector;
        };
        this._body.add_child(row('Picture', [
            chip('Own desktop', selected.on && !selected.mirrorOf, () => this._apply(mirrorOf(null))),
            ...sources.map(source => {
                const others = this._screens.filter(s => s.mirrorOf === source.connector && s !== selected);
                // Not the top screen in table mode: the mirror would be upside down too.
                const possible = mirrorMode([source, ...others, selected]) !== null &&
                    !(source.panel === 'top' && source.transform === 2);
                return chip(`Mirror ${source.label}`, selected.mirrorOf === source.connector,
                    () => this._apply(mirrorOf(source.connector)), possible);
            }),
        ]));
    }

    /**
     * Table mode: the laptop flat between two people, the top screen upside
     * down for the person across (e.g. slides there, the presenter view on
     * the bottom screen, which becomes the main display).
     */
    _buildTableMode(covered) {
        const top = this._screens.find(s => s.panel === 'top');
        const bottom = this._screens.find(s => s.panel === 'bottom');
        if (!top?.on || !bottom?.on || covered(bottom) || top.mirrorOf || bottom.mirrorOf)
            return;
        const on = top.transform === 2;
        // Mirrors share their source's rotation: a mirrored top screen can't flip alone.
        const topMirrored = this._screens.some(s => s.mirrorOf === top.connector);
        const set = flipped => this._apply(all => {
            for (const s of all) {
                if (s.panel === 'top')
                    s.transform = flipped ? 2 : 0;
                s.primary = s.panel === (flipped ? 'bottom' : 'top');
            }
        });
        this._body.add_child(row('Table mode', [
            chip('Off', !on, () => set(false)),
            chip('On', on, () => set(true), on || !topMirrored),
        ]));
        this._body.add_child(note(on
            ? 'The top screen is upside down for the person across; the bottom screen is the main display.'
            : 'Lay the laptop flat: the top screen turns upside down for the person across the table.'));
    }

    _buildPositionTab(selected) {
        const source = this._screens.find(s => s.connector === selected.mirrorOf);
        if (source?.panel) {
            this._body.add_child(note(`This screen mirrors the ${source.label.toLowerCase()}, so it sits where that one is.`));
            return;
        }
        if (source) {
            // Mirrored screens are one screen in the layout: place them together.
            this._body.add_child(note(`Mirrors ${source.label}: they move together.`));
            this._buildPosition(source);
            return;
        }
        if (selected.panel) {
            this._body.add_child(note(this._screens.some(s => !s.panel)
                ? 'The Duo panels stay stacked. Pick an external screen to place it.'
                : 'No external screen is connected.'));
            return;
        }
        this._buildPosition(selected);
    }

    /**
     * A Duo panel's brightness (both panels together when the Control Panel's
     * "same brightness" is on). External screens have their own buttons.
     */
    _buildBrightness(selected) {
        const both = sameBrightness();
        const percent = readPanelBrightness(selected.panel);
        if (percent === null)
            return;
        const box = new St.BoxLayout({
            style_class: 'duo-snap-display-row',
            orientation: Clutter.Orientation.VERTICAL,
        });
        box.add_child(new St.Label({text: 'Brightness', style_class: 'duo-snap-display-row-title'}));
        const line = new St.BoxLayout({style_class: 'duo-snap-brightness'});
        line.add_child(new St.Icon({icon_name: 'display-brightness-symbolic', icon_size: 16}));
        const slider = new Slider.Slider(percent / 100);
        slider.x_expand = true;
        line.add_child(slider);
        box.add_child(line);
        this._body.add_child(box);

        // One request at a time while dragging; the last value always gets sent.
        let inFlight = false, queued = null;
        const send = async value => {
            if (inFlight) {
                queued = value;
                return;
            }
            inFlight = true;
            try {
                await setPanelBrightness(both ? 'both' : selected.panel, Math.max(1, value * 100));
            } catch (error) {
                log(`Zenbook Duo Snap: brightness failed: ${error.message}`);
            } finally {
                inFlight = false;
                const next = queued;
                queued = null;
                if (next !== null)
                    send(next);
            }
        };
        slider.connect('notify::value', () => send(slider.value));

        // Both panels together (the Control Panel's "same brightness").
        if (readPanelBrightness('bottom') !== null) {
            const toggle = chip('Both Duo screens', both, async () => {
                await setSameBrightness(!both);
                if (!both)
                    await setPanelBrightness('both', Math.max(1, slider.value * 100));
                this._build();
            });
            const toggles = new St.BoxLayout({style_class: 'duo-snap-chips'});
            toggles.add_child(toggle);
            box.add_child(toggles);
        }
    }

    _buildScaleRotation(selected) {
        const source = this._screens.find(s => s.connector === selected.mirrorOf);
        if (source) {
            const name = source.panel ? `the ${source.label.toLowerCase()}` : source.label;
            this._body.add_child(note(`This screen mirrors ${name} and uses its scale and rotation.`));
            return;
        }
        const mirrors = this._screens.filter(s => s.mirrorOf === selected.connector);
        const change = fn => this._apply(all => fn(all.find(s => s.connector === selected.connector)));
        if (selected.panel)
            this._buildBrightness(selected);
        if (!selected.panel) {
            this._body.add_child(row('Rotation', ROTATIONS.map(({transform, label}) =>
                chip(label, selected.transform === transform, () => change(s => {
                    s.transform = transform;
                })))));
        }
        // Mirrored: only scales every screen of the group supports, for that group alone.
        const scales = offeredScales(selected.supportedScales, selected.scale).filter(scale =>
            mirrors.every(m => m.supportedScales.some(x => Math.abs(x - scale) < 0.01)));
        const together = selected.panel && mirrors.length === 0;
        if (mirrors.length > 0)
            this._body.add_child(note(`Mirrored to ${mirrors.map(m => m.label).join(' and ')}: the scale applies to them too.`));
        this._body.add_child(row(together ? 'Scale (both panels)' : 'Scale', scales.map(scale =>
            chip(`${Math.round(scale * 100)}%`, Math.abs(selected.scale - scale) < 0.01, () => {
                if (together) {
                    // Both panels share one scale, also kept on attach/detach.
                    // A mirrored panel keeps the scale its mirror group uses.
                    this._apply(all => all.filter(s => s.panel && !all.some(m => m.mirrorOf === s.connector))
                        .forEach(s => {
                            s.scale = scale;
                        }), () => savePanelScale(scale));
                } else {
                    change(s => {
                        s.scale = scale;
                    });
                }
            }))));
    }

    /**
     * Position of an external screen: the side (cross), the screen it is
     * against (From), and the offset along that edge. A choice that would
     * overlap another screen is greyed out.
     */
    _buildPosition(selected) {
        const screens = this._screens;
        const on = screens.filter(s => s.on);
        const panelsOn = on.filter(s => s.panel);
        // Not against itself, nor a screen placed against it (no loops).
        const followers = placedAgainst(selected.connector,
            new Map(screens.filter(s => !s.panel).map(s => [s.connector, s.placement])));
        const anchors = [
            ...(panelsOn.length === 2 ? ['duo'] : []),
            ...['top', 'bottom'].filter(name => panelsOn.some(s => s.panel === name)),
            ...on.filter(s => !s.panel && !s.mirrorOf && s !== selected && !followers.has(s.connector))
                .map(s => s.connector),
        ];
        if (anchors.length === 0)
            return;
        const current = selected.placement;
        const base = {
            anchor: current && anchors.includes(current.anchor) ? current.anchor : anchors[0],
            side: current?.side ?? 'right',
            offset: current?.offset ?? 0,
            shift: current?.shift ?? 0,
        };

        // A placement works if nothing has to move elsewhere for it.
        const before = new Set(tryLayout(screens).moved);
        const withPlacement = placement => all => {
            all.find(s => s.connector === selected.connector).placement = placement;
        };
        const works = placement => {
            const {moved} = tryLayout(screens, withPlacement(placement));
            return !moved.has(selected.connector) && [...moved].every(c => before.has(c));
        };
        const choose = placement => {
            remembered.set(selected.connector, placement);
            this._apply(withPlacement(placement));
        };
        const option = (label, checked, placement) => chip(label, checked,
            () => choose(placement), checked || works(placement));

        const sides = {};
        for (const side of Object.keys(SIDE_LABELS)) {
            const placement = {...base, side};
            sides[side] = option(SIDE_LABELS[side], current?.side === side, placement);
        }
        this._body.add_child(crossRow('Position', sides));

        const labelOf = anchor => ANCHOR_LABELS[anchor] ??
            screens.find(s => s.connector === anchor)?.label ?? anchor;
        this._body.add_child(row('From', anchors.map(anchor => option(labelOf(anchor),
            current?.anchor === anchor, {...base, anchor}))));

        if (!current)
            return;
        const vertical = current.side === 'left' || current.side === 'right';
        // Same length as the edge it is against: lined up is the same as centred.
        const sizeOf = anchor => {
            if (anchor === 'duo') {
                const sizes = panelsOn.map(logicalSize);
                return {width: Math.max(...sizes.map(z => z.width)), height: sizes.reduce((h, z) => h + z.height, 0)};
            }
            const screen = anchor === 'top' || anchor === 'bottom'
                ? panelsOn.find(s => s.panel === anchor) : on.find(s => s.connector === anchor);
            return logicalSize(screen);
        };
        const edge = sizeOf(current.anchor), own = logicalSize(selected);
        const sameLength = vertical ? edge.height === own.height : edge.width === own.width;
        const labels = OFFSET_LABELS[vertical ? 'vertical' : 'horizontal'];
        const offsetChip = (offset, sensitive) => {
            const placement = {...base, offset, shift: 0};
            const checked = current.offset === offset;
            return chip(labels[offset], checked, () => choose(placement),
                checked || (sensitive && works(placement)));
        };
        const offsets = OFFSETS.map(offset => offsetChip(offset, true));
        if (current.offset === null)
            offsets.push(indicator('Custom'));
        this._body.add_child(row(vertical ? 'Offset up/down' : 'Offset left/right', offsets, 6));
        // Edges of the same length are lined up when centred already.
        this._body.add_child(row('Line up', EDGE_OFFSETS.map(offset => offsetChip(offset, !sameLength))));
    }

    /** The screens that are on, arranged like the real ones; off ones below. */
    _buildMap() {
        const box = new St.BoxLayout({
            style_class: 'duo-snap-display-map-box',
            orientation: Clutter.Orientation.VERTICAL,
        });
        const on = this._screens.filter(s => s.on && !s.mirrorOf);
        const map = new St.Widget({
            style_class: 'duo-snap-pick-map',
            width: MAP_WIDTH,
            height: MAP_HEIGHT,
            x_align: Clutter.ActorAlign.CENTER,
        });
        const rects = screenMap(on.map(s => ({x: s.x, y: s.y, ...logicalSize(s)})), MAP_WIDTH, MAP_HEIGHT);
        on.forEach((screen, i) => {
            const tile = new St.Button({
                style_class: 'duo-snap-pick-screen',
                can_focus: true,
                x: rects[i].x,
                y: rects[i].y,
                width: rects[i].width,
                height: rects[i].height,
                child: new St.Label({
                    text: [screen.label, ...this._screens.filter(m => m.mirrorOf === screen.connector)
                        .map(m => `= ${m.label}`)].join('\n'),
                    style_class: 'duo-snap-pick-name',
                    x_align: Clutter.ActorAlign.CENTER,
                    y_align: Clutter.ActorAlign.CENTER,
                }),
            });
            if (screen.connector === this._selected)
                tile.add_style_pseudo_class('checked');
            tile.connect('clicked', () => {
                this._selected = screen.connector;
                this._build();
            });
            map.add_child(tile);
        });
        box.add_child(map);

        const mirrors = this._screens.filter(s => s.on && s.mirrorOf);
        if (mirrors.length > 0) {
            box.add_child(row('Mirroring', mirrors.map(screen => chip(screen.label,
                screen.connector === this._selected, () => {
                    this._selected = screen.connector;
                    this._build();
                }))));
        }
        const off = this._screens.filter(s => !s.on);
        if (off.length > 0) {
            box.add_child(row('Connected, turned off', off.map(screen => chip(screen.label,
                screen.connector === this._selected, () => {
                    this._selected = screen.connector;
                    this._build();
                }))));
        }
        return box;
    }
});
