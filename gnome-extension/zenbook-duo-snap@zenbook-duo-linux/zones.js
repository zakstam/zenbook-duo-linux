// Pure geometry for the snap popup, kept free of GNOME imports so it can be
// tested with plain gjs (see gnome-extension/tests).

/**
 * The layout icons of one screen's box: each icon is drawn as rows of cells,
 * and each cell is a zone the window can be dropped on. "free" moves the window
 * to the screen as a normal, untiled window (see freeRect).
 */
export const SCREEN_LAYOUTS = [
    {name: 'full', rows: [['full']]},
    {name: 'free', rows: [['free']]},
    {name: 'halves', rows: [['left', 'right']]},
    {name: 'stacked', rows: [['top'], ['bottom']]},
    {name: 'thirds', rows: [['third-left', 'third-middle', 'third-right']]},
    {name: 'quarters', rows: [['top-left', 'top-right'], ['bottom-left', 'bottom-right']]},
];

/** The layout icons of the "both screens" box, stretched over both screens. */
export const SPAN_LAYOUTS = [
    {name: 'span-full', rows: [['span-full']]},
    {name: 'span-halves', rows: [['span-left', 'span-right']]},
];

/**
 * The rectangle a zone covers inside a screen's work area.
 * @param {string} zone a cell of SCREEN_LAYOUTS other than "free"
 * @param {{x: number, y: number, width: number, height: number}} area
 */
export function zoneRect(zone, area) {
    const third = Math.floor(area.width / 3);
    const halfW = Math.floor(area.width / 2);
    const halfH = Math.floor(area.height / 2);
    const right = {x: area.x + halfW, width: area.width - halfW};
    const bottom = {y: area.y + halfH, height: area.height - halfH};
    const left = {x: area.x, width: halfW};
    const top = {y: area.y, height: halfH};
    const fullW = {x: area.x, width: area.width};
    const fullH = {y: area.y, height: area.height};
    const parts = {
        'full': [fullW, fullH],
        'left': [left, fullH],
        'right': [right, fullH],
        'top': [fullW, top],
        'bottom': [fullW, bottom],
        'top-left': [left, top],
        'top-right': [right, top],
        'bottom-left': [left, bottom],
        'bottom-right': [right, bottom],
        'third-left': [{x: area.x, width: third}, fullH],
        'third-middle': [{x: area.x + third, width: third}, fullH],
        'third-right': [{x: area.x + 2 * third, width: area.width - 2 * third}, fullH],
    }[zone];
    if (!parts)
        throw new Error(`Unknown zone: ${zone}`);
    return {...parts[0], ...parts[1]};
}

/**
 * The rectangle a "both screens" zone covers: from the top of the top screen's
 * work area to the bottom of the bottom screen's.
 */
export function spanRect(zone, topArea, bottomArea) {
    const x = Math.max(topArea.x, bottomArea.x);
    const right = Math.min(topArea.x + topArea.width, bottomArea.x + bottomArea.width);
    const y = topArea.y;
    const height = bottomArea.y + bottomArea.height - topArea.y;
    const width = right - x;
    const half = Math.floor(width / 2);
    switch (zone) {
    case 'span-full':
        return {x, y, width, height};
    case 'span-left':
        return {x, y, width: half, height};
    case 'span-right':
        return {x: x + half, y, width: width - half, height};
    default:
        throw new Error(`Unknown span zone: ${zone}`);
    }
}

/** Whether a point lies inside a rectangle (right/bottom edges exclusive). */
export function contains(rect, px, py) {
    return px >= rect.x && px < rect.x + rect.width &&
        py >= rect.y && py < rect.y + rect.height;
}

/**
 * Finds the drop target under the pointer.
 * @param {Array<{target: object, rect: object, enabled: boolean}>} targets
 *   zone cells listed before the boxes that contain them, so a zone wins over
 *   its box
 * @returns {object|null} the target of the first enabled hit
 */
export function hitTest(targets, px, py) {
    for (const {target, rect, enabled} of targets) {
        if (enabled && contains(rect, px, py))
            return target;
    }
    return null;
}

