#!/bin/sh
set -eu
PROJECT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$PROJECT_DIR"
cargo test --locked
./scripts/build.sh
node scripts/test-browser.mjs
./scripts/build-gpui-web.sh
npx --yes wrangler@4.151.1 pages deploy web --project-name zeuss-lens --branch main "$@"
