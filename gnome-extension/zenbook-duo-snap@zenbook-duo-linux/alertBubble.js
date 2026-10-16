// A warning bubble that stays until it is acknowledged: for things that must
// not be missed, like the detached keyboard's battery running low. It looks
// like and sits where GNOME's own pop-ups (volume, brightness) do, at the
// bottom centre of every screen; a button on any screen closes it everywhere
// (OK, or the buttons given, each with an action that is told which screen it
// was pressed on). A newer alert with the same id replaces it.

import Clutter from 'gi://Clutter';
import Pango from 'gi://Pango';
import St from 'gi://St';

import * as Layout from 'resource:///org/gnome/shell/ui/layout.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

export class AlertBubble {
    constructor(id, icon, title, message, onClose, buttons = [{label: 'OK'}]) {
        this.alertId = id;
        this._content = {icon, title, message, buttons};
        this._onClose = onClose;
        this._screens = [];
        this._monitorsId = Main.layoutManager.connect('monitors-changed', () => this._build());
        this._build();
    }

    /** One bubble per screen, each in a screen-sized holder that aligns it. */
    _build() {
        this._clear();
        Main.layoutManager.monitors.forEach((_monitor, index) => {
            const holder = new St.Bin({
                child: this._bubble(index),
                x_align: Clutter.ActorAlign.CENTER,
                y_align: Clutter.ActorAlign.END,
            });
            holder.add_constraint(new Layout.MonitorConstraint({index}));
            Main.uiGroup.add_child(holder);
            // Below GNOME's own pop-ups, so a volume change still shows.
            const osd = Main.osdWindowManager._osdWindows?.[0];
            if (osd?.get_parent() === Main.uiGroup)
                Main.uiGroup.set_child_below_sibling(holder, osd);
            this._screens.push(holder);
        });
    }

    /**
     * The bubble on screen `index`. A button's label and `sensitive` may be
     * functions of that index, and its action gets it.
     */
    _bubble(index) {
        const {icon, title, message, buttons} = this._content;
        const bubble = new St.BoxLayout({
            style_class: 'osd-window duo-snap-alert',
            orientation: Clutter.Orientation.HORIZONTAL,
            reactive: true,
        });
        bubble.add_child(new St.Icon({
            icon_name: icon,
            style_class: 'duo-snap-alert-icon',
            y_align: Clutter.ActorAlign.CENTER,
        }));
        const text = new St.BoxLayout({
            style_class: 'duo-snap-alert-text',
            orientation: Clutter.Orientation.VERTICAL,
            x_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        });
        text.add_child(new St.Label({text: title, style_class: 'duo-snap-alert-title'}));
        const body = new St.Label({text: message, style_class: 'duo-snap-alert-message'});
        body.clutter_text.line_wrap = true;
        body.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
        text.add_child(body);
        bubble.add_child(text);

        const actions = new St.BoxLayout({
            style_class: 'duo-snap-alert-actions',
            orientation: buttons.length > 1 ? Clutter.Orientation.VERTICAL : Clutter.Orientation.HORIZONTAL,
            y_align: Clutter.ActorAlign.CENTER,
        });
        for (const {label, action, sensitive} of buttons) {
            const enabled = typeof sensitive === 'function' ? sensitive(index) : true;
            const button = new St.Button({
                label: typeof label === 'function' ? label(index) : label,
                style_class: 'duo-snap-alert-button',
                can_focus: enabled,
                reactive: enabled,
            });
            if (!enabled)
                button.add_style_pseudo_class('insensitive');
            button.connect('clicked', () => {
                this.dismiss();
                action?.(index);
            });
            actions.add_child(button);
        }
        bubble.add_child(actions);
        return bubble;
    }

    _clear() {
        for (const holder of this._screens)
            holder.destroy();
        this._screens = [];
    }

    dismiss() {
        if (this._dismissed)
            return;
        this._dismissed = true;
        Main.layoutManager.disconnect(this._monitorsId);
        this._clear();
        this._onClose?.(this);
    }
}
