# Continuous integration

This repository uses one required GitHub Actions workflow at
`.github/workflows/ci.yml`. It runs for every pull request and for every push
to `main`. The workflow is intentionally a direct transcription of the local
verification commands, so a contributor can reproduce a failure without
learning a CI-specific wrapper.

## Toolchain contract

`rust-toolchain.toml` pins Rust `1.97.1` and requests the
`wasm32-unknown-unknown` target. The CI job names the same version explicitly
and installs the wasm target. The file is part of the cache key, so changing the
compiler invalidates old build artifacts.

A pinned toolchain makes compiler, formatter, and clippy behavior reviewable.
It also prevents a contributor's local stable channel from silently producing a
different result from the required checks. To use the repository toolchain
locally, install rustup and run commands from the repository root; rustup reads
the file automatically.

## Trigger policy

The workflow has two triggers:

| Event | Scope | Purpose |
| --- | --- | --- |
| Pull request | Every pull request | Gate review before merge |
| Push | `main` only | Verify the protected integration branch |

The workflow requests only `contents: read`. It does not need signing keys,
deployment credentials, write access, or access to pull-request secrets. Fork
pull requests therefore run with the same read-only boundary as trusted
branches.

## Verification order

The job runs checks in this order:

1. Check out the exact commit under review.
2. Install the pinned compiler and wasm target.
3. Restore or populate the Cargo cache.
4. Check all repository formatting.
5. Build all native workspace targets.
6. Run all workspace tests.
7. Run clippy for all targets with warnings denied.
8. Build the escrow release wasm.
9. Print the wasm size and SHA-256 digest.

The first failing command stops the job. Every gate is therefore an actual
required status, rather than a log-only advisory. The native build and tests
run before the release wasm build so ordinary Rust errors are easier to find in
the job summary.

## Formatting gate

The format command is:

```sh
cargo fmt --all -- --check
```

It checks every workspace package and does not rewrite files in CI. If it
fails, run `cargo fmt --all` locally, inspect the complete diff, and commit the
result. Formatting changes should remain separate from behavior changes when
that makes review easier.

## Build gate

The native build command is:

```sh
cargo build --workspace --all-targets
```

It compiles the workspace and test targets without depending on a release
artifact. The separate wasm command is:

```sh
cargo build -p grainhack-escrow \
  --target wasm32-unknown-unknown --release
```

The release build uses the profile in the root Cargo manifest. The workflow
does not upload or deploy the wasm. It only proves that the artifact can be
reproduced and reports its identity for review.

## Test gate

The test command is:

```sh
cargo test --workspace --all-targets
```

Tests run in the host environment and are expected to be deterministic. Tests
that require an external service must provide a documented local prerequisite
and should not silently skip a security assertion. A test failure blocks the
workflow even when compilation and formatting pass.

## Clippy gate

The lint command is:

```sh
cargo clippy --workspace --all-targets -- -D warnings
```

Warnings are errors in CI. Contributors should fix the underlying warning
instead of adding a broad `allow` attribute. An exception should be narrowly
scoped, explained in code, and discussed in the pull request because a new
allow can hide a future regression.

The current escrow cleanup removes unnecessary `clone()` calls on the `Pool`
enum. `Pool` derives `Copy`, so moving or copying it is clearer and avoids
allocating or retaining an unnecessary duplicate value.

## Wasm artifact reporting

The final step searches `target/wasm32-unknown-unknown/release` for wasm files.
It fails if none exists, then prints one record per artifact:

```text
wasm: target/wasm32-unknown-unknown/release/grainhack_escrow.wasm
size_bytes: 12345
sha256: ...
```

The size is a byte count from `wc -c`, not a rounded human-readable estimate.
The digest helps reviewers distinguish a new artifact from stale local output.
The workflow does not impose an arbitrary size threshold because this issue
requires visibility and reproducible review; a future size budget can consume
the same reported value.

## Cache design

The cache includes:

- the Cargo registry index and downloaded registry sources;
- downloaded Git dependencies;
- the workspace `target` directory.

The primary key includes the operating system, pinned toolchain, lockfile,
package manifests, and Rust source hashes. A source edit gets an exact cache
key, while the restore keys allow reuse of dependency compilation when only
application code changed. A lockfile or toolchain update invalidates the
dependency graph cache deliberately.

