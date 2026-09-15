#!/usr/bin/env bash
set -euo pipefail

npm run tauri:build -- --bundles deb

deb_path="$(find src-tauri/target/release/bundle/deb -maxdepth 1 -type f -name '*.deb' -print -quit)"
test -n "$deb_path"
package_root="$(mktemp -d)"
rebuilt_deb="${deb_path}.rebuilt"
cleanup() {
  rm -rf -- "$package_root"
  rm -f -- "$rebuilt_deb"
}
trap cleanup EXIT

dpkg-deb -R "$deb_path" "$package_root"
install -m 0755 src-tauri/debian/postinst "$package_root/DEBIAN/postinst"
dpkg-deb -b "$package_root" "$rebuilt_deb"
mv -f -- "$rebuilt_deb" "$deb_path"
