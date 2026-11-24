#!/usr/bin/env bash
# Build radiance at the tested commit with this repo's patches (libr3d on RDNA2 + a schema fix).
#
#   bash radiance/build.sh [GPU_TARGETS] [SOURCE_DIR]
#     GPU_TARGETS  semicolon list, default gfx1031; "none" builds the CPU host backend only
#     SOURCE_DIR   default ./radiance-src
#
# Needs git, cmake 3.21+, ninja, clang (C++20), and for GPU targets ROCm 7.2 (ROCM_PATH, default /opt/rocm).
# Tests are off (RAD_BUILD_TESTS=OFF): one upstream test file does not compile with every clang.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
targets=${1:-gfx1031}
dest=${2:-$PWD/radiance-src}
rev=5e0cd1e

[ -d "$dest/.git" ] || git clone https://codeberg.org/StillDeadcode/radiance.git "$dest"
cd "$dest"
git checkout -q "$rev"
git switch -q -C rdna2-libr3d
git -c user.name=build -c user.email=build@localhost am -q "$here"/patches/*.patch

if [ "$targets" = none ]; then
    # clang, as upstream builds it: the architecture plugins do not compile with GCC.
    CC=${CC:-clang} CXX=${CXX:-clang++} \
        cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DRAD_WITH_HIP=OFF -DRAD_BUILD_TESTS=OFF
    cmake --build build
else
    rocm=${ROCM_PATH:-/opt/rocm}
    export ROCM_PATH=$rocm   # CMake's HIP detection reads it from the environment
    link="-L$rocm/lib -Wl,-rpath,$rocm/lib"
    # The -isystem and -L flags are for ROCm installs whose headers and libraries the
    # build does not find on its own (seen with ROCm 7.2 packages on Arch).
    CC=$rocm/llvm/bin/clang CXX=$rocm/llvm/bin/clang++ cmake -S . -B build -G Ninja \
        -DCMAKE_BUILD_TYPE=Release -DRAD_GPU_TARGETS="$targets" -DRAD_BUILD_TESTS=OFF \
        -DCMAKE_CXX_FLAGS="-isystem $rocm/include" -DCMAKE_HIP_FLAGS="-isystem $rocm/include" \
        -DCMAKE_SHARED_LINKER_FLAGS="$link" -DCMAKE_MODULE_LINKER_FLAGS="$link" \
        -DCMAKE_EXE_LINKER_FLAGS="$link"
    cmake --build build
fi
echo "built: $dest/build/bin/radiance  plugins: $dest/build/radiance_home"