/**
 * Indices of two monitors stacked vertically (the upper one and the closest one
 * directly below it, overlapping horizontally), or -1 for a missing one.
 * @param {Array<{x: number, y: number, width: number, height: number}>} monitors
 */
export function stackedPair(monitors) {
    let best = {top: -1, bottom: -1, gap: Infinity};
    monitors.forEach((upper, top) => {
        monitors.forEach((lower, bottom) => {
            const overlaps = lower.x < upper.x + upper.width && upper.x < lower.x + lower.width;
            const gap = lower.y - (upper.y + upper.height);
            if (top !== bottom && overlaps && gap >= 0 && gap < best.gap)
                best = {top, bottom, gap};
        });
    });
    if (best.top < 0 && monitors.length > 0)
        return {top: 0, bottom: -1};
    return {top: best.top, bottom: best.bottom};
}

/** Gap between the pointer and the popup, so the popup never opens under it. */
export const POPUP_GAP = 32;
/** Minimum distance between the popup and the edge of the work area. */
export const POPUP_MARGIN = 8;

/**
 * Where to show the popup: centred horizontally on the pointer, below it, or
 * above it when there is no room below; always inside the work area.
 */
export function popupPosition(px, py, width, height, area) {
    const clamp = (value, min, max) => Math.max(min, Math.min(value, max));
    const x = clamp(Math.round(px - width / 2),
        area.x + POPUP_MARGIN, area.x + area.width - width - POPUP_MARGIN);
    const below = py + POPUP_GAP;
    const above = py - POPUP_GAP - height;
    const fitsBelow = below + height <= area.y + area.height - POPUP_MARGIN;
    const y = clamp(fitsBelow ? below : above,
        area.y + POPUP_MARGIN, area.y + area.height - height - POPUP_MARGIN);
    return {x, y};
}

/**
 * For each edge of a placed window, whether it lies on the edge of a screen's
 * work area ("monitor") or not ("window", i.e. next to another tiled window).
 * Mutter tells the app its edges are constrained ("tiled"), so it keeps the
 * size it is given instead of shrinking back to the size of one screen.
 * @param rect the window's frame rectangle
 * @param areas the work areas the window may touch (one screen, or both)
 */
export function edgeConstraints(rect, areas) {
    const on = (value, key) => areas.some(area => value === key(area));
    const right = rect.x + rect.width;
    const bottom = rect.y + rect.height;
    return {
        top: on(rect.y, a => a.y) ? 'monitor' : 'window',
        left: on(rect.x, a => a.x) ? 'monitor' : 'window',
        right: on(right, a => a.x + a.width) ? 'monitor' : 'window',
        bottom: on(bottom, a => a.y + a.height) ? 'monitor' : 'window',
    };
}

/** How far apart (px) two edges may be and still count as touching. */
export const LINK_TOLERANCE = 2;
/** Smallest width/height a linked neighbour is squeezed to. */
export const MIN_LINKED_SIZE = 120;

/**
 * Linked resizing: when an edge of a tiled window moves, the dividing line it
 * lies on moves with it. Every neighbour with an edge on that line, connected
 * to the resized window along the line, follows (so the middle cross of four
 * quarters moves as a whole). A line only links windows when some window lies
 * on its other side, so windows merely sharing a screen edge are not linked.
 * Computed from the rectangles at the start of the resize, so repeated updates
 * do not drift. Neighbours are never squeezed below MIN_LINKED_SIZE.
 * @param start the resized window's rectangle when the resize began
 * @param now its current rectangle
 * @param neighbours [{id, rect}] other tiled windows on the same screen, as at the start
 * @returns [{id, rect}] the neighbours that change, with their new rectangles
 */
