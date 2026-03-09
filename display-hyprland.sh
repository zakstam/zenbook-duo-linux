#!/bin/bash
# Hyprland display backend (hyprctl)

function duo-display-count() {
    if ! command -v hyprctl >/dev/null 2>&1; then
        echo 0
        return 1
    fi
    hyprctl -j monitors | jq '. | length'
}

function duo-hyprland-transform-token() {
    # 0=normal, 1=90, 2=180, 3=270
    case "$1" in
        left) echo "1" ;;
        right) echo "3" ;;
        bottom) echo "2" ;;
        normal|*) echo "0" ;;
    esac
}

function duo-display-set-single() {
    # Active eDP-1 et désactive eDP-2
    duo-timeout 3s hyprctl keyword monitor eDP-1,preferred,auto,1
    duo-timeout 3s hyprctl keyword monitor eDP-2,disable
}

function duo-display-set-dual-below() {
    # Récupérer la hauteur de eDP-1 pour placer eDP-2 en dessous
    local h=$(hyprctl -j monitors | jq -r '.[] | select(.name=="eDP-1") | .height' 2>/dev/null)
    local scale=$(hyprctl -j monitors | jq -r '.[] | select(.name=="eDP-1") | .scale' 2>/dev/null)
    
    # Calcul de la résolution logique (hauteur / scale)
    local logical_h=$(echo "$h / $scale" | bc)
    [ -z "$logical_h" ] && logical_h=1800

    duo-timeout 3s hyprctl keyword monitor eDP-1,preferred,0x0,1
    duo-timeout 3s hyprctl keyword monitor eDP-2,preferred,0x${logical_h},1
}

function duo-display-rotate-single() {
    local token="$(duo-hyprland-transform-token "$1")"
    duo-timeout 3s hyprctl keyword monitor eDP-1,preferred,auto,1,transform,"${token}"
    duo-timeout 3s hyprctl keyword monitor eDP-2,disable
}

function duo-display-rotate-dual() {
    local token="$(duo-hyprland-transform-token "$1")"
    
    local w=$(hyprctl -j monitors | jq -r '.[] | select(.name=="eDP-1") | .width' 2>/dev/null)
    local h=$(hyprctl -j monitors | jq -r '.[] | select(.name=="eDP-1") | .height' 2>/dev/null)
    local scale=$(hyprctl -j monitors | jq -r '.[] | select(.name=="eDP-1") | .scale' 2>/dev/null)
    
    local logical_w=$(echo "$w / $scale" | bc)
    local logical_h=$(echo "$h / $scale" | bc)

    duo-timeout 3s hyprctl keyword monitor eDP-1,preferred,0x0,1,transform,"${token}"

    # Calcul de l'emplacement du second moniteur selon la rotation
    local pos_x=0
    local pos_y=0

    case "$1" in
        left) pos_x="-${logical_h}"; pos_y="0" ;;
        right) pos_x="${logical_h}"; pos_y="0" ;;
        bottom) pos_x="0"; pos_y="-${logical_h}" ;;
        normal|*) pos_x="0"; pos_y="${logical_h}" ;;
    esac

    duo-timeout 3s hyprctl keyword monitor eDP-2,preferred,${pos_x}x${pos_y},1,transform,"${token}"
}
