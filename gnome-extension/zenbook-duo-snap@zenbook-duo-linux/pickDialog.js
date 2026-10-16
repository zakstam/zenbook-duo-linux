// The dialog for F8 / Shift+F8 when more than two screens are on: a map of
// the screens, arranged like the real ones, to pick the two to swap (or the
// screen to move the focused window to). Mouse, touch, and keyboard (arrows
// to move between screens, Space to pick, Enter to swap, Esc to cancel).

import Clutter from 'gi://Clutter';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as Dialog from 'resource:///org/gnome/shell/ui/dialog.js';
import * as ModalDialog from 'resource:///org/gnome/shell/ui/modalDialog.js';

import {screenMap, togglePick} from './zones.js';

const MAP_WIDTH = 380;
const MAP_HEIGHT = 230;

export const ScreenPickDialog = GObject.registerClass(
class ScreenPickDialog extends ModalDialog.ModalDialog {
    /**
     * @param {object} params
     * @param params.monitors Main.layoutManager.monitors
     * @param params.named namedScreens(): the screens to show
     * @param params.source with a window to move: its screen's index (fixed);
     *   -1 to pick two screens to swap
     * @param params.picked the screens picked at the start (swap)
     * @param params.onDone called with the two picked screens' indices
     *   (swap: in pick order; move: source, then target)
     */
    _init({monitors, named, source, picked, onDone}) {
        super._init({styleClass: 'duo-snap-pick-dialog'});
        this._source = source;
        this._picked = source >= 0 ? [] : [...picked];
        this._onDone = onDone;
        const moving = source >= 0;

        this.contentLayout.add_child(new Dialog.MessageDialogContent({
            title: moving ? 'Move Window' : 'Swap Windows',
            description: moving
                ? 'Pick the screen to move the window to.'
                : 'Pick two screens; their windows trade places.',
        }));

        const map = new St.Widget({
            style_class: 'duo-snap-pick-map',
            width: MAP_WIDTH,
            height: MAP_HEIGHT,
            x_align: Clutter.ActorAlign.CENTER,
        });
        const rects = screenMap(named.map(s => monitors[s.index]), MAP_WIDTH, MAP_HEIGHT);
        this._tiles = named.map((screen, i) => {
            const isSource = screen.index === source;
            const tile = new St.Button({
                style_class: 'duo-snap-pick-screen',
                can_focus: !isSource,
                reactive: !isSource,
                x: rects[i].x,
                y: rects[i].y,
                width: rects[i].width,
                height: rects[i].height,
            });
            if (isSource)
                tile.add_style_class_name('duo-snap-pick-source');
            const box = new St.BoxLayout({
                orientation: Clutter.Orientation.VERTICAL,
                x_align: Clutter.ActorAlign.CENTER,
                y_align: Clutter.ActorAlign.CENTER,
            });
            const badge = new St.Label({
                style_class: 'duo-snap-pick-badge',
                x_align: Clutter.ActorAlign.CENTER,
            });
            box.add_child(badge);
            box.add_child(new St.Label({
                text: screen.name,
                style_class: 'duo-snap-pick-name',
                x_align: Clutter.ActorAlign.CENTER,
            }));
            tile.set_child(box);
            tile.connect('clicked', () => this._pick(screen.index));
            map.add_child(tile);
            return {screen, tile, badge, isSource};
        });
        this.contentLayout.add_child(map);

        const buttons = [{
            label: 'Cancel',
            action: () => this.close(),
            key: Clutter.KEY_Escape,
        }];
        if (!moving) {
            buttons.push({
                label: 'Swap',
                action: () => this._finish(),
                default: true,
            });
        }
        this.setButtons(buttons);
        this._swapButton = moving ? null : buttons[1].button;
        this._update();
    }

    _pick(index) {
        if (this._source >= 0) {
            this.close();
            this._onDone(this._source, index);
            return;
        }
        this._picked = togglePick(this._picked, index);
        this._update();
    }

    _finish() {
        if (this._picked.length !== 2)
            return;
        this.close();
        this._onDone(...this._picked);
    }

    _update() {
        for (const {screen, tile, badge, isSource} of this._tiles) {
            const order = this._picked.indexOf(screen.index);
            if (isSource)
                badge.text = 'Window';
            else
                badge.text = order >= 0 ? `${order + 1}` : '';
            if (order >= 0)
                tile.add_style_pseudo_class('checked');
            else
                tile.remove_style_pseudo_class('checked');
        }
        if (this._swapButton) {
            const ready = this._picked.length === 2;
            this._swapButton.reactive = ready;
            this._swapButton.can_focus = ready;
        }
    }
});