export function linkedResize(start, now, neighbours) {
    const near = (a, b) => Math.abs(a - b) <= LINK_TOLERANCE;
    const end = (r, axis) => axis === 'x' ? r.x + r.width : r.y + r.height;
    const begin = (r, axis) => axis === 'x' ? r.x : r.y;
    const other = axis => (axis === 'x' ? 'y' : 'x');

    const moved = [];
    for (const axis of ['x', 'y']) {
        if (begin(now, axis) !== begin(start, axis))
            moved.push({axis, from: begin(start, axis), to: begin(now, axis), side: 'begin'});
        if (end(now, axis) !== end(start, axis))
            moved.push({axis, from: end(start, axis), to: end(now, axis), side: 'end'});
    }

    const updated = new Map();
    for (const {axis, from, to, side} of moved) {
        // Neighbours on the line, chained along it from the resized window.
        const along = other(axis);
        let lo = begin(start, along), hi = end(start, along);
        const members = [];
        let grew = true;
        while (grew) {
            grew = false;
            for (const neighbour of neighbours) {
                const r = neighbour.rect;
                if (members.includes(neighbour))
                    continue;
                const onBegin = near(begin(r, axis), from);
                const onEnd = near(end(r, axis), from);
                const connected = begin(r, along) <= hi + LINK_TOLERANCE &&
                    end(r, along) >= lo - LINK_TOLERANCE;
                if ((onBegin || onEnd) && connected) {
                    members.push(neighbour);
                    lo = Math.min(lo, begin(r, along));
                    hi = Math.max(hi, end(r, along));
                    grew = true;
                }
            }
        }
        // A dividing line has windows on both sides of it.
        const opposite = side === 'begin' ? 'end' : 'begin';
        const isDivider = members.some(n =>
            near(opposite === 'begin' ? begin(n.rect, axis) : end(n.rect, axis), from));
        if (!isDivider)
            continue;

        for (const neighbour of members) {
            const rect = updated.get(neighbour.id) ?? {...neighbour.rect};
            const size = axis === 'x' ? 'width' : 'height';
            if (near(begin(neighbour.rect, axis), from)) {
                // Its leading edge is on the line: move it, keep the far edge.
                const far = rect[axis] + rect[size];
                rect[axis] = Math.min(to, far - MIN_LINKED_SIZE);
                rect[size] = far - rect[axis];
            } else {
                // Its trailing edge is on the line.
                rect[size] = Math.max(MIN_LINKED_SIZE, to - rect[axis]);
            }
            updated.set(neighbour.id, rect);
        }
    }
    return [...updated].map(([id, rect]) => ({id, rect}));
}

/**
 * Where "free" puts a window: its size from before it was tiled (or its
 * current size), never larger than the screen, centred on the screen.
 */
export function freeRect(size, area) {
    const width = Math.min(size.width, area.width);
    const height = Math.min(size.height, area.height);
    return {
        x: area.x + Math.round((area.width - width) / 2),
        y: area.y + Math.round((area.height - height) / 2),
        width,
        height,
    };
}

/**
 * The rectangle a tiled window gets back when it is dragged away: its free
 * size, positioned so the pointer stays at the same relative spot across its
 * width (grabbing a wide window at a third keeps it at a third).
 */
export function restoredUnderPointer(frame, size, px) {
    const ratio = frame.width > 0 ? (px - frame.x) / frame.width : 0.5;
    return {
        x: Math.round(px - ratio * size.width),
        y: frame.y,
        width: size.width,
        height: size.height,
    };
}

/**
 * Index of the monitor containing a point (e.g. a finger: during a touch drag
 * Mutter's "current monitor" is still the mouse pointer's), or the closest one.
 */
export function monitorAt(monitors, px, py) {
    let best = -1, bestDistance = Infinity;
    monitors.forEach((m, index) => {
        const dx = Math.max(m.x - px, 0, px - (m.x + m.width - 1));
        const dy = Math.max(m.y - py, 0, py - (m.y + m.height - 1));
        const distance = Math.hypot(dx, dy);
        if (distance < bestDistance) {
            best = index;
            bestDistance = distance;
        }
    });
    return best;
}

/**
 * The screen for the on-screen keyboard with both Duo screens on: the bottom
 * screen, unless the text field is on the bottom screen where the keyboard
 * would cover it; then the top screen. -1 leaves it to GNOME (one screen on).
 * @param fieldMonitor the field's monitor index, or -1 when GNOME does not
 *   know where the text cursor is
 * @param fieldBottom the bottom edge of the text field
 * @param keyboardTop where the keyboard's top edge is on the bottom screen
 */
