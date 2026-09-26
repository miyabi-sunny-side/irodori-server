#!/usr/bin/env bash
# Tests the pure helpers of irodori.sh. Run: bash tests/launcher_test.sh
set -u
# shellcheck source=irodori.sh
source "$(dirname "$0")/../irodori.sh"
fail=0
check() {
  local want=$1 got; shift
  got=$(select_backend "$@") || got=ERR
  [[ $got == "$want" ]] || { echo "FAIL select_backend $*: want $want, got $got"; fail=1; }
}
check cu128 auto "" 1  # GPU found
check cpu auto "" 0    # no GPU
check cpu auto cpu 1   # saved choice wins over detection
check cu128 cu128 cpu 0  # explicit request wins over saved
check ERR auto bogus 0 # broken config/backend.txt
check ERR rocm "" 1    # unsupported request
[[ $fail == 0 ]] && echo "launcher tests passed"
exit $fail
