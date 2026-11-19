#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$repo_root"
source lib/migration/scripts/entity-generation.sh

cli="$(entity_generator "$repo_root/target/lorebayd-migration-tools")"
schema_dir="$(mktemp -d "${TMPDIR:-/tmp}/lorebay-product-entities.XXXXXX")"
trap 'rm -rf -- "$schema_dir"' EXIT
database="$schema_dir/product.sqlite3"
cargo run -p lorebayd-migration --locked --features entity-generation --bin prepare-lorebay-entity-database -- "$database"

python3 - "$database" <<'PY'
from contextlib import closing
import sqlite3
import sys

with closing(sqlite3.connect(sys.argv[1])) as projection:
    projection.execute("DROP INDEX participant_one_owner")
PY

generate_entities "$cli" "$database" "$schema_dir/entities" lorebayd/src/entity \
  account,participant,agent_authorization,login_session,oauth_client,oauth_code,refresh_token_history,device_sign_in_request
