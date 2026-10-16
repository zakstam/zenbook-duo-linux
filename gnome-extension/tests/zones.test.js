// Run with: gjs -m gnome-extension/tests/zones.test.js
import {SCREEN_LAYOUTS, SPAN_LAYOUTS, zoneRect, spanRect, hitTest, stackedPair, popupPosition, edgeConstraints, linkedResize, freeRect, restoredUnderPointer, monitorAt, keyboardScreenFor, externalScreens, micVolumeIcon, namedScreens, defaultSwapPair, screenMap, togglePick, isExternal, keyboardScreenForTouch, swipeScreen, logicalSize, offeredScales, placeBeside, placementBeside, layoutScreens, currentPlacements, placedAgainst, mirrorSources, mirrorMode, FingerHold} from '../zenbook-duo-snap@zenbook-duo-linux/zones.js';

let failures = 0;
// Compare by value, independent of object key order (Maps by their entries).
function canonical(value) {
    return JSON.stringify(value, (_key, v) =>
        v instanceof Map ? {Map: [...v]} : v && typeof v === 'object' && !Array.isArray(v)
            ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => a.localeCompare(b)))
            : v);
}

function eq(actual, expected, name) {
    const a = canonical(actual);
    const e = canonical(expected);
    if (a === e) {
        print(`ok   ${name}`);
    } else {
        failures++;
        print(`FAIL ${name}\n     got      ${a}\n     expected ${e}`);
    }
}

// Logical work areas at scale 1.67: top screen below a 32 px top bar, bottom
// screen stacked under it.
const top = {x: 0, y: 32, width: 1728, height: 1048};
const bottom = {x: 0, y: 1080, width: 1728, height: 1080};

eq(zoneRect('full', top), top, 'full fills the work area');
eq(zoneRect('left', top), {x: 0, width: 864, y: 32, height: 1048}, 'left half');
eq(zoneRect('right', top), {x: 864, width: 864, y: 32, height: 1048}, 'right half');
eq(zoneRect('top', bottom), {x: 0, width: 1728, y: 1080, height: 540}, 'top half of the bottom screen');
eq(zoneRect('bottom-right', bottom), {x: 864, width: 864, y: 1620, height: 540}, 'bottom-right corner');

const odd = {x: 10, y: 20, width: 101, height: 51};
const l = zoneRect('left', odd), r = zoneRect('right', odd);
eq([l.width + r.width, r.x], [101, l.x + l.width], 'odd widths: halves cover the area without a gap');

const cells = layouts => layouts.flatMap(layout => layout.rows.flat());
for (const zone of cells(SCREEN_LAYOUTS).filter(z => z !== 'free'))
    zoneRect(zone, top);
eq(cells(SCREEN_LAYOUTS).length, 13, 'every screen zone except free has a rectangle');

const t1 = zoneRect('third-left', odd), t2 = zoneRect('third-middle', odd), t3 = zoneRect('third-right', odd);
eq([t1.width, t2.width, t3.width, t2.x, t3.x], [33, 33, 35, 43, 76], 'thirds cover the width without a gap');

eq(spanRect('span-full', top, bottom), {x: 0, y: 32, width: 1728, height: 2128}, 'span over both screens');
eq(spanRect('span-left', top, bottom), {x: 0, y: 32, width: 864, height: 2128}, 'left half over both screens');
eq(spanRect('span-right', top, bottom), {x: 864, y: 32, width: 864, height: 2128}, 'right half over both screens');
for (const zone of cells(SPAN_LAYOUTS))
    spanRect(zone, top, bottom);
eq(cells(SPAN_LAYOUTS), ['span-full', 'span-left', 'span-right'], 'both screens: full and left/right halves');

