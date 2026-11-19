entity_generator() {
  local tools="$1"
  local version cli
  version="$(cargo tree --locked -p sea-orm --depth 0 --prefix none | awk '{ sub(/^v/, "", $2); print $2 }')"
  cli="$tools/bin/sea-orm-cli"
  if [[ ! -x "$cli" ]] || [[ "$("$cli" --version)" != "sea-orm-cli $version" ]]; then
    cargo install sea-orm-cli --version "$version" --locked --no-default-features \
      --features codegen,sqlx-sqlite,runtime-tokio --root "$tools" \
      --target-dir "${tools}-build" >&2 || return
  fi
  printf '%s\n' "$cli"
}

generate_entities() {
  local cli="$1" database="$2" output="$3" destination="$4" tables="$5"
  mkdir "$output"
  "$cli" generate entity --compact-format --with-prelude none \
    --database-url "sqlite://$database" --tables "$tables" --output-dir "$output"
  rustfmt --edition 2024 "$output/mod.rs"
  mkdir -p "$destination"
  find "$destination" -maxdepth 1 -name '*.rs' -type f -delete
  cp "$output/"*.rs "$destination/"
}
