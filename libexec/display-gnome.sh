#!/bin/bash
# GNOME display backend (gdctl/mutter)

function duo-display-count() {
    if ! command -v gdctl >/dev/null 2>&1; then
        echo 0
        return 1
    fi
    if ! duo-has-graphical-session; then
        echo 0
        return 0
    fi
    duo-timeout 2s gdctl show | grep 'Logical monitor #' | wc -l
}

function duo-display-set-single() {
    duo-timeout 3s gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1
}

function duo-display-set-dual-below() {
    duo-timeout 3s gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 \
        --logical-monitor --scale ${SCALE} --monitor eDP-2 --below eDP-1
}

function duo-display-rotate-single() {
    local orientation="$1"
    case "${orientation}" in
        left)
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 90
            ;;
        right)
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 270
            ;;
        bottom)
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 180
            ;;
        normal|*)
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1
            ;;
    esac
}

function duo-display-rotate-dual() {
    local orientation="$1"
    case "${orientation}" in
        left)
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 90 \
                --logical-monitor --scale ${SCALE} --monitor eDP-2 --left-of eDP-1 --transform 90
            ;;
        right)
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 270 \
                --logical-monitor --scale ${SCALE} --monitor eDP-2 --right-of eDP-1 --transform 270
            ;;
        bottom)
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 --transform 180 \
                --logical-monitor --scale ${SCALE} --monitor eDP-2 --above eDP-1 --transform 180
            ;;
        normal|*)
            gdctl set --logical-monitor --primary --scale ${SCALE} --monitor eDP-1 \
                --logical-monitor --scale ${SCALE} --monitor eDP-2 --below eDP-1
            ;;
    esac
}