const cell = {target: 'top:left', rect: {x: 10, y: 10, width: 20, height: 20}, enabled: true};
const box = {target: 'top', rect: {x: 0, y: 0, width: 100, height: 100}, enabled: true};
const off = {target: 'bottom', rect: {x: 0, y: 100, width: 100, height: 100}, enabled: false};
eq(hitTest([cell, box, off], 15, 15), 'top:left', 'a zone wins over its box');
eq(hitTest([cell, box, off], 50, 50), 'top', 'the box outside its zones');
eq(hitTest([cell, box, off], 50, 150), null, 'a disabled box is never a target');
eq(hitTest([cell, box, off], 100, 50), null, 'right edge is exclusive');

// Nested test session: two stacked screens plus a third one to the right.
const nested = [
    {x: 0, y: 0, width: 1280, height: 800},
    {x: 0, y: 800, width: 1280, height: 800},
    {x: 1280, y: 0, width: 1280, height: 800},
];
eq(stackedPair(nested), {top: 0, bottom: 1}, 'picks the screen directly below, not the one beside');
eq(stackedPair([nested[0], nested[2]]), {top: 0, bottom: -1}, 'side by side: no bottom screen');
eq(stackedPair([]), {top: -1, bottom: -1}, 'no monitors');

const screen = {x: 0, y: 32, width: 1728, height: 1048};
eq(popupPosition(800, 300, 400, 250, screen), {x: 600, y: 332}, 'popup centred below the pointer');
eq(popupPosition(800, 900, 400, 250, screen), {x: 600, y: 618}, 'no room below: above the pointer');
eq(popupPosition(20, 300, 400, 250, screen), {x: 8, y: 332}, 'kept inside the left edge');
eq(popupPosition(1720, 300, 400, 250, screen), {x: 1320, y: 332}, 'kept inside the right edge');

const M = 'monitor', W = 'window';
eq(edgeConstraints(zoneRect('left', top), [top]), {top: M, left: M, right: W, bottom: M},
    'left half: only the middle edge is next to another window');
eq(edgeConstraints(zoneRect('bottom-right', bottom), [bottom]), {top: W, left: W, right: M, bottom: M},
    'bottom-right quarter');
eq(edgeConstraints(spanRect('span-full', top, bottom), [top, bottom]), {top: M, left: M, right: M, bottom: M},
    'stretched over both screens: every edge on a screen edge');
eq(edgeConstraints(spanRect('span-right', top, bottom), [top, bottom]), {top: M, left: W, right: M, bottom: M},
    'right half over both screens');

// Linked resizing (work area of the top screen: x 0..1728, y 32..1080).
const L = zoneRect('left', top), R = zoneRect('right', top);
const narrowerRight = {...R, x: 700, width: 1028};
eq(linkedResize(R, narrowerRight, [{id: 'L', rect: L}]),
    [{id: 'L', rect: {...L, width: 700}}], 'halves: left follows the right half\'s left edge');
eq(linkedResize(L, {...L, width: 1000}, [{id: 'R', rect: R}]),
    [{id: 'R', rect: {...R, x: 1000, width: 728}}], 'halves: right follows the left half\'s right edge');
eq(linkedResize(L, {...L, y: 40}, [{id: 'R', rect: R}]), [], 'an edge without neighbour changes nothing');

const T1 = zoneRect('third-left', top), T2 = zoneRect('third-middle', top), T3 = zoneRect('third-right', top);
eq(linkedResize(T2, {...T2, width: T2.width + 100}, [{id: 'T1', rect: T1}, {id: 'T3', rect: T3}]),
    [{id: 'T3', rect: {...T3, x: T3.x + 100, width: T3.width - 100}}],
    'thirds: only the window at the moved edge follows');

const TL = zoneRect('top-left', top), TR = zoneRect('top-right', top), BR = zoneRect('bottom-right', top);
const BL = zoneRect('bottom-left', top);
const quarters = [{id: 'TR', rect: TR}, {id: 'BL', rect: BL}, {id: 'BR', rect: BR}];
eq(linkedResize(TL, {...TL, width: TL.width + 50}, quarters),
    [{id: 'TR', rect: {...TR, x: TR.x + 50, width: TR.width - 50}},
        {id: 'BL', rect: {...BL, width: BL.width + 50}},
        {id: 'BR', rect: {...BR, x: BR.x + 50, width: BR.width - 50}}],
    'quarters: the whole vertical middle line moves');
