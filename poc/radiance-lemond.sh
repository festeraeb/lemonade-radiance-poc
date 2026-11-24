#!/usr/bin/env bash
# Start a throwaway lemond that runs radiance through engine-adapters/backends.d/radiance.json.
#
#   bash poc/radiance-lemond.sh MODEL.rad RADIANCE_BUILD_DIR [PORT] [ROOT]
#     MODEL.rad           a radiance container (e.g. minicpm5-2b-bf16.rad)
#     RADIANCE_BUILD_DIR  radiance's build dir from radiance/build.sh (has bin/ and radiance_home/)
#     PORT                lemond port, default 13391
#     ROOT                cache/config folder, default ./poc-root
#
# Before: build lemond and lemonade (cmake --preset default && cmake --build --preset default
# --target lemond lemonade) and the launcher (cargo build --release --manifest-path
# engine-adapters/Cargo.toml --bin radiance-lemonade). Registers the model twice (rad-a, rad-b)
# for the eviction check. Stop lemond with: kill "$(cat ROOT/lemond.pid)"
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
model=$(realpath "$1"); rbuild=$(realpath "$2"); port=${3:-13391}; root=$(realpath -m "${4:-$PWD/poc-root}")

launcher=${RADIANCE_LEMONADE:-${CARGO_TARGET_DIR:-$repo/engine-adapters/target}/release/radiance-lemonade}
[ -x "$launcher" ] || { echo "build the launcher first (see the header)"; exit 1; }
[ -x "$rbuild/bin/radiance" ] || { echo "no $rbuild/bin/radiance"; exit 1; }
lemond=$repo/build/lemond
[ -x "$lemond" ] || { echo "build lemond first (see the header)"; exit 1; }

# The launcher finds radiance beside itself and RADIANCE_HOME in the build tree.
cp "$launcher" "$rbuild/bin/radiance-lemonade"
mkdir -p "$root/cache" "$root/config/backends.d"
cp "$repo/engine-adapters/backends.d/radiance.json" "$root/config/backends.d/"
bin="$rbuild/bin/radiance-lemonade"
cat > "$root/config/config.json" <<EOF
{"config_version": 2, "offline": true, "auto_check_model_updates": false, "broadcast": false,
 "disable_model_filtering": true, "log_level": "debug",
 "radiance": {"cpu_bin": "$bin", "rocm_bin": "$bin"}}
EOF
cat > "$root/config/user_models.json" <<EOF
{"rad-a": {"recipe": "radiance", "checkpoint": "$model", "source": "local_path", "labels": ["chat"]},
 "rad-b": {"recipe": "radiance", "checkpoint": "$model", "source": "local_path", "labels": ["chat"]}}
EOF
nohup "$lemond" --port "$port" --host 127.0.0.1 "$root/cache" "$root/config" > "$root/lemond.log" 2>&1 &
echo $! > "$root/lemond.pid"
echo "lemond pid $(cat "$root/lemond.pid") on 127.0.0.1:$port; log $root/lemond.log"
echo "grade: python3 tools/backend_conformance.py --lemond http://127.0.0.1:$port \\"
echo "         --definition $root/config/backends.d/radiance.json --backends-d $root/config/backends.d \\"
echo "         --model rad-a --other rad-b [--device 0 --device-gpu 0 | --cpu-only]"