export function keyboardScreenFor(screens, fieldMonitor, fieldBottom, keyboardTop) {
    if (screens.top < 0 || screens.bottom < 0)
        return -1;
    if (fieldMonitor === screens.bottom && fieldBottom > keyboardTop)
        return screens.top;
    return screens.bottom;
}

/**
 * Most external screens the popup shows. Meteor Lake graphics drive four
 * screens at once, two of them the Duo's own panels; with the bottom panel
 * off a third external screen fits.
 */
export const MAX_EXTERNAL_SCREENS = 3;

/**
 * Indices of the monitors that are not one of the Duo's panels, ordered left
 * to right (then top to bottom), at most MAX_EXTERNAL_SCREENS.
 * @param screens the panels' indices ({top, bottom}, -1 when off)
 */
export function externalScreens(monitors, screens) {
    return monitors
        .map((monitor, index) => ({monitor, index}))
        .filter(({index}) => index !== screens.top && index !== screens.bottom)
        .sort((a, b) => a.monitor.x - b.monitor.x || a.monitor.y - b.monitor.y)
        .slice(0, MAX_EXTERNAL_SCREENS)
        .map(({index}) => index);
}

/**
 * The icon of the microphone volume pop-up, like GNOME's input volume slider.
 * @param level the volume, 0 to 1
 */
export function micVolumeIcon(level, muted) {
    let name = 'high';
    if (muted || level <= 0)
        name = 'muted';
    else if (level < 1 / 3)
        name = 'low';
    else if (level < 2 / 3)
        name = 'medium';
    return `microphone-sensitivity-${name}-symbolic`;
}

/**
 * Every screen that is on, named for the swap dialog: the Duo panels, then
 * the external screens left to right.
 * @returns {Array<{index: number, name: string}>}
 */
export function namedScreens(monitors, screens) {
    const named = [];
    if (screens.top >= 0)
        named.push({index: screens.top, name: 'Top screen'});
    if (screens.bottom >= 0)
        named.push({index: screens.bottom, name: 'Bottom screen'});
    externalScreens(monitors, screens).forEach((index, n) =>
        named.push({index, name: `External ${n + 1}`}));
    return named;
}

/**
 * The two screens the swap dialog starts with: the last pair used if both
 * are still there, else the two Duo panels, else the first two screens.
 * @param named namedScreens()
 * @param lastNames names of the last pair, or null
 * @returns {number[]} monitor indices
 */
export function defaultSwapPair(named, lastNames) {
    const byName = name => named.find(s => s.name === name);
    if (lastNames && lastNames.every(byName))
        return lastNames.map(name => byName(name).index);
    const panels = named.filter(s => s.name === 'Top screen' || s.name === 'Bottom screen');
    const pair = panels.length === 2 ? panels : named.slice(0, 2);
    return pair.map(s => s.index);
}

/**
 * The screens' rectangles scaled into a box for the screen map, keeping the
 * arrangement and proportions, centred; `gap` px are taken off each side so
 * neighbouring screens stay apart.
 */
export function screenMap(monitors, width, height, gap = 3) {
    if (monitors.length === 0)
        return [];
    const left = Math.min(...monitors.map(m => m.x));
    const top = Math.min(...monitors.map(m => m.y));
    const right = Math.max(...monitors.map(m => m.x + m.width));
    const bottom = Math.max(...monitors.map(m => m.y + m.height));
    const scale = Math.min(width / (right - left), height / (bottom - top));
    const offsetX = (width - (right - left) * scale) / 2;
    const offsetY = (height - (bottom - top) * scale) / 2;
    return monitors.map(m => ({
        x: Math.round(offsetX + (m.x - left) * scale + gap),
        y: Math.round(offsetY + (m.y - top) * scale + gap),
        width: Math.round(m.width * scale - 2 * gap),
        height: Math.round(m.height * scale - 2 * gap),
    }));
}

/**
 * Picking screens in the swap dialog: picking a picked screen unpicks it;
 * a third pick replaces the older of the two.
 */
export function togglePick(picked, index) {
    if (picked.includes(index))
        return picked.filter(i => i !== index);
    return [...picked, index].slice(-2);
}