eq(linkedResize(TL, {...TL, height: TL.height - 40}, quarters),
    [{id: 'TR', rect: {...TR, height: TR.height - 40}},
        {id: 'BL', rect: {...BL, y: BL.y - 40, height: BL.height + 40}},
        {id: 'BR', rect: {...BR, y: BR.y - 40, height: BR.height + 40}}],
    'quarters: the whole horizontal middle line moves');
eq(linkedResize(L, {...L, width: L.width + 20}, [{id: 'TR', rect: TR}, {id: 'BR', rect: BR}]),
    [{id: 'TR', rect: {...TR, x: TR.x + 20, width: TR.width - 20}},
        {id: 'BR', rect: {...BR, x: BR.x + 20, width: BR.width - 20}}],
    'left half next to two right quarters: both follow');

const TOP = zoneRect('top', top), BOT = zoneRect('bottom', top);
eq(linkedResize(TOP, {...TOP, height: TOP.height + 60}, [{id: 'B', rect: BOT}]),
    [{id: 'B', rect: {...BOT, y: BOT.y + 60, height: BOT.height - 60}}], 'stacked halves follow vertically');

eq(linkedResize(L, {...L, width: 1700}, [{id: 'R', rect: R}]),
    [{id: 'R', rect: {...R, x: 1728 - 120, width: 120}}], 'a neighbour is not squeezed below the minimum');

eq(freeRect({width: 800, height: 600}, top), {x: 464, y: 256, width: 800, height: 600},
    'free: the remembered size, centred');
eq(freeRect({width: 1728, height: 2128}, top), {x: 0, y: 32, width: 1728, height: 1048},
    'free: a stretched window is capped to the screen');
eq(restoredUnderPointer({x: 0, y: 32, width: 1728, height: 1048}, {width: 864, height: 600}, 576),
    {x: 288, y: 32, width: 864, height: 600},
    'drag away: the pointer stays at the same relative spot (a third)');

const duo = [{x: 0, y: 0, width: 1728, height: 1080}, {x: 0, y: 1080, width: 1728, height: 1080}];
eq(monitorAt(duo, 500, 1500), 1, 'a finger on the bottom screen is on the bottom screen');
eq(monitorAt(duo, 500, 1079.5), 0, 'just above the seam is the top screen');
eq(monitorAt(duo, 2000, 1500), 1, 'outside every screen: the closest one');
eq(monitorAt([], 0, 0), -1, 'no monitors');

const screens2 = {top: 1, bottom: 0};
eq(keyboardScreenFor(screens2, 1, 500, 1700), 0, 'field on the top screen: keyboard on the bottom screen');
eq(keyboardScreenFor(screens2, -1, 0, 1700), 0, 'unknown field position: keyboard on the bottom screen');
eq(keyboardScreenFor(screens2, 0, 1300, 1700), 0, 'field high on the bottom screen: keyboard stays on the bottom');
eq(keyboardScreenFor(screens2, 0, 1900, 1700), 1, 'field the keyboard would cover: keyboard on the top screen');
eq(keyboardScreenFor({top: 1, bottom: -1}, 1, 500, 700), -1, 'one screen on: left to GNOME');

const withExternals = [
    {x: 0, y: 0, width: 1728, height: 1080},       // bottom panel
    {x: 3648, y: 0, width: 1920, height: 1080},    // external, far right
    {x: 0, y: 1080, width: 1728, height: 1080},    // top panel
    {x: 1728, y: 0, width: 1920, height: 1080},    // external, next to the panels
];
eq(externalScreens(withExternals, {top: 2, bottom: 0}), [3, 1], 'external screens, left to right');
eq(externalScreens(duo, {top: 1, bottom: 0}), [], 'no external screens');
const many = Array.from({length: 6}, (_, i) => ({x: i * 100, y: 0, width: 100, height: 100}));
eq(externalScreens(many, {top: 0, bottom: 1}), [2, 3, 4], 'capped at three');

