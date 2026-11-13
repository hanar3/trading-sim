#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$project_root"
export APP_DATABASE__FILE="${APP_DATABASE__FILE:-$project_root/message-persistor/db.sqlite}"
cargo run --locked -p message-persistor -- --migrate-only
