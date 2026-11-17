#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

sea_orm_version="$(cargo tree --locked -p sea-orm --depth 0 --prefix none | awk '{ sub(/^v/, "", $2); print $2 }')"
cli="$repo_root/target/tools/bin/sea-orm-cli"
if [[ ! -x "$cli" ]] || [[ "$("$cli" --version)" != "sea-orm-cli $sea_orm_version" ]]; then
  cargo install sea-orm-cli --version "$sea_orm_version" --locked --no-default-features \
    --features codegen,sqlx-sqlite,runtime-tokio --root "$repo_root/target/tools" \
    --target-dir "$repo_root/target/tools-build"
fi
cargo build -p lorebayd --locked --target-dir "$repo_root/target"

schema_dir="$(mktemp -d "${TMPDIR:-/tmp}/lorebay-entities.XXXXXX")"
server_pid=""
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill -TERM "$server_pid" 2>/dev/null || true
    wait "$server_pid" || true
  fi
  rm -rf -- "$schema_dir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

printf '[]\n' > "$schema_dir/users.json"
env -i PATH="$PATH" LOREBAYD__USERS_FILE="$schema_dir/users.json" \
  LOREBAYD__DATA_DIRECTORY="$schema_dir" LOREBAYD__LISTEN_ADDRESS=127.0.0.1:0 \
  "$repo_root/target/debug/lorebayd" 2>"$schema_dir/server.log" &
server_pid=$!
for _ in {1..100}; do
  if grep -q '"message":"lorebayd ready"' "$schema_dir/server.log"; then
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    cat "$schema_dir/server.log" >&2
    exit 1
  fi
  sleep 0.1
done
if ! grep -q '"message":"lorebayd ready"' "$schema_dir/server.log"; then
  cat "$schema_dir/server.log" >&2
  exit 1
fi
kill -TERM "$server_pid"
wait "$server_pid"
server_pid=""

"$cli" generate entity --compact-format --with-prelude none \
  --database-url "sqlite://$schema_dir/lorebay.sqlite3" --output-dir "$repo_root/lorebayd/src/entity"
cargo fmt --all