eq(micVolumeIcon(0.8, false), 'microphone-sensitivity-high-symbolic', 'loud microphone');
eq(micVolumeIcon(0.5, false), 'microphone-sensitivity-medium-symbolic', 'medium microphone');
eq(micVolumeIcon(0.1, false), 'microphone-sensitivity-low-symbolic', 'quiet microphone');
eq(micVolumeIcon(0, false), 'microphone-sensitivity-muted-symbolic', 'microphone at zero');
eq(micVolumeIcon(0.8, true), 'microphone-sensitivity-muted-symbolic', 'muted microphone');

// Swap dialog: bottom panel is monitor 0, top 2, externals 3 and 1 (see withExternals).
const named = namedScreens(withExternals, {top: 2, bottom: 0});
eq(named.map(s => [s.index, s.name]), [[2, 'Top screen'], [0, 'Bottom screen'], [3, 'External 1'], [1, 'External 2']],
    'screens named: panels first, then externals left to right');
eq(defaultSwapPair(named, null), [2, 0], 'swap starts with the two panels');
eq(defaultSwapPair(named, ['External 2', 'Top screen']), [1, 2], 'then with the last pair used');
eq(defaultSwapPair(named, ['External 3', 'Top screen']), [2, 0], 'unless one of it is gone');
const topAndExternal = [{x: 0, y: 0, width: 1728, height: 1080}, {x: 1728, y: 0, width: 1920, height: 1080}];
eq(defaultSwapPair(namedScreens(topAndExternal, {top: 0, bottom: -1}), null), [0, 1],
    'bottom panel off: top screen and the external');
// Two stacked 1728x1080 panels in a 100x200 box: 100x62.5 each, centred.
eq(screenMap(duo, 100, 200, 0), [{x: 0, y: 38, width: 100, height: 63}, {x: 0, y: 100, width: 100, height: 63}],
    'screen map: stacked panels, centred vertically');
eq(screenMap([], 100, 100), [], 'screen map without screens');
eq(togglePick([], 3), [3], 'pick one');
eq(togglePick([3], 1), [3, 1], 'pick two');
eq(togglePick([3, 1], 0), [1, 0], 'a third pick replaces the older');
eq(togglePick([3, 1], 3), [1], 'picking again unpicks');

// Touch keyboard with a touch monitor: panels 1 (top) and 0 (bottom), external 2.
const panels2 = {top: 1, bottom: 0};
eq([isExternal(panels2, 2), isExternal(panels2, 0), isExternal(panels2, -1)], [true, false, false], 'external screens');
eq(keyboardScreenForTouch(panels2, 2, 2, 500, 1700), 2, 'touched on the external screen: keyboard there');
eq(keyboardScreenForTouch(panels2, 1, 1, 500, 1700), 0, 'touched on the top panel: bottom panel as before');
eq(keyboardScreenForTouch(panels2, -1, 0, 1900, 1700), 1, 'no touch known: the panel rules');
eq(swipeScreen(panels2, 2), 2, 'swipe on the external screen opens there');
eq(swipeScreen(panels2, 0), 0, 'swipe on the bottom panel opens there');
eq(swipeScreen(panels2, 1), -1, 'swipe at the seam (top panel) is ignored');
eq(swipeScreen({top: 1, bottom: -1}, 1), 1, 'only the top panel on: its swipe counts');
eq(swipeScreen({top: 1, bottom: -1}, 2), 2, 'top panel and an external: external swipe opens there');

// Display controls.
const panelScreen = (connector, panel, on = true) => ({connector, builtin: true, panel, on,
    modeWidth: 2880, modeHeight: 1800, scale: 5 / 3, transform: 0});
