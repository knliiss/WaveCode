#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p assets/examples

# The committed SVGs are generated with the real V1 encoder and default
# capsule-rail renderer. Track titles are documentation only, not payload data.
cargo run --quiet -p wavecode -- encode \
  'https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b' \
  --svg assets/examples/blinding-lights.svg --width 960 --height 240 >/dev/null

cargo run --quiet -p wavecode -- encode \
  'https://open.spotify.com/track/7qiZfU4dY1lWllzX7mPBI3' \
  --svg assets/examples/shape-of-you.svg --width 960 --height 240 >/dev/null

cargo run --quiet -p wavecode -- encode \
  'https://open.spotify.com/track/2Fxmhks0bxGSBdJ92vM42m' \
  --svg assets/examples/bad-guy.svg --width 960 --height 240 >/dev/null

echo "WaveCode examples regenerated in assets/examples/"
