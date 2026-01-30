#!/bin/bash
# KDE display backend (kscreen-doctor)

function duo-kde-json() {
    kscreen-doctor -j 2>/dev/null
}

function duo-kde-output-logical-size() {
    local name="$1"
    local py="${PYTHON3:-}"
    if [ -z "${py}" ]; then
        py="$(command -v python3 2>/dev/null || true)"
    fi
    if [ -z "${py}" ]; then
        return 1
    fi
    # Pipe JSON into Python; use -c so stdin remains the JSON stream.
    duo-kde-json | "${py}" -c '
import json, sys
name = sys.argv[1]
data = json.load(sys.stdin)
for out in data.get("outputs", []):
    if out.get("name") == name:
        size = out.get("size") or {}
        scale = out.get("scale") or 1
        w = size.get("width", 0)
        h = size.get("height", 0)
        if scale:
            w = round(w / scale)
            h = round(h / scale)
        print(f"{int(w)} {int(h)}")
        raise SystemExit(0)
raise SystemExit(1)
' "${name}"
}

function duo-display-count() {
    if ! command -v kscreen-doctor >/dev/null 2>&1; then
        echo 0
        return 1
    fi
    if ! duo-has-graphical-session; then
        echo 0
        return 0
    fi
    local py="${PYTHON3:-}"
    if [ -z "${py}" ]; then
        py="$(command -v python3 2>/dev/null || true)"
    fi
    if [ -z "${py}" ]; then
        echo 0
        return 1
    fi
    duo-kde-json | "${py}" -c '
import json, sys
data = json.load(sys.stdin)
print(sum(1 for out in data.get("outputs", []) if out.get("enabled")))
'
}

function duo-kde-rotation-token() {
    local orientation="$1"
    case "${orientation}" in
        left)
            echo "right"
            ;;
        right)
            echo "left"
            ;;
        bottom)
            echo "inverted"
            ;;
        normal|*)
            echo "none"
            ;;
    esac
}

function duo-display-set-single() {
    duo-timeout 3s kscreen-doctor output.eDP-1.enable output.eDP-2.disable
}

function duo-display-set-dual-below() {
    local w h
    if ! read -r w h < <(duo-kde-output-logical-size "eDP-1"); then
        w=0
        h=0
    fi
    duo-timeout 3s kscreen-doctor \
        output.eDP-1.enable output.eDP-2.enable \
        output.eDP-1.position.0,0 output.eDP-2.position.0,${h}
}

function duo-display-rotate-single() {
    local orientation="$1"
    local token
    token="$(duo-kde-rotation-token "${orientation}")"
    duo-timeout 3s kscreen-doctor output.eDP-1.enable output.eDP-1.position.0,0 output.eDP-1.rotation.${token}
}

function duo-display-rotate-dual() {
    local orientation="$1"
    local token
    token="$(duo-kde-rotation-token "${orientation}")"

    local w h
    if ! read -r w h < <(duo-kde-output-logical-size "eDP-1"); then
        w=0
        h=0
    fi

    local pos_x=0
    local pos_y=0
    local rot_w="${w}"
    local rot_h="${h}"
    if [ "${orientation}" = "left" ] || [ "${orientation}" = "right" ]; then
        rot_w="${h}"
        rot_h="${w}"
    fi

    case "${orientation}" in
        left)
            pos_x="-${rot_w}"
            pos_y="0"
            ;;
        right)
            pos_x="${rot_w}"
            pos_y="0"
            ;;
        bottom)
            pos_x="0"
            pos_y="-${rot_h}"
            ;;
        normal|*)
            pos_x="0"
            pos_y="${rot_h}"
            ;;
    esac

    duo-timeout 3s kscreen-doctor \
        output.eDP-1.enable output.eDP-2.enable \
        output.eDP-1.rotation.${token} output.eDP-2.rotation.${token} \
        output.eDP-1.position.0,0 output.eDP-2.position.${pos_x},${pos_y}
}