const monitorScreen = (connector, placement, extra = {}) => ({connector, builtin: false, panel: null, on: true,
    modeWidth: 1920, modeHeight: 1080, scale: 1, transform: 0, placement, ...extra});
const at = (anchor, side, offset, shift = 0) => ({anchor, side, offset, shift});
const placesOf = positions => Object.fromEntries([...positions].map(([c, p]) => [c, [p.x, p.y]]));
eq(logicalSize(panelScreen('eDP-1', 'top')), {width: 1728, height: 1080}, 'panel at 166%');
eq(logicalSize(monitorScreen('DP-2', null, {transform: 1})), {width: 1080, height: 1920}, 'rotated monitor');
const duoBlock = {x: 0, y: 0, width: 1728, height: 2160};
const fullHd = {width: 1920, height: 1080};
eq([placeBeside(duoBlock, fullHd, 'right', 0), placeBeside(duoBlock, fullHd, 'right', 'start'), placeBeside(duoBlock, fullHd, 'right', 'end')],
    [{x: 1728, y: 540, ...fullHd}, {x: 1728, y: 0, ...fullHd}, {x: 1728, y: 1080, ...fullHd}],
    'right of the Duo: centred, top edges, bottom edges');
// 10% of the shorter edge (1080) stays shared: 108 px.
eq([placeBeside(duoBlock, fullHd, 'right', -100), placeBeside(duoBlock, fullHd, 'right', 100), placeBeside(duoBlock, fullHd, 'right', 50)],
    [{x: 1728, y: -972, ...fullHd}, {x: 1728, y: 2052, ...fullHd}, {x: 1728, y: 1296, ...fullHd}],
    'right of the Duo: as far up and down as GNOME allows, keeping 10% shared; 50 halfway');
eq(placeBeside(duoBlock, fullHd, 'below', 'start'), {x: 0, y: 2160, ...fullHd}, 'below the Duo, left edges lined up');
eq(placeBeside(duoBlock, fullHd, 'right', null, 300), {x: 1728, y: 300, ...fullHd}, 'custom place: shift in pixels');
eq(placementBeside(duoBlock, {x: 1728, y: 540, ...fullHd}), {side: 'right', offset: 0, shift: 540}, 'read back: centred right');
eq(placementBeside(duoBlock, {x: 1728, y: 300, ...fullHd}), {side: 'right', offset: null, shift: 300}, 'read back: in between is custom');
eq(placementBeside(duoBlock, {x: 1728, y: 2160, ...fullHd}), null, 'touching only at a corner is no placement');
eq(placementBeside(duoBlock, {x: 1728, y: 2052, ...fullHd}), {side: 'right', offset: 100, shift: 2052}, 'read back: furthest down');
const duoPanels = () => [panelScreen('eDP-1', 'top'), panelScreen('eDP-2', 'bottom')];
const layoutOf = screens => {
    const {positions, moved} = layoutScreens(screens);
    return {places: placesOf(positions), moved: [...moved]};
};
eq(layoutOf([...duoPanels(), monitorScreen('DP-2', at('duo', 'right', 0))]),
    {places: {'eDP-1': [0, 0], 'eDP-2': [0, 1080], 'DP-2': [1728, 540]}, moved: []}, 'monitor centred right of the Duo');
eq(layoutOf([...duoPanels(), monitorScreen('DP-2', at('duo', 'left', 'start'), {transform: 1})]),
    {places: {'eDP-1': [1080, 0], 'eDP-2': [1080, 1080], 'DP-2': [0, 0]}, moved: []}, 'rotated monitor left of the Duo, shifted to 0,0');
eq(layoutOf([...duoPanels(), monitorScreen('DP-2', at('top', 'left', 0))]).places['DP-2'], [0, 0],
    'left of the top screen');
