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

schema_dir="$(mktemp -d "${TMPDIR:-/tmp}/lorebay-entities.XXXXXX")"
trap 'rm -rf -- "$schema_dir"' EXIT
cargo run -p lorebay-schema --locked -- "$schema_dir/core.sqlite3" "$schema_dir/product.sqlite3"

python3 - "$schema_dir/product.sqlite3" <<'PY'
from contextlib import closing
import sqlite3
import sys

with closing(sqlite3.connect(sys.argv[1])) as projection:
    projection.execute("DROP INDEX participant_one_owner")
PY

generate() {
  local database="$1" destination="$2" tables="$3"
  local output="$schema_dir/$database"
  mkdir "$output"
  "$cli" generate entity --compact-format --with-prelude none \
    --database-url "sqlite://$schema_dir/$database.sqlite3" --tables "$tables" --output-dir "$output"
  mkdir -p "$destination"
  find "$destination" -maxdepth 1 -name '*.rs' -type f -delete
  cp "$output/"*.rs "$destination/"
  rustfmt --edition 2024 "$destination/mod.rs"
}

generate core lib/core/src/entity space,space_task,space_note,space_artifact,space_knowledge,space_knowledge_version,space_proposal
generate product lorebayd/src/entity account,participant,agent_authorization,login_session,oauth_client,oauth_code,refresh_token_history,device_sign_in_request
