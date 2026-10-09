#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
if [[ -d "$PWD/.local/sysroot/usr/lib/x86_64-linux-gnu" ]]; then
  export LIBRARY_PATH="$PWD/.local/sysroot/usr/lib/x86_64-linux-gnu${LIBRARY_PATH:+:$LIBRARY_PATH}"
  export LD_LIBRARY_PATH="$PWD/.local/sysroot/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi
exec cargo run --locked -- "$@"