eq(layoutOf([...duoPanels(), monitorScreen('DP-2', at('duo', 'below', 'start')), monitorScreen('HDMI-1', at('DP-2', 'left', 0))]),
    {places: {'eDP-1': [1920, 0], 'eDP-2': [1920, 1080], 'DP-2': [1920, 2160], 'HDMI-1': [0, 2160]}, moved: []},
    'second monitor left of the first, which is below the Duo');
eq(layoutOf([...duoPanels(), monitorScreen('DP-2', at('duo', 'right', 'end')), monitorScreen('HDMI-1', at('DP-2', 'right', 0))]).places['HDMI-1'],
    [3648, 1080], 'a monitor placed against another follows it');
eq(layoutOf([monitorScreen('HDMI-1', at('DP-2', 'right', 0)), ...duoPanels(), monitorScreen('DP-2', at('duo', 'right', 0))]).moved, [],
    'placed after the monitor it is against, whatever the order');
eq(layoutOf([...duoPanels(), monitorScreen('DP-2', at('duo', 'right', 0)), monitorScreen('HDMI-1', at('duo', 'right', 0))]),
    {places: {'eDP-1': [0, 0], 'eDP-2': [0, 1080], 'DP-2': [1728, 540], 'HDMI-1': [3648, 540]}, moved: ['HDMI-1']},
    'overlapping place: moved to the first free place');
eq(layoutOf([panelScreen('eDP-1', 'top'), panelScreen('eDP-2', 'bottom', false), monitorScreen('DP-2', at('bottom', 'below', 0))]),
    {places: {'eDP-1': [0, 0], 'DP-2': [1728, 0]}, moved: ['DP-2']}, 'placed against a screen that is off: moved');
eq(layoutOf([panelScreen('eDP-1', 'top', false), panelScreen('eDP-2', 'bottom', false), monitorScreen('DP-2', null), monitorScreen('HDMI-1', null)]).places,
    {'DP-2': [0, 0], 'HDMI-1': [1920, 0]}, 'external only, no placements: in a row');
const asPlaced = (screen, x, y) => ({...screen, x, y});
const placedScreens = [asPlaced(panelScreen('eDP-1', 'top'), 1920, 0), asPlaced(panelScreen('eDP-2', 'bottom'), 1920, 1080),
    asPlaced(monitorScreen('DP-2', null), 1920, 2160), asPlaced(monitorScreen('HDMI-1', null), 0, 2160)];
eq(Object.fromEntries(currentPlacements(placedScreens)),
    {'DP-2': at('duo', 'below', 'start'), 'HDMI-1': at('DP-2', 'left', 0)}, 'placements read from the screens');
const rightOfTop = [asPlaced(panelScreen('eDP-1', 'top'), 0, 0), asPlaced(panelScreen('eDP-2', 'bottom'), 0, 1080),
    asPlaced(monitorScreen('DP-2', null), 1728, 0)];
eq(currentPlacements(rightOfTop).get('DP-2'), at('duo', 'right', 'start', 0), 'read as the Duo first');
eq(currentPlacements(rightOfTop, new Map([['DP-2', at('top', 'right', 0)]])).get('DP-2'), at('top', 'right', 0),
    'the placement last picked wins while it still fits');
eq([...placedAgainst('DP-2', new Map([['DP-2', at('duo', 'right', 0)], ['HDMI-1', at('DP-2', 'left', 0)], ['DP-3', at('HDMI-1', 'below', 0)]]))],
    ['HDMI-1', 'DP-3'], 'screens placed against a screen, also through others');
eq(Object.fromEntries(mirrorSources([
    {connector: 'eDP-1', panel: 'top', on: true, sharedWith: ['HDMI-1', 'DP-2']},
    {connector: 'eDP-2', panel: 'bottom', on: true, sharedWith: []},
    {connector: 'HDMI-1', panel: null, on: true, sharedWith: ['eDP-1', 'DP-2']},
    {connector: 'DP-2', panel: null, on: true, sharedWith: ['eDP-1', 'HDMI-1']},
])), {'HDMI-1': 'eDP-1', 'DP-2': 'eDP-1'}, 'mirror group: the panel is the source');
const externalPair = [{connector: 'DP-2', panel: null, on: true, sharedWith: ['HDMI-1']},
    {connector: 'HDMI-1', panel: null, on: true, sharedWith: ['DP-2']}];
