#!/bin/bash

set -euxo pipefail

mkdir -p release/

cross_fn () {
    export CARGO_BUILD_TARGET="${1}"
    export CARGO_TARGET_DIR=target/build/"${CARGO_BUILD_TARGET}"
    cross build --release

    exe="$(find "${CARGO_TARGET_DIR}"/"${CARGO_BUILD_TARGET}"/release/ -maxdepth 1 -executable -type f)"
    cp "$exe" "release/${CARGO_BUILD_TARGET}-$(basename $exe)"
}

cross_fn x86_64-pc-windows-gnu
cross_fn x86_64-unknown-linux-gnu
