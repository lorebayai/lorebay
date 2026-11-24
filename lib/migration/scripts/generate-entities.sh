#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$repo_root"
source lib/migration/scripts/entity-generation.sh

cli="$(entity_generator "$repo_root/target/tools")"
schema_dir="$(mktemp -d "${TMPDIR:-/tmp}/lorebay-library-entities.XXXXXX")"
trap 'rm -rf -- "$schema_dir"' EXIT
database="$schema_dir/library.sqlite3"
cargo run -p lorebay-lib-migration --locked --features entity-generation --bin prepare-library-entity-database -- "$database"
generate_entities "$cli" "$database" "$schema_dir/entities" lib/core/src/entity \
  space,space_task,space_task_result,space_checkpoint,space_agent_file,space_knowledge,space_knowledge_version,space_person_request