The cache is an optimization only. A cache miss must still produce a complete
build. No generated artifact or secret is used as a source of truth.

## Local reproduction

From a clean checkout, run the gates individually:

```sh
cargo fmt --all -- --check
cargo build --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p grainhack-escrow --target wasm32-unknown-unknown --release
```

To reproduce the artifact report:

```sh
for artifact in target/wasm32-unknown-unknown/release/*.wasm; do
  wc -c < "$artifact"
  shasum -a 256 "$artifact"
done
```

On macOS, `shasum -a 256` is the local equivalent of the Linux runner's
`sha256sum`. The size must be measured from the release output, not from a
debug build or an unoptimized intermediate.

## Failure triage

### Formatting failure

Run `cargo fmt --all`, inspect the diff, and rerun the check. Do not manually
reformat generated files or suppress the format gate.

### Build failure

Read the first compiler error, confirm the pinned toolchain with `rustc
--version`, and run the package's build command locally. A build failure is
not fixed by changing CI to compile fewer packages.

### Test failure

Run the failing test by name with `cargo test package_name test_name`. Check
whether the failure is deterministic and whether the test depends on a clock,
network, filesystem, or an undeclared service. The fix belongs in the test or
implementation, not in an unconditional skip.

### Clippy failure

Run the displayed lint locally without `-D warnings` if additional context is
needed, then apply the narrowest correct fix. Review whether a warning reveals
a real ownership, arithmetic, or API issue before changing the code.

### Wasm failure

Confirm the wasm target is installed and that the package name is still
`grainhack-escrow`. Do not replace the release build with a host build: the
target-specific compile is the deployment artifact check.

### Size report failure

Confirm that the release build completed and that a `.wasm` file exists under
the expected target directory. The script intentionally exits non-zero when no
artifact is found so a deleted or renamed package cannot produce a misleading
green job.

## Review expectations

Reviewers should verify that a change:

- keeps all five gates present and required;
- does not widen permissions or expose secrets to fork builds;
- preserves the pinned toolchain unless a version change is intentional;
- keeps cache keys tied to the dependency graph and compiler;
- leaves the wasm size visible in the job log;
- explains any new test infrastructure prerequisite;
- does not introduce a silent skip for a failing assertion.

The workflow is deliberately small enough to audit in one sitting. New steps
should have a clear repository-level purpose and should use the narrowest
available action permissions.

## Maintenance policy

When Rust is upgraded, update `rust-toolchain.toml`, the workflow's explicit
toolchain input, and this document in one pull request. Run all local gates and
record the old and new compiler versions in the description.

When a new workspace package is added, the workspace commands automatically
include it. Confirm that the release wasm step still names the intended
deployable package and that the size report does not accidentally select an
unrelated artifact.

When the CI provider changes an action major version, review its permission and
runtime changes before updating. Pinning a moving action to a major version is
acceptable here because the repository already relies on maintained official
actions, but the change should remain visible in review.

## Security boundary

CI builds and tests code. It does not deploy contracts, fund accounts, sign
transactions, publish roots, or access production data. The read-only token
permission is intentional. Any future deployment job must be separate from
this verification job, require an explicit environment approval, and use
credentials unavailable to pull-request workflows.

The artifact digest is informational and does not constitute an audit or a
release signature. Release signing and mainnet operations remain human tasks
outside this repository's CI gate.

## Status interpretation

The single `verify` job is the merge gate for this repository. A green result
means that the checked commit passed the repository's declared build, test,
format, lint, and wasm compilation commands on the pinned runner toolchain. It
does not mean that a contract has been deployed or that external integrations
are healthy.

The size and digest lines are retained in the job log as ordinary build output.
They are useful during review, but they are not a replacement for a release
artifact registry. A release process may copy the same values into a signed
release note after a maintainer has reviewed the source and workflow result.

If the workflow is red because a dependency service is unavailable, the
service prerequisite must be made explicit or the test must be redesigned for
the host environment. Marking the job successful by adding `|| true`, hiding
the failure in a shell pipeline, or changing a test to an unconditional skip
violates the purpose of this gate.

If a contributor needs to compare two artifact sizes, compare commits built by
the same workflow and toolchain. Local debug artifacts, different optimization
profiles, and different compiler versions are not comparable release data.