eq(Object.fromEntries(mirrorSources(externalPair)), {'HDMI-1': 'DP-2'}, 'two external screens: first by name');
eq(Object.fromEntries(mirrorSources(externalPair, new Map([['DP-2', 'HDMI-1']]))), {'DP-2': 'HDMI-1'},
    'two external screens: the direction picked last');
const mode = (id, width, height, refresh, preferredScale = 1, supportedScales = [1, 1.25, 1.5, 2], variable = false) =>
    ({id, width, height, refresh, preferredScale, supportedScales, variable});
const panelModes = {connector: 'eDP-1', preferredModeId: 'p0', modes: [
    mode('p0', 2880, 1800, 120, 5 / 3), mode('p1', 1920, 1200, 120, 1.25), mode('p1v', 1920, 1200, 120, 1.25, undefined, true),
    mode('p2', 1920, 1080, 60, 1.25), mode('p3', 1920, 1080, 120, 1.25)]};
const tv = {connector: 'HDMI-1', preferredModeId: 't0', modes: [mode('t0', 1920, 1080, 60), mode('t1', 1280, 720, 60)]};
eq(mirrorMode([panelModes, tv]), {modeIds: new Map([['eDP-1', 'p3'], ['HDMI-1', 't0']]), width: 1920, height: 1080, scale: 1},
    'mirror: the external screen\'s size, fastest fixed refresh on the panel, its scale');
const wuxga = {connector: 'DP-2', preferredModeId: 'w9', modes: [mode('w9', 3840, 2160, 60, 2), mode('w1', 1920, 1200, 60), mode('w2', 1920, 1080, 60)]};
eq(mirrorMode([panelModes, wuxga]).modeIds, new Map([['eDP-1', 'p1'], ['DP-2', 'w1']]),
    'mirror: preferred size not on the panel, the largest common one');
eq(mirrorMode([panelModes, {connector: 'X', preferredModeId: 'x', modes: [mode('x', 800, 600, 60)]}]), null, 'mirror: no common size');
{
    const hold = new FingerHold();
    const steps = [1, 2, 3, 4].map(i => hold.begin(i, i * 100, 500, 1));
    steps.push(hold.begin(5, 500, 500, 1));
    eq(steps, [null, null, null, null, 'armed'], 'five fingers on one screen arm the hold');
    eq(hold.monitor, 1, 'on that screen');
    eq(hold.move(3, 310, 505), null, 'a small wobble still counts');
    eq(hold.move(3, 400, 500), 'cancelled', 'moving a finger cancels it');
    const lifted = new FingerHold();
    [1, 2, 3, 4, 5].forEach(i => lifted.begin(i, i * 100, 500, 0));
    eq(lifted.end(2), 'cancelled', 'lifting a finger cancels it');
    const split = new FingerHold();
    eq([1, 2, 3, 4, 5].map(i => split.begin(i, i * 100, 500, i < 3 ? 0 : 1)), [null, null, null, null, null],
        'fingers on two screens do not count');
    const six = new FingerHold();
    [1, 2, 3, 4, 5].forEach(i => six.begin(i, i * 100, 500, 0));
    eq(six.begin(6, 600, 500, 0), 'cancelled', 'a sixth finger cancels it');
}
eq(offeredScales([1, 1.25, 1.3333333730697632, 1.5, 1.6666666269302368, 2, 2.25, 2.5, 3, 3.3333332538604736], 1.6666666269302368),
    [1, 1.25, 1.5, 1.6666666269302368, 2, 2.25, 2.5, 3], 'offered scales: quarters and 5/3, up to 300%');

if (failures) {
    print(`${failures} failed`);
    imports.system.exit(1);
}
print('all passed');