/** Whether a monitor is a screen other than the Duo panels (and is on). */
export function isExternal(screens, monitor) {
    return monitor >= 0 && monitor !== screens.top && monitor !== screens.bottom;
}

/**
 * The keyboard's screen for a text field: after a touch on an external
 * screen (e.g. a touch monitor), that screen; otherwise as keyboardScreenFor.
 * @param touched the monitor of the last touch, or -1
 */
export function keyboardScreenForTouch(screens, touched, fieldMonitor, fieldBottom, keyboardTop) {
    if (isExternal(screens, touched))
        return touched;
    return keyboardScreenFor(screens, fieldMonitor, fieldBottom, keyboardTop);
}

/**
 * Where a swipe up from a screen's bottom edge opens the keyboard, or -1 to
 * ignore it: on external screens and a lone Duo panel, there; with both
 * panels on, only swipes from the bottom panel count (GNOME counts the bottom
 * edge of every screen, which includes the seam between the panels).
 */
export function swipeScreen(screens, finger) {
    if (finger < 0)
        return -1;
    if (isExternal(screens, finger) || screens.top < 0 || screens.bottom < 0)
        return finger;
    return finger === screens.bottom ? finger : -1;
}

/** Mutter transforms used by the display controls (rotation, counterclockwise). */
export const TRANSFORMS = [0, 1, 2, 3];

/**
 * A screen's size in the desktop layout: its mode divided by its scale,
 * width and height swapped when rotated a quarter turn.
 */
export function logicalSize(screen) {
    const quarter = screen.transform % 2 === 1;
    const width = quarter ? screen.modeHeight : screen.modeWidth;
    const height = quarter ? screen.modeWidth : screen.modeHeight;
    return {
        width: Math.round(width / screen.scale),
        height: Math.round(height / screen.scale),
    };
}

/**
 * Offsets offered along an edge: 0 centred, -100/100 as far up/left or
 * down/right as GNOME allows (screens must share part of an edge; at least
 * EDGE_SHARE of the shorter one is kept so the pointer can cross), and
 * 'start'/'end' for edges lined up (top/left, bottom/right).
 */
export const OFFSETS = [-100, -50, 0, 50, 100];
export const EDGE_OFFSETS = ['start', 'end'];
export const EDGE_SHARE = 0.1;
const VERTICAL_SIDES = ['left', 'right'];

/** Whether two rectangles overlap (touching edges do not count). */
export function overlaps(a, b) {
    return a.x < b.x + b.width && b.x < a.x + a.width &&
        a.y < b.y + b.height && b.y < a.y + a.height;
}

/**
 * Where a screen of `size` goes on `side` of `anchor`: against that edge,
 * shifted along it by `offset` (see OFFSETS), or by `shift` pixels from the
 * anchor's top or left edge when offset is null (a custom place).
 */
export function placeBeside(anchor, {width, height}, side, offset, shift = 0) {
    const vertical = VERTICAL_SIDES.includes(side);
    const edge = vertical ? anchor.height : anchor.width;
    const own = vertical ? height : width;
    let along;
    if (offset === null)
        along = shift;
    else if (offset === 'start')
        along = 0;
    else if (offset === 'end')
        along = edge - own;
    else {
        // From centred to the furthest place that still shares EDGE_SHARE.
        const centre = (edge - own) / 2;
        const keep = Math.max(1, Math.round(Math.min(edge, own) * EDGE_SHARE));
        const reach = (edge + own) / 2 - keep;
        along = Math.round(centre + reach * offset / 100);
    }
    switch (side) {
    case 'left':
        return {x: anchor.x - width, y: anchor.y + along, width, height};
    case 'right':
        return {x: anchor.x + anchor.width, y: anchor.y + along, width, height};
    case 'above':
        return {x: anchor.x + along, y: anchor.y - height, width, height};
    default:
        return {x: anchor.x + along, y: anchor.y + anchor.height, width, height};
    }
}

/**
 * How `rect` sits against `anchor`: the side whose edge they share, and the
 * offset (centred, then edges lined up, then OFFSETS) or null with the shift in pixels
 * when it is somewhere in between. Null when they share no edge.
 */
