#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Execute on the frame. No persistent service, kernel or browser-profile changes.
set -eu
mode=${1:---check}
stage=${2:-}
sync_url=${3:-http://127.0.0.1:8877/sync}
seconds=${4:-60}
case "$mode" in --check|--run) ;; *) echo 'Use --check or --run STAGING_DIR [SYNC_URL] [SECONDS]' >&2; exit 2;; esac
case "$stage" in /tmp/otrios-dashboard-*) ;; *) echo 'Use a dedicated /tmp/otrios-dashboard-* staging folder' >&2; exit 2;; esac
case "$seconds" in ''|*[!0-9]*) exit 2;; esac
[ "$seconds" -ge 1 ] && [ "$seconds" -le 300 ]
[ "$(id -u)" -eq 0 ]
[ -f "$stage/dashboard-native" ] && [ -x "$stage/dashboard-native" ]
[ -S /run/spd-kiosk/wayland-0 ]
systemctl is-active --quiet spd-weston.service
systemctl is-active --quiet spd-browser.service
ldd_output=$(LD_LIBRARY_PATH="$stage/lib" ldd "$stage/dashboard-native")
if printf '%s\n' "$ldd_output" | grep -q 'not found'; then printf '%s\n' "$ldd_output"; exit 1; fi
printf 'Qualified native GLES2 trial: %ss; browser automatically restored afterwards.\n' "$seconds"
[ "$mode" = --run ] || exit 0
unit="otrios-dashboard-trial-$(date +%s)"
printf 'Trial unit: %s; capture: /tmp/%s.ppm\n' "$unit" "$unit"
# The service manager restores the existing kiosk even after crashes or timeout.
systemd-run --wait --collect --unit="$unit" \
  --property=Type=exec --property="RuntimeMaxSec=$((seconds+15))s" \
  --property='ExecStartPre=/usr/bin/systemctl stop spd-browser.service' \
  --property='ExecStopPost=/usr/bin/systemctl start spd-browser.service' \
  /usr/sbin/runuser -u kiosk -- /usr/bin/env \
  XDG_RUNTIME_DIR=/run/spd-kiosk WAYLAND_DISPLAY=wayland-0 \
  SDL_VIDEODRIVER=wayland SDL_RENDER_DRIVER=opengles2 LD_LIBRARY_PATH="$stage/lib" \
  "$stage/dashboard-native" --fullscreen --seconds "$seconds" \
  --sync "$sync_url" --capture "/tmp/$unit.ppm"
systemctl is-active spd-browser.service
journalctl -u "$unit" --no-pager -o cat
