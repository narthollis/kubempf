#!/bin/bash
llvm-cov show \
    $( \
      for file in \
        $( \
          RUSTFLAGS="-C instrument-coverage" \
            cargo test --tests --no-run --message-format=json \
              | jq -r "select(.profile.test == true) | .filenames[]" \
              | grep -v dSYM - \
        ); \
      do \
        printf "%s %s " -object $file; \
      done \
    ) \
  --ignore-filename-regex='/.cargo/registry' \
  --instr-profile=kubempf.profdata \
  --show-instantiations --show-line-counts-or-regions --use-color \
  --Xdemangler=rustfilt