export function placementBeside(anchor, rect) {
    const alongV = rect.y < anchor.y + anchor.height && anchor.y < rect.y + rect.height;
    const alongH = rect.x < anchor.x + anchor.width && anchor.x < rect.x + rect.width;
    let side = null;
    if (alongV && rect.x + rect.width === anchor.x)
        side = 'left';
    else if (alongV && rect.x === anchor.x + anchor.width)
        side = 'right';
    else if (alongH && rect.y + rect.height === anchor.y)
        side = 'above';
    else if (alongH && rect.y === anchor.y + anchor.height)
        side = 'below';
    if (!side)
        return null;
    const vertical = VERTICAL_SIDES.includes(side);
    const shift = vertical ? rect.y - anchor.y : rect.x - anchor.x;
    const axis = vertical ? 'y' : 'x';
    const offset = [0, 'start', 'end', -50, 50, -100, 100].find(o =>
        Math.abs(placeBeside(anchor, rect, side, o)[axis] - rect[axis]) <= 1) ?? null;
    return {side, offset, shift};
}

/**
 * The Duo panels' places: the top panel over the bottom panel, centred on
 * each other, from 0,0. Returns the rectangles by connector and the anchors
 * the external screens can be placed against: 'duo' (both panels, only when
 * both are on), 'top' and 'bottom'.
 */
function panelBlock(on) {
    const rects = new Map();
    const anchors = new Map();
    const top = on.find(s => s.panel === 'top');
    const bottom = on.find(s => s.panel === 'bottom');
    const panels = [top, bottom].filter(Boolean);
    const sizes = panels.map(logicalSize);
    const blockWidth = Math.max(0, ...sizes.map(size => size.width));
    let y = 0;
    panels.forEach((panel, i) => {
        const rect = {x: Math.round((blockWidth - sizes[i].width) / 2), y, ...sizes[i]};
        rects.set(panel.connector, rect);
        anchors.set(panel.panel, rect);
        y += sizes[i].height;
    });
    if (panels.length === 2)
        anchors.set('duo', {x: 0, y: 0, width: blockWidth, height: y});
    return {rects, anchors};
}

/**
 * Lays the screens that are on out after a change. The Duo panels stay
 * stacked (top over bottom); each external screen goes where its placement
 * says ({anchor: 'duo'|'top'|'bottom'|connector, side, offset, shift}). One
 * without a placement, against a screen that is off, or that would overlap
 * another screen goes to the first free place beside the others instead and
 * is reported in `moved`. Everything is shifted to start at 0,0, so the
 * result never has gaps or overlaps.
 * @returns {{positions: Map<string, {x, y}>, moved: Set<string>}}
 */
export function layoutScreens(screens) {
    const on = screens.filter(s => s.on);
    const {rects, anchors} = panelBlock(on);
    const moved = new Set();
    const placed = () => [...rects.values()];
    const free = rect => !placed().some(other => overlaps(rect, other));

    let pending = on.filter(s => !s.panel);
    // Screens placed against other external screens wait for those.
    while (pending.length > 0) {
        const ready = pending.find(s => {
            const anchor = s.placement?.anchor;
            return !anchor || anchors.has(anchor) || rects.has(anchor) ||
                !pending.some(p => p.connector === anchor);
        }) ?? pending[0];
        pending = pending.filter(s => s !== ready);
        const size = logicalSize(ready);
        const p = ready.placement;
        const anchor = p && (anchors.get(p.anchor) ?? rects.get(p.anchor));
        let rect = anchor ? placeBeside(anchor, size, p.side, p.offset, p.shift) : null;
        if (!rect || !free(rect)) {
            moved.add(ready.connector);
            rect = firstFreePlace(placed(), anchors, size, free);
        }
        rects.set(ready.connector, rect);
    }

    const all = placed();
    const minX = Math.min(...all.map(r => r.x));
    const minY = Math.min(...all.map(r => r.y));
    const positions = new Map();
    for (const [connector, r] of rects)
        positions.set(connector, {x: r.x - minX, y: r.y - minY});
    return {positions, moved};
}

