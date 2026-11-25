#!/usr/bin/env bash
# Installs the scoreboard's competitors at pinned versions, outside the repository.
# Safe to run again: a tool already at its pinned version is left as it is.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
cache=${VT_SCOREBOARD_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/vt-scoreboard}
mkdir -p "$cache/bin" "$cache/node"

mermaid_ascii_version=1.6.2
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) asset=Darwin_arm64 sha256=ee46ce2f41f83ef59830c541c23b99a7937cea278b7502218d079b973fd2ac1b ;;
  Darwin-x86_64) asset=Darwin_x86_64 sha256=947c7aae051819b499d20ac109473936cd96fccd51244f1d851bd6161e463f10 ;;
  Linux-aarch64) asset=Linux_arm64 sha256=121bc3594f08cf41be2d0c9ec31b4103d81b21852290e2817b5c536f66a59446 ;;
  Linux-x86_64) asset=Linux_x86_64 sha256=e85222d5cedb39cfc346af02a0b68d2c90663e47d6512cc042fa7eb866bf2a30 ;;
  *) asset="" ;;
esac

install_mermaid_ascii() {
  if [ -z "$asset" ]; then
    echo "mermaid-ascii: no pinned build for $(uname -s)-$(uname -m), skipped" >&2
    return
  fi
  if [ -f "$cache/bin/mermaid-ascii.version" ] && [ "$(cat "$cache/bin/mermaid-ascii.version")" = "$mermaid_ascii_version $asset $sha256" ]; then
    return
  fi
  local download
  download=$(mktemp -d)
  curl -fsSL -o "$download/mermaid-ascii.tar.gz" \
    "https://github.com/AlexanderGrooff/mermaid-ascii/releases/download/$mermaid_ascii_version/mermaid-ascii_$asset.tar.gz"
  echo "$sha256  $download/mermaid-ascii.tar.gz" | shasum -a 256 -c --quiet
  tar -xzf "$download/mermaid-ascii.tar.gz" -C "$download" mermaid-ascii
  mv "$download/mermaid-ascii" "$cache/bin/mermaid-ascii"
  echo "$mermaid_ascii_version $asset $sha256" > "$cache/bin/mermaid-ascii.version"
  rm -rf "$download"
}

install_beautiful_mermaid() {
  if ! command -v npm > /dev/null; then
    echo "beautiful-mermaid: npm not found, skipped" >&2
    return
  fi
  cp "$here/node/package.json" "$here/node/package-lock.json" "$cache/node/"
  npm ci --prefix "$cache/node" --ignore-scripts --no-audit --no-fund --silent
}

install_mermaid_ascii
install_beautiful_mermaid

echo "installed in $cache:"
[ -x "$cache/bin/mermaid-ascii" ] && echo "  mermaid-ascii $(cat "$cache/bin/mermaid-ascii.version")"
[ -f "$cache/node/node_modules/beautiful-mermaid/package.json" ] &&
  echo "  beautiful-mermaid $(node -p 'require(process.argv[1]).version' "$cache/node/node_modules/beautiful-mermaid/package.json")"
exit 0
