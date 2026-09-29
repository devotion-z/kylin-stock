#!/usr/bin/env bash
set -u

section() {
  printf '\n===== %s =====\n' "$1"
}

command_value() {
  local label="$1"
  shift
  printf '%-24s' "$label"
  if "$@" >/tmp/kylin-stock-doctor.out 2>/tmp/kylin-stock-doctor.err; then
    head -n 1 /tmp/kylin-stock-doctor.out
  else
    printf 'NOT AVAILABLE'
    if [ -s /tmp/kylin-stock-doctor.err ]; then
      printf ' (%s)' "$(head -n 1 /tmp/kylin-stock-doctor.err)"
    fi
    printf '\n'
  fi
}

section "KylinStock Target Environment"
printf 'Collected at: %s\n' "$(date '+%Y-%m-%d %H:%M:%S %z')"
printf 'Hostname: %s\n' "$(hostname 2>/dev/null || true)"
printf 'Architecture: %s\n' "$(uname -m)"
printf 'Kernel: %s\n' "$(uname -r)"

if [ "$(uname -m)" = "aarch64" ]; then
  echo 'ARM64 target: PASS'
else
  echo 'ARM64 target: WARNING - expected aarch64 for the customer Phytium D2000 machine'
fi

section "Operating System"
if [ -r /etc/os-release ]; then
  grep -E '^(NAME|VERSION|VERSION_ID|ID|PRETTY_NAME|KYLIN_RELEASE_ID)=' /etc/os-release || cat /etc/os-release
else
  echo '/etc/os-release not readable'
fi

section "CPU / Memory"
if command -v lscpu >/dev/null 2>&1; then
  lscpu | grep -E '^(Architecture|CPU\(s\)|Model name|Vendor ID|Thread|Core|Socket|Byte Order):' || true
fi
if command -v free >/dev/null 2>&1; then
  free -h
fi

section "Runtime ABI"
command_value 'glibc' sh -c 'ldd --version | head -n 1'
command_value 'pkg-config' pkg-config --version
command_value 'WebKitGTK 4.1' pkg-config --modversion webkit2gtk-4.1
command_value 'WebKitGTK 4.0' pkg-config --modversion webkit2gtk-4.0
command_value 'GTK 3' pkg-config --modversion gtk+-3.0
command_value 'OpenSSL' pkg-config --modversion openssl
command_value 'Ayatana AppIndicator' pkg-config --modversion ayatana-appindicator3-0.1

section "Document OCR"
command_value 'Tesseract OCR' tesseract --version
if command -v tesseract >/dev/null 2>&1 && tesseract --list-langs 2>&1 | grep -Fxq chi_sim; then
  echo 'Simplified Chinese OCR: PASS'
else
  echo 'Simplified Chinese OCR: WARNING - chi_sim language data is unavailable'
fi

section "Installed WebKit / GTK Packages"
if command -v dpkg >/dev/null 2>&1; then
  dpkg -l 2>/dev/null | grep -E 'webkit2gtk|libgtk-3|appindicator|librsvg' || true
else
  echo 'dpkg not available'
fi

section "Optional Build Toolchain"
command_value 'Node.js' node --version
command_value 'npm' npm --version
command_value 'rustc' rustc --version
command_value 'cargo' cargo --version
command_value 'gcc' gcc --version
command_value 'make' make --version
command_value 'dpkg-deb' dpkg-deb --version

section "Desktop Session"
printf 'XDG_CURRENT_DESKTOP: %s\n' "${XDG_CURRENT_DESKTOP:-unknown}"
printf 'XDG_SESSION_TYPE: %s\n' "${XDG_SESSION_TYPE:-unknown}"
printf 'LANG: %s\n' "${LANG:-unknown}"

rm -f /tmp/kylin-stock-doctor.out /tmp/kylin-stock-doctor.err

echo
echo 'Doctor finished. Save the complete terminal output for the KylinStock compatibility record.'
