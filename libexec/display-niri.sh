#!/bin/bash
# Niri display backend (niri msg)

_DUO_NIRI_JSON_CACHE=""

function duo-niri-json-outputs() {
    if [ -z "${_DUO_NIRI_JSON_CACHE}" ]; then
        _DUO_NIRI_JSON_CACHE="$(duo-timeout 2s niri msg --json outputs 2>/dev/null)"
    fi
    echo "${_DUO_NIRI_JSON_CACHE}"
}

function duo-niri-invalidate-cache() {
    _DUO_NIRI_JSON_CACHE=""
}

function duo-niri-output-logical-size() {
    local name="$1"
    local py="${PYTHON3:-}"
    if [ -z "${py}" ]; then
        py="$(command -v python3 2>/dev/null || true)"
    fi
    if [ -z "${py}" ]; then
        return 1
    fi
    duo-niri-json-outputs | "${py}" -c '
import json, sys
name = sys.argv[1]
data = json.load(sys.stdin)
for out in data.values() if isinstance(data, dict) else data:
    if out.get("name") == name:
        logical = out.get("logical")
        if logical is None:
            raise SystemExit(1)
        print(f"{int(logical[\"width\"])} {int(logical[\"height\"])}")
        raise SystemExit(0)
raise SystemExit(1)
' "${name}"
}

function duo-niri-transform-token() {
    local orientation="$1"
    case "${orientation}" in
        left)
            echo "90"
            ;;
        right)
            echo "270"
            ;;
        bottom)
            echo "180"
            ;;
        normal|*)
            echo "normal"
            ;;
    esac
}

function duo-display-count() {
    if ! command -v niri >/dev/null 2>&1; then
        echo 0
        return 1
    fi
    local py="${PYTHON3:-}"
    if [ -z "${py}" ]; then
        py="$(command -v python3 2>/dev/null || true)"
    fi
    if [ -z "${py}" ]; then
        echo 0
        return 1
    fi
    duo-niri-invalidate-cache
    duo-niri-json-outputs | "${py}" -c '
import json, sys
data = json.load(sys.stdin)
outputs = data.values() if isinstance(data, dict) else data
print(sum(1 for out in outputs if out.get("current_mode") is not None))
'
}

function duo-display-set-single() {
    duo-niri-invalidate-cache
    duo-timeout 3s niri msg output eDP-1 on
    duo-timeout 3s niri msg output eDP-2 off
}

function duo-display-set-dual-below() {
    duo-niri-invalidate-cache
    duo-timeout 3s niri msg output eDP-1 on
    duo-timeout 3s niri msg output eDP-2 on

    # Wait briefly for outputs to settle before reading sizes
    sleep 0.3
    duo-niri-invalidate-cache

    local w h
    if ! read -r w h < <(duo-niri-output-logical-size "eDP-1"); then
        w=0
        h=0
    fi
    duo-timeout 3s niri msg output eDP-1 position set 0 0
    duo-timeout 3s niri msg output eDP-2 position set 0 "${h}"
}

function duo-display-rotate-single() {
    local orientation="$1"
    local token
    token="$(duo-niri-transform-token "${orientation}")"
    duo-niri-invalidate-cache
    duo-timeout 3s niri msg output eDP-1 transform "${token}"
    duo-timeout 3s niri msg output eDP-1 position set 0 0
}

function duo-display-rotate-dual() {
    local orientation="$1"
    local token
    token="$(duo-niri-transform-token "${orientation}")"

    duo-niri-invalidate-cache

    # Apply transforms first
    duo-timeout 3s niri msg output eDP-1 transform "${token}"
    duo-timeout 3s niri msg output eDP-2 transform "${token}"

    # Wait briefly then read the new logical size after rotation
    sleep 0.3
    duo-niri-invalidate-cache

    local w h
    if ! read -r w h < <(duo-niri-output-logical-size "eDP-1"); then
        w=0
        h=0
    fi

    local pos_x=0
    local pos_y=0

    case "${orientation}" in
        left)
            pos_x="-${w}"
            pos_y="0"
            ;;
        right)
            pos_x="${w}"
            pos_y="0"
            ;;
        bottom)
            pos_x="0"
            pos_y="-${h}"
            ;;
        normal|*)
            pos_x="0"
            pos_y="${h}"
            ;;
    esac

    duo-timeout 3s niri msg output eDP-1 position set 0 0
    duo-timeout 3s niri msg output eDP-2 position set "${pos_x}" "${pos_y}"
}