/** The first place beside the screens already placed that overlaps none. */
function firstFreePlace(placedRects, anchors, size, free) {
    if (placedRects.length === 0)
        return {x: 0, y: 0, width: size.width, height: size.height};
    const candidates = [anchors.get('duo'), ...placedRects].filter(Boolean);
    for (const side of ['right', 'below', 'left', 'above']) {
        for (const anchor of candidates) {
            for (const offset of ['start', 0, 'end']) {
                const rect = placeBeside(anchor, size, side, offset);
                if (free(rect))
                    return rect;
            }
        }
    }
    // Against the rightmost screen, beyond everything: always free.
    const rightmost = placedRects.reduce((a, b) => (b.x + b.width > a.x + a.width ? b : a));
    return placeBeside(rightmost, size, 'right', -100);
}

/**
 * Where each external screen that is on currently sits, as a placement
 * against one screen it shares an edge with. The `remembered` placement (the
 * one last picked) wins while it still describes the screen's place; then
 * the Duo (both panels), the top panel, the bottom panel, other external
 * screens, preferring a place one of the offset chips gives.
 * @returns {Map<string, placement|null>}
 */
export function currentPlacements(screens, remembered = new Map()) {
    const on = screens.filter(s => s.on);
    const rectOf = s => ({x: s.x, y: s.y, ...logicalSize(s)});
    const anchors = [];
    const panels = on.filter(s => s.panel);
    if (panels.length === 2) {
        const r = panels.map(rectOf);
        const x = Math.min(...r.map(a => a.x)), y = Math.min(...r.map(a => a.y));
        anchors.push({id: 'duo', rect: {
            x, y,
            width: Math.max(...r.map(a => a.x + a.width)) - x,
            height: Math.max(...r.map(a => a.y + a.height)) - y,
        }});
    }
    for (const name of ['top', 'bottom']) {
        const panel = panels.find(s => s.panel === name);
        if (panel)
            anchors.push({id: name, rect: rectOf(panel)});
    }
    const externals = on.filter(s => !s.panel);
    for (const s of externals)
        anchors.push({id: s.connector, rect: rectOf(s)});

    const result = new Map();
    for (const s of externals) {
        const rect = rectOf(s);
        const others = anchors.filter(a => a.id !== s.connector);
        const memory = remembered.get(s.connector);
        const known = memory && others.find(a => a.id === memory.anchor);
        if (known) {
            const again = placeBeside(known.rect, rect, memory.side, memory.offset, memory.shift);
            if (again.x === rect.x && again.y === rect.y) {
                result.set(s.connector, memory);
                continue;
            }
        }
        const touching = others
            .map(a => ({id: a.id, beside: placementBeside(a.rect, rect)}))
            .filter(a => a.beside);
        const best = touching.find(a => a.beside.offset !== null) ?? touching[0];
        result.set(s.connector, best ? {anchor: best.id, ...best.beside} : null);
    }
    return result;
}

/** The external screens placed against `connector`, directly or through others. */
export function placedAgainst(connector, placements) {
    const found = new Set();
    let grew = true;
    while (grew) {
        grew = false;
        for (const [other, p] of placements) {
            if (p && !found.has(other) && (p.anchor === connector || found.has(p.anchor))) {
                found.add(other);
                grew = true;
            }
        }
    }
    return found;
}

/**
 * For screens that share their place (mirroring): which screen each one
 * mirrors. The source is the group's Duo panel (the top one first); in a
 * group of external screens only, the one `preferred` names (the choice last
 * made), else the first connector by name.
 * @param screens [{connector, panel, on, sharedWith: [connector]}]
 * @param preferred {Map<string, string>} mirror connector → source picked
 * @returns {Map<string, string>} mirror connector → source connector
 */
export function mirrorSources(screens, preferred = new Map()) {
    const result = new Map();
    for (const s of screens.filter(x => x.on && x.sharedWith?.length > 0)) {
        const group = screens.filter(x => x === s || s.sharedWith.includes(x.connector));
        const picked = group.map(x => preferred.get(x.connector))
            .find(c => c && group.some(x => x.connector === c));
        const source = group.find(x => x.panel === 'top') ?? group.find(x => x.panel) ??
            group.find(x => x.connector === picked) ??
            [...group].sort((a, b) => a.connector.localeCompare(b.connector))[0];
        if (source !== s)
            result.set(s.connector, source.connector);
    }
    return result;
}

