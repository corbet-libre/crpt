#!/usr/bin/env bash
set -euo pipefail
exec 2>&1
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"
export RUST_TEST_THREADS="${RUST_TEST_THREADS:-2}"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
# The checkout's own content, never a per-run value: resolve the tree this
# checkout stands on, so only committed content decides the result. A source
# without a resolvable revision is still refused (the job runner already
# refuses a checkout that is not the declared commit).
git rev-parse --verify 'HEAD^{tree}' >/dev/null 2>&1 ||
  { echo 'verify requires a checkout with a resolvable source tree' >&2; exit 2; }
: "${ARTIFACT_ROOT:?verify requires an absolute ARTIFACT_ROOT}"
[[ "$ARTIFACT_ROOT" = /* ]] || { echo "ARTIFACT_ROOT must be absolute" >&2; exit 2; }
export RUNNER_TEMP="${RUNNER_TEMP:-$(mktemp -d)}"
export GITHUB_PATH="${GITHUB_PATH:-$RUNNER_TEMP/github_path}"
touch "$GITHUB_PATH"
cp Cargo.lock "$ARTIFACT_ROOT/Cargo.lock"
python3 .github/check-first-party.py
bash .ci/update-our-crates.sh
cargo fmt --all -- --check
cargo clippy --locked --all-features --all-targets -- -D warnings
cargo test --locked --all-features
cargo check --locked --target wasm32-unknown-unknown
rustup run nightly rustc --version
NIGHTLY_CARGO="$(rustup which --toolchain nightly cargo)"
NIGHTLY_RUSTC="$(rustup which --toolchain nightly rustc)"
export RUSTC="$NIGHTLY_RUSTC" CARGO="$NIGHTLY_CARGO" RUSTUP_TOOLCHAIN=nightly
export PATH="$(dirname "$NIGHTLY_RUSTC"):$PATH"
"$NIGHTLY_CARGO" llvm-cov --version
# Bug 2 isolation: coverage data must not reuse the shared cargo target
# across jobs (stale checkout paths fail the next gate); scope the
# coverage target to this job only. Native gates keep the shared warm
# target; outer traps and services are untouched.
(
coverage_target="$(mktemp -d)"
case "$coverage_target" in ""|"$PWD"/*) echo 'unsafe coverage target' >&2; exit 2;; esac
trap 'rm -rf -- "$coverage_target"' EXIT
export CARGO_TARGET_DIR="$coverage_target"
"$NIGHTLY_CARGO" llvm-cov --locked --workspace --all-features --branch --no-cfg-coverage --no-cfg-coverage-nightly --ignore-filename-regex '/tests/' --json --output-path coverage.json
"$NIGHTLY_CARGO" llvm-cov report --locked --branch --ignore-filename-regex '/tests/' --lcov --output-path coverage.lcov
"$NIGHTLY_CARGO" llvm-cov report --locked --branch --ignore-filename-regex '/tests/' --text --output-path coverage.txt
)
cp coverage.json coverage.lcov coverage.txt "$ARTIFACT_ROOT/"
python3 .github/test-source-coverage.py
python3 .github/check-source-coverage.py coverage.lcov coverage.json coverage.txt
