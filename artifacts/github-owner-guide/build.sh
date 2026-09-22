#!/usr/bin/env bash
set -euo pipefail
guide_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
for guide_name in github-owner-guide guia-github-pt-br; do
  nix shell github:NixOS/nixpkgs/9ae611a455b90cf061d8f332b977e387bda8e1ca#typst --command typst compile --ignore-system-fonts --pdf-standard 1.5 --creation-timestamp 1790078400 "$guide_dir/$guide_name.typ" "$guide_dir/$guide_name.pdf"
done