/**
 * The mode each screen of a mirror group runs and their shared scale: all
 * must have the same size. Prefers the first mirror's own preferred size
 * (the screen others look at), else the largest size all support; per screen
 * the fastest fixed refresh rate; the scale that mirror prefers when all
 * support it, else 100%.
 * @param group [source, ...mirrors], each {connector, preferredModeId, modes:
 *   [{id, width, height, refresh, preferredScale, supportedScales, variable}]}
 * @returns {{modeIds: Map<string, string>, width, height, scale}|null} null
 *   when they have no size in common
 */
export function mirrorMode(group) {
    const sizeKey = m => `${m.width}x${m.height}`;
    const common = group.map(s => new Set(s.modes.map(sizeKey)))
        .reduce((a, b) => new Set([...a].filter(k => b.has(k))));
    if (common.size === 0)
        return null;
    const viewer = group[1] ?? group[0];
    const viewerPreferred = viewer.modes.find(m => m.id === viewer.preferredModeId);
    const key = viewerPreferred && common.has(sizeKey(viewerPreferred))
        ? sizeKey(viewerPreferred)
        : [...common].sort((a, b) => {
            const area = k => k.split('x').reduce((w, h) => w * h);
            return area(b) - area(a);
        })[0];
    const chosen = group.map(s => s.modes
        .filter(m => sizeKey(m) === key)
        .sort((a, b) => (a.variable - b.variable) || (b.refresh - a.refresh))[0]);
    const supportedByAll = scale => chosen.every(m => m.supportedScales.some(x => Math.abs(x - scale) < 0.01));
    const viewerMode = chosen[group.indexOf(viewer)];
    const scale = supportedByAll(viewerMode.preferredScale) ? viewerMode.preferredScale : 1;
    const [width, height] = key.split('x').map(Number);
    return {modeIds: new Map(group.map((s, i) => [s.connector, chosen[i].id])), width, height, scale};
}

/** Fingers for the touch hold that opens the main-display switch. */
export const HOLD_FINGERS = 5;
/** How far (logical pixels) a finger may move before the hold no longer counts. */
export const HOLD_SLOP = 50;

/**
 * Tracks the fingers on the screens for a still hold of HOLD_FINGERS fingers
 * on one screen (the caller times the hold). `begin`, `move` and `end` return
 * 'armed' when the hold starts, 'cancelled' when it stops counting, else null.
 */
export class FingerHold {
    constructor() {
        this._fingers = new Map();
        this.armed = false;
        this.monitor = -1;
    }

    begin(slot, x, y, monitor) {
        this._fingers.set(slot, {x, y, monitor});
        if (this.armed)
            return this._cancel(); // A sixth finger: not the gesture.
        const all = [...this._fingers.values()];
        if (all.length === HOLD_FINGERS && all.every(f => f.monitor === monitor)) {
            this.armed = true;
            this.monitor = monitor;
            return 'armed';
        }
        return null;
    }

    move(slot, x, y) {
        const finger = this._fingers.get(slot);
        if (!finger || !this.armed)
            return null;
        if (Math.hypot(x - finger.x, y - finger.y) > HOLD_SLOP)
            return this._cancel();
        return null;
    }

    end(slot) {
        this._fingers.delete(slot);
        return this.armed ? this._cancel() : null;
    }

    _cancel() {
        this.armed = false;
        this.monitor = -1;
        return 'cancelled';
    }
}

/**
 * The scales offered for a screen: the supported ones that are whole
 * quarters, plus 5/3 (the Duo panels' 166%) and the current scale.
 */
export function offeredScales(supported, current) {
    const nice = s => Math.abs(s * 4 - Math.round(s * 4)) < 0.01 ||
        Math.abs(s - 5 / 3) < 0.01 || Math.abs(s - current) < 0.01;
    return supported.filter(s => s <= 3.01 && nice(s));
}
