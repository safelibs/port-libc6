# Final Coverage, CI Integration, and Drop-In Closure

## Phase Name

Final Coverage, CI Integration, and Drop-In Closure

## Implement Phase ID

`impl_15_final_coverage_ci_drop_in`

## Preexisting Inputs

- All outputs from `impl_14_client_regression_fixes`.
- Template CI hooks: `scripts/build-debs.sh`, `scripts/run-upstream-tests.sh`, `scripts/run-port-tests.sh`, `scripts/run-validation-tests.sh`.
- Existing `xtask` checkers.
- Existing `.github/workflows/ci-release.yml`, which must remain unchanged in this port.
- The phase-13 `test-original.sh` wrapper and the package authority set from the phase-15 `PHASE_BASE_REF`, both of which are preserved inputs in this phase.

## New Outputs

- `scripts/run-upstream-tests.sh` wired to run the full upstream execution/equivalence gate through `xtask check-owned-tests`.
- `scripts/run-port-tests.sh` wired to run regression tests and a configurable dependent-app profile.
- `safe/generated/baseline/upstream-test-execution-ledger.json`.
- Upgraded `safe/xtask/src/commands/check_headers.rs` for exhaustive installed-header coverage.
- Upgraded `safe/xtask/src/commands/check_owned_tests.rs` final mode so skipped original tests are covered by an explicit execution/equivalence ledger.
- Upgraded `safe/xtask/src/commands/check_abi.rs` strict mode for symbol metadata.
- Upgraded `safe/xtask/src/commands/link_compat_smoke.rs` to add `link-compat-smoke --strict-dev-assets` development asset/static archive checks.

## File Changes

- `scripts/run-port-tests.sh`
- `scripts/run-upstream-tests.sh`
- `tests/port/dependent-apps/**`
- `tests/port/regressions/**`
- `safe/xtask/src/commands/check_headers.rs`
- `safe/xtask/src/commands/check_owned_tests.rs`
- `safe/xtask/src/commands/check_abi.rs`
- `safe/xtask/src/commands/link_compat_smoke.rs`
- `safe/generated/baseline/upstream-test-execution-ledger.json`
- `safe/generated/baseline/dependent-app-test-plan.json`
- `safe/generated/baseline/client-app-regressions.json`
- Do not edit `.github/workflows/ci-release.yml`.
- Do not edit `test-original.sh`; phase 15 must preserve the phase-13 wrapper.
- Do not edit `safe/generated/baseline/package-files/*.json`, `safe/generated/install-manifests/*.json`, or `safe/generated/packaging/package-build-manifest.json`; phase 15 does not own package authority updates.

## Implementation Details

- `scripts/run-upstream-tests.sh` must resolve its profile inside the hook, without requiring workflow edits:
  - `SAFELIBS_UPSTREAM_TEST_PROFILE=legacy`: run the existing `tests/upstream/*.sh` discovery.
  - `SAFELIBS_UPSTREAM_TEST_PROFILE=full`: stage/build the safe tree and run `cargo run -p xtask -- check-owned-tests --all-ported --root work/install-root --build-root work/original-build --require-execution-ledger`.
  - unset profile in CI, detected by non-empty `GITHUB_ACTIONS` or `SAFELIBS_COMMIT_SHA`: use `full`.
  - unset profile outside CI: use `legacy`.
- `scripts/run-port-tests.sh` must run `tests/port/regressions/run.sh` unconditionally once it exists. It must run dependent app coverage based on an environment profile:
  - unset profile in CI, detected by non-empty `GITHUB_ACTIONS` or `SAFELIBS_COMMIT_SHA`: use `full`.
  - unset profile outside CI, or `SAFELIBS_PORT_TEST_PROFILE=quick`: image contract plus core-network apps.
  - `SAFELIBS_PORT_TEST_PROFILE=full`: build the dependent image, run the `image-contract` suite, run the `all-runtime` suite, run the `source-builds` suite, and run `tests/port/regressions/run.sh`. The `all-runtime` suite is the full 16-app runtime matrix and includes the diagnostic apps `strace`, `valgrind`, and `libvirt`.
- Both hooks must write a machine-readable profile trace under `safe/work/hook-profiles/` on every successful invocation. `scripts/run-upstream-tests.sh` writes `safe/work/hook-profiles/run-upstream-tests.json` with `script`, `requested_profile` (`null` when unset), `effective_profile`, `ci_detected`, `ci_signals`, and `executed_commands`. `scripts/run-port-tests.sh` writes `safe/work/hook-profiles/run-port-tests.json` with the same profile fields plus `executed_suites`. The phase-15 checker uses these traces to prove that the unset `SAFELIBS_COMMIT_SHA` path takes the same full coverage route as the explicit `SAFELIBS_*_PROFILE=full` path.
- Hook traces are not sufficient by themselves. Every `full` port-test hook invocation must run the suites and regression runner in that invocation and must recreate `safe/work/dependent-apps/results/image-contract.json`, `safe/work/dependent-apps/results/all-runtime.json`, `safe/work/dependent-apps/results/source-builds.json`, their matching log directories, and `safe/work/regressions/results.json` if those files were deleted immediately before the hook. Every `full` upstream-test hook invocation must recreate `safe/work/hook-profiles/run-upstream-tests.json` and `safe/generated/baseline/upstream-test-execution-ledger.json` if they were deleted immediately before the hook. A hook must not infer success from existing result JSON, existing logs, an existing regression result, an existing hook profile, or an existing upstream-test execution ledger.
- The existing GitHub workflow already passes `SAFELIBS_COMMIT_SHA` to the upstream-test and port-test hooks, so release CI receives full coverage through hook profile resolution rather than workflow customization.
- Upgrade `check_headers.rs`:
  - Add `--all-installed`.
  - Enumerate every public header under `work/install-root/usr/include`.
  - Compile each header in C and C++ where applicable.
  - Run feature profiles for default, `_GNU_SOURCE`, `_POSIX_C_SOURCE=200809L`, `_XOPEN_SOURCE=700`, `_FILE_OFFSET_BITS=64`, and `_FORTIFY_SOURCE=3` with optimization.
  - Keep using upstream wrapper/local header scripts as additional checks.
- Upgrade `check_owned_tests.rs`:
  - Add `--require-execution-ledger`.
  - In final `--require-execution-ledger` mode, atomically rewrite `safe/generated/baseline/upstream-test-execution-ledger.json` from the current owned-test execution/equivalence result before validating it, so deleting that file immediately before `scripts/run-upstream-tests.sh` is a valid freshness check.
  - Fail final mode unless every one of the 5,584 catalog entries is either executed or mapped to an explicit equivalent verifier with a reason and command.
  - Remove silent acceptance of skipped original tests in final mode.
- Upgrade `check_abi.rs` strict mode:
  - Compare symbol name, version, default-version status, binding, type, visibility, size where meaningful, SONAME, version definitions, and `DT_NEEDED` against the prepared ABI/original build oracle.
  - Fail on data/TLS/IFUNC symbols exported with the wrong class.
- Extend static and startfile coverage:
  - Ensure `link-compat-smoke --strict-dev-assets` compares final static archive member lists and global symbol sets against the staged original artifacts.
  - Validate all final startfiles: `Mcrt1.o`, `Scrt1.o`, `crt1.o`, `crti.o`, `crtn.o`, `gcrt1.o`, `grcrt1.o`, and `rcrt1.o`.
- Keep logs under `safe/work/**`; do not commit run outputs.
- Commit the phase before yielding.

## Verification Phases

- `check_15_full_source_link_runtime_compatibility`
  - Type: `check`
  - Fixed `bounce_target`: `impl_15_final_coverage_ci_drop_in`
  - Purpose: Verify final source, link, runtime, ABI, header, upstream-test, and safety coverage.
  - Commands:
    ```bash
    bash scripts/check-layout.sh
    bash scripts/build-debs.sh
    cd safe
    cargo fmt --check
    cargo test --workspace
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check-abi --all-dsos --strict-symbol-metadata
    cargo run -p xtask -- check-headers --root work/install-root --lang c --lang c++ --all-installed --feature-profiles default,gnu,posix,xopen,large-file,fortify
    cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build --strict-dev-assets
    cargo run -p xtask -- check-owned-tests --all-ported --root work/install-root --build-root work/original-build --require-execution-ledger
    cargo run -p xtask -- audit-safety \
      --deny-unreviewed-unsafe \
      --deny-untracked-fallback-c \
      --deny-shipped-temporary-fallback-binaries \
      --deny-shipped-private-backend-dsos \
      --require-cve-disposition \
      --require-package-scope-clean
    test -z "$(git -C .. ls-files -- safe/work)"
    ```

- `check_15_dependent_app_full_matrix`
  - Type: `check`
  - Fixed `bounce_target`: `impl_15_final_coverage_ci_drop_in`
  - Purpose: Run final image-contract, full client-application runtime, source-build, and regression suites against the final safe image. Requires Docker with network access; image-contract runs without `--privileged`, and full runtime/source-build suites run with `--privileged`.
  - Commands:
    ```bash
    SAFE_VERSION="$(jq -r '.safe_package_version' safe/generated/packaging/package-build-manifest.json)"
    SAFE_IMAGE_TAG="$(printf '%s' "$SAFE_VERSION" | sed 's/[^A-Za-z0-9_.-]/-/g')"
    IMAGE="safelibs-libc6-dependent:${SAFE_IMAGE_TAG}"
    bash safe/scripts/build-debs.sh
    bash tests/port/dependent-apps/build-image.sh --debs safe/work/debs --manifest dependents.json --tag "$IMAGE"
    rm -f \
      safe/work/dependent-apps/results/image-contract.json \
      safe/work/dependent-apps/results/all-runtime.json \
      safe/work/dependent-apps/results/source-builds.json \
      safe/work/regressions/results.json
    rm -rf \
      safe/work/dependent-apps/logs/image-contract \
      safe/work/dependent-apps/logs/all-runtime \
      safe/work/dependent-apps/logs/source-builds
    rm -f safe/work/dependent-apps/results/image-contract.json
    rm -rf safe/work/dependent-apps/logs/image-contract
    bash tests/port/dependent-apps/run.sh --image "$IMAGE" --suite image-contract
    rm -f safe/work/dependent-apps/results/all-runtime.json
    rm -rf safe/work/dependent-apps/logs/all-runtime
    bash tests/port/dependent-apps/run.sh --image "$IMAGE" --suite all-runtime --privileged
    rm -f safe/work/dependent-apps/results/source-builds.json
    rm -rf safe/work/dependent-apps/logs/source-builds
    bash tests/port/dependent-apps/run.sh --image "$IMAGE" --suite source-builds --privileged
    rm -f safe/work/regressions/results.json
    bash tests/port/regressions/run.sh
    jq -e '.summary.failed == 0' safe/work/dependent-apps/results/image-contract.json
    jq -e '.summary.failed == 0' safe/work/dependent-apps/results/all-runtime.json
    jq -e '.summary.failed == 0' safe/work/dependent-apps/results/source-builds.json
    jq -e '.summary.failed == 0 and all(.results[]; .status == "passed")' safe/work/regressions/results.json
    for suite in image-contract all-runtime source-builds; do
      result="safe/work/dependent-apps/results/${suite}.json"
      jq -s -e --arg suite "$suite" --arg safe_version "$SAFE_VERSION" '
        .[0].suites[$suite].cases as $expected |
        .[1] as $result |
        ($result.suite == $suite) and
        ($result.status == "passed") and
        ($result.summary.total == ($expected | length)) and
        ($result.summary.total == ($result.cases | length)) and
        ($result.summary.passed == ($expected | length)) and
        ($result.summary.failed == 0) and
        ($result.summary.harness_failed == 0) and
        (($result.cases | map(.case) | sort) == ($expected | sort)) and
        all($result.cases[];
          (.suite == $suite) and
          (.case_id == ($suite + "/" + .case)) and
          (.status == "passed") and
          (has("failure_kind")) and
          (.failure_kind == null) and
          (.duration_seconds | type == "number") and
          (.log == ("safe/work/dependent-apps/logs/" + $suite + "/" + .case + ".log")) and
          (.safe_version == $safe_version) and
          (.rerun | type == "string" and length > 0)
        )
      ' safe/generated/baseline/dependent-app-test-plan.json "$result"
      jq -r '.cases[] | [.case_id, .case, .log, .safe_version] | @tsv' "$result" |
        while IFS=$'\t' read -r case_id case_name log_path case_version; do
          case "$log_path" in safe/work/dependent-apps/logs/${suite}/*) ;; *) exit 1 ;; esac
          test -f "$log_path"
          grep -F "suite=${suite}" "$log_path"
          grep -F "case=${case_name}" "$log_path"
          grep -F "case_id=${case_id}" "$log_path"
          grep -F "safe_version=${case_version}" "$log_path"
        done
    done
    rm -f \
      safe/work/dependent-apps/results/image-contract.json \
      safe/work/dependent-apps/results/all-runtime.json \
      safe/work/dependent-apps/results/source-builds.json
    rm -rf \
      safe/work/dependent-apps/logs/image-contract \
      safe/work/dependent-apps/logs/all-runtime \
      safe/work/dependent-apps/logs/source-builds
    bash test-original.sh
    for suite in image-contract all-runtime source-builds; do
      result="safe/work/dependent-apps/results/${suite}.json"
      test -f "$result"
      jq -s -e --arg suite "$suite" --arg safe_version "$SAFE_VERSION" '
        .[0].suites[$suite].cases as $expected |
        .[1] as $result |
        ($result.suite == $suite) and
        ($result.status == "passed") and
        ($result.summary.total == ($expected | length)) and
        ($result.summary.total == ($result.cases | length)) and
        ($result.summary.passed == ($expected | length)) and
        ($result.summary.failed == 0) and
        ($result.summary.harness_failed == 0) and
        (($result.cases | map(.case) | sort) == ($expected | sort)) and
        all($result.cases[];
          (.suite == $suite) and
          (.case_id == ($suite + "/" + .case)) and
          (.status == "passed") and
          (has("failure_kind")) and
          (.failure_kind == null) and
          (.duration_seconds | type == "number") and
          (.log == ("safe/work/dependent-apps/logs/" + $suite + "/" + .case + ".log")) and
          (.safe_version == $safe_version) and
          (.rerun | type == "string" and length > 0)
        )
      ' safe/generated/baseline/dependent-app-test-plan.json "$result"
      jq -r '.cases[] | [.case_id, .case, .log, .safe_version] | @tsv' "$result" |
        while IFS=$'\t' read -r case_id case_name log_path case_version; do
          case "$log_path" in safe/work/dependent-apps/logs/${suite}/*) ;; *) exit 1 ;; esac
          test -f "$log_path"
          grep -F "suite=${suite}" "$log_path"
          grep -F "case=${case_name}" "$log_path"
          grep -F "case_id=${case_id}" "$log_path"
          grep -F "safe_version=${case_version}" "$log_path"
        done
    done
    test -z "$(git ls-files -- safe/work)"
    ```

- `check_15_package_validator_release`
  - Type: `check`
  - Fixed `bounce_target`: `impl_15_final_coverage_ci_drop_in`
  - Purpose: Prove the packages remain drop-in installable through the template CI hooks and validator. The full port-test hook requires Docker with network access and `--privileged`.
  - Commands:
    ```bash
    rm -rf dist
    bash scripts/build-debs.sh
    cd safe
    cargo run -p xtask -- package-deb --out work/debs
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set basic-required-packages
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set libc-family-cutover
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set loader-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set runtime-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set network-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set locale-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set dev-and-time-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set backend-payload-closure
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set dev-link-artifacts
    cd ..
    SAFE_VERSION="$(jq -r '.safe_package_version' safe/generated/packaging/package-build-manifest.json)"
    reset_upstream_hook_outputs() {
      rm -f safe/work/hook-profiles/run-upstream-tests.json
      rm -f safe/generated/baseline/upstream-test-execution-ledger.json
    }
    reset_port_hook_outputs() {
      rm -f safe/work/hook-profiles/run-port-tests.json
      rm -f \
        safe/work/dependent-apps/results/image-contract.json \
        safe/work/dependent-apps/results/all-runtime.json \
        safe/work/dependent-apps/results/source-builds.json \
        safe/work/regressions/results.json
      rm -rf \
        safe/work/dependent-apps/logs/image-contract \
        safe/work/dependent-apps/logs/all-runtime \
        safe/work/dependent-apps/logs/source-builds
    }
    validate_fresh_port_outputs() {
      jq -e '.summary.failed == 0 and .summary.harness_failed == 0' safe/work/dependent-apps/results/image-contract.json
      jq -e '.summary.failed == 0 and .summary.harness_failed == 0' safe/work/dependent-apps/results/all-runtime.json
      jq -e '.summary.failed == 0 and .summary.harness_failed == 0' safe/work/dependent-apps/results/source-builds.json
      jq -e '.summary.failed == 0 and all(.results[]; .status == "passed")' safe/work/regressions/results.json
      for suite in image-contract all-runtime source-builds; do
        result="safe/work/dependent-apps/results/${suite}.json"
        test -f "$result"
        jq -s -e --arg suite "$suite" --arg safe_version "$SAFE_VERSION" '
          .[0].suites[$suite].cases as $expected |
          .[1] as $result |
          ($result.suite == $suite) and
          ($result.status == "passed") and
          ($result.summary.total == ($expected | length)) and
          ($result.summary.total == ($result.cases | length)) and
          ($result.summary.passed == ($expected | length)) and
          ($result.summary.failed == 0) and
          ($result.summary.harness_failed == 0) and
          (($result.cases | map(.case) | sort) == ($expected | sort)) and
          all($result.cases[];
            (.suite == $suite) and
            (.case_id == ($suite + "/" + .case)) and
            (.status == "passed") and
            (has("failure_kind")) and
            (.failure_kind == null) and
            (.duration_seconds | type == "number") and
            (.log == ("safe/work/dependent-apps/logs/" + $suite + "/" + .case + ".log")) and
            (.safe_version == $safe_version) and
            (.rerun | type == "string" and length > 0)
          )
        ' safe/generated/baseline/dependent-app-test-plan.json "$result"
        jq -r '.cases[] | [.case_id, .case, .log, .safe_version] | @tsv' "$result" |
          while IFS=$'\t' read -r case_id case_name log_path case_version; do
            case "$log_path" in safe/work/dependent-apps/logs/${suite}/*) ;; *) exit 1 ;; esac
            test -f "$log_path"
            grep -F "suite=${suite}" "$log_path"
            grep -F "case=${case_name}" "$log_path"
            grep -F "case_id=${case_id}" "$log_path"
            grep -F "safe_version=${case_version}" "$log_path"
          done
      done
    }
    reset_upstream_hook_outputs
    env SAFELIBS_UPSTREAM_TEST_PROFILE=full bash scripts/run-upstream-tests.sh
    jq -e '
      (.script == "scripts/run-upstream-tests.sh") and
      (.requested_profile == "full") and
      (.effective_profile == "full") and
      (.executed_commands | any(contains("check-owned-tests") and contains("--all-ported") and contains("--require-execution-ledger")))
    ' safe/work/hook-profiles/run-upstream-tests.json
    jq -e '(.entries | length == 5584) and all(.entries[]; .coverage_status == "executed" or .coverage_status == "equivalent")' safe/generated/baseline/upstream-test-execution-ledger.json
    git ls-files --error-unmatch -- safe/generated/baseline/upstream-test-execution-ledger.json
    git diff --exit-code -- safe/generated/baseline/upstream-test-execution-ledger.json
    reset_port_hook_outputs
    env SAFELIBS_PORT_TEST_PROFILE=full bash scripts/run-port-tests.sh
    jq -e '
      (.script == "scripts/run-port-tests.sh") and
      (.requested_profile == "full") and
      (.effective_profile == "full") and
      (.executed_suites | index("image-contract")) and
      (.executed_suites | index("all-runtime")) and
      (.executed_suites | index("source-builds")) and
      (.executed_suites | index("regressions"))
    ' safe/work/hook-profiles/run-port-tests.json
    validate_fresh_port_outputs
    reset_upstream_hook_outputs
    env -u SAFELIBS_UPSTREAM_TEST_PROFILE -u SAFELIBS_PORT_TEST_PROFILE SAFELIBS_COMMIT_SHA="$(git rev-parse HEAD)" bash scripts/run-upstream-tests.sh
    jq -e '
      (.script == "scripts/run-upstream-tests.sh") and
      (.requested_profile == null) and
      (.effective_profile == "full") and
      (.ci_detected == true) and
      (.ci_signals | index("SAFELIBS_COMMIT_SHA")) and
      (.executed_commands | any(contains("check-owned-tests") and contains("--all-ported") and contains("--require-execution-ledger")))
    ' safe/work/hook-profiles/run-upstream-tests.json
    jq -e '(.entries | length == 5584) and all(.entries[]; .coverage_status == "executed" or .coverage_status == "equivalent")' safe/generated/baseline/upstream-test-execution-ledger.json
    git ls-files --error-unmatch -- safe/generated/baseline/upstream-test-execution-ledger.json
    git diff --exit-code -- safe/generated/baseline/upstream-test-execution-ledger.json
    reset_port_hook_outputs
    env -u SAFELIBS_UPSTREAM_TEST_PROFILE -u SAFELIBS_PORT_TEST_PROFILE SAFELIBS_COMMIT_SHA="$(git rev-parse HEAD)" bash scripts/run-port-tests.sh
    jq -e '
      (.script == "scripts/run-port-tests.sh") and
      (.requested_profile == null) and
      (.effective_profile == "full") and
      (.ci_detected == true) and
      (.ci_signals | index("SAFELIBS_COMMIT_SHA")) and
      (.executed_suites | index("image-contract")) and
      (.executed_suites | index("all-runtime")) and
      (.executed_suites | index("source-builds")) and
      (.executed_suites | index("regressions"))
    ' safe/work/hook-profiles/run-port-tests.json
    validate_fresh_port_outputs
    bash scripts/run-validation-tests.sh
    test -z "$(git ls-files -- safe/work)"
    ```

- `check_15_senior_final_review`
  - Type: `check`
  - Fixed `bounce_target`: `impl_15_final_coverage_ci_drop_in`
  - Purpose: Final senior review of workflow linearity, committed phase outputs, preserved artifacts, and remaining risks.
  - Commands:
    ```bash
    : "${WORKFLOW_START_REF:?set to the commit before impl_11_dependent_app_image_contract began}"
    : "${PHASE_BASE_REF:?set to the commit before impl_15_final_coverage_ci_drop_in began}"
    git diff --check "$WORKFLOW_START_REF"..HEAD
    git diff --exit-code "$WORKFLOW_START_REF"..HEAD -- original dependents.json .github/workflows/ci-release.yml
    git diff --exit-code "$WORKFLOW_START_REF"..HEAD -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --exit-code "$WORKFLOW_START_REF"..HEAD -- all_cves.json relevant_cves.json
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- test-original.sh
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- safe/generated/baseline/package-files safe/generated/install-manifests safe/generated/packaging/package-build-manifest.json
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- all_cves.json relevant_cves.json safe/generated/security/relevant-cves-index.json safe/generated/baseline/fallback-c-inventory.json safe/generated/baseline/committed-safe-frontier.txt safe/upstream-compat
    git diff --cached --exit-code -- original dependents.json .github/workflows/ci-release.yml
    git diff --exit-code -- original dependents.json .github/workflows/ci-release.yml
    git diff --cached --exit-code -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --exit-code -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --cached --exit-code -- test-original.sh
    git diff --exit-code -- test-original.sh
    git diff --cached --exit-code -- safe/generated/baseline/package-files safe/generated/install-manifests safe/generated/packaging/package-build-manifest.json
    git diff --exit-code -- safe/generated/baseline/package-files safe/generated/install-manifests safe/generated/packaging/package-build-manifest.json
    git diff --cached --exit-code -- all_cves.json relevant_cves.json safe/generated/security/relevant-cves-index.json safe/generated/baseline/fallback-c-inventory.json safe/generated/baseline/committed-safe-frontier.txt safe/upstream-compat
    git diff --exit-code -- all_cves.json relevant_cves.json safe/generated/security/relevant-cves-index.json safe/generated/baseline/fallback-c-inventory.json safe/generated/baseline/committed-safe-frontier.txt safe/upstream-compat
    COMMIT_COUNT="$(git rev-list --count "$WORKFLOW_START_REF"..HEAD)"
    test "$COMMIT_COUNT" -ge 5
    for path in \
      tests/port/dependent-apps/Dockerfile \
      tests/port/dependent-apps/build-image.sh \
      tests/port/dependent-apps/run.sh \
      tests/port/dependent-apps/lib/common.sh \
      tests/port/dependent-apps/lib/image-contract.sh \
      tests/port/dependent-apps/lib/source-build-common.sh \
      tests/port/dependent-apps/cases/bash.sh \
      tests/port/dependent-apps/cases/coreutils.sh \
      tests/port/dependent-apps/cases/systemd.sh \
      tests/port/dependent-apps/cases/python3.12-minimal.sh \
      tests/port/dependent-apps/cases/git.sh \
      tests/port/dependent-apps/cases/openssh-client.sh \
      tests/port/dependent-apps/cases/network-manager.sh \
      tests/port/dependent-apps/cases/nginx.sh \
      tests/port/dependent-apps/cases/postgresql-16.sh \
      tests/port/dependent-apps/cases/ffmpeg.sh \
      tests/port/dependent-apps/cases/qemu-system-x86.sh \
      tests/port/dependent-apps/cases/podman.sh \
      tests/port/dependent-apps/cases/gnome-shell.sh \
      tests/port/dependent-apps/cases/strace.sh \
      tests/port/dependent-apps/cases/valgrind.sh \
      tests/port/dependent-apps/cases/libvirt.sh \
      tests/port/dependent-apps/source-builds/strace.sh \
      tests/port/dependent-apps/source-builds/valgrind.sh \
      tests/port/dependent-apps/source-builds/libvirt.sh \
      tests/port/regressions/run.sh \
      scripts/run-upstream-tests.sh \
      scripts/run-port-tests.sh \
      test-original.sh \
      safe/generated/baseline/dependent-app-test-plan.json \
      safe/generated/baseline/client-app-regressions.json \
      safe/generated/baseline/upstream-test-execution-ledger.json \
      safe/xtask/src/commands/check_headers.rs \
      safe/xtask/src/commands/check_owned_tests.rs \
      safe/xtask/src/commands/check_abi.rs \
      safe/xtask/src/commands/link_compat_smoke.rs
    do
      git ls-files --error-unmatch -- "$path"
    done
    find tests/port/regressions -type f -print |
      while IFS= read -r path; do
        git ls-files --error-unmatch -- "$path"
      done
    git diff --name-only "$PHASE_BASE_REF"..HEAD -- \
      scripts/run-upstream-tests.sh \
      scripts/run-port-tests.sh \
      safe/generated/baseline/upstream-test-execution-ledger.json \
      safe/xtask/src/commands/check_headers.rs \
      safe/xtask/src/commands/check_owned_tests.rs \
      safe/xtask/src/commands/check_abi.rs \
      safe/xtask/src/commands/link_compat_smoke.rs | grep -q .
    git diff --cached --exit-code -- \
      scripts/run-upstream-tests.sh \
      scripts/run-port-tests.sh \
      test-original.sh \
      tests/port/dependent-apps \
      tests/port/regressions \
      safe/generated/baseline/dependent-app-test-plan.json \
      safe/generated/baseline/client-app-regressions.json \
      safe/generated/baseline/upstream-test-execution-ledger.json \
      safe/generated/baseline/package-files \
      safe/generated/install-manifests \
      safe/generated/packaging/package-build-manifest.json \
      safe/xtask/src/commands/check_headers.rs \
      safe/xtask/src/commands/check_owned_tests.rs \
      safe/xtask/src/commands/check_abi.rs \
      safe/xtask/src/commands/link_compat_smoke.rs
    git diff --exit-code -- \
      scripts/run-upstream-tests.sh \
      scripts/run-port-tests.sh \
      test-original.sh \
      tests/port/dependent-apps \
      tests/port/regressions \
      safe/generated/baseline/dependent-app-test-plan.json \
      safe/generated/baseline/client-app-regressions.json \
      safe/generated/baseline/upstream-test-execution-ledger.json \
      safe/generated/baseline/package-files \
      safe/generated/install-manifests \
      safe/generated/packaging/package-build-manifest.json \
      safe/xtask/src/commands/check_headers.rs \
      safe/xtask/src/commands/check_owned_tests.rs \
      safe/xtask/src/commands/check_abi.rs \
      safe/xtask/src/commands/link_compat_smoke.rs
    git log --oneline --decorate -n 20
    test -f safe/generated/baseline/dependent-app-test-plan.json
    test -f safe/generated/baseline/client-app-regressions.json
    test -f safe/generated/baseline/upstream-test-execution-ledger.json
    CHANGED_AUTHORITY_FILES="$(mktemp)"
    git diff --name-only "$WORKFLOW_START_REF"..HEAD -- \
      safe/upstream-compat \
      safe/generated/baseline/fallback-c-inventory.json \
      safe/generated/baseline/committed-safe-frontier.txt \
      safe/generated/security/relevant-cves-index.json >"$CHANGED_AUTHORITY_FILES"
    if [ -s "$CHANGED_AUTHORITY_FILES" ]; then
      jq -e '(.issues | length) > 0' safe/generated/baseline/client-app-regressions.json
      while IFS= read -r authority_file; do
        jq --arg authority_file "$authority_file" -e '
          any(.issues[];
            (.status == "fixed") and
            (.impacted_cases | type == "array" and length > 0) and
            (.fix_files | index($authority_file))
          )
        ' safe/generated/baseline/client-app-regressions.json
      done <"$CHANGED_AUTHORITY_FILES"
    fi
    CHANGED_PACKAGE_AUTHORITY_FILES="$(mktemp)"
    git diff --name-only "$WORKFLOW_START_REF"..HEAD -- \
      safe/generated/baseline/package-files \
      safe/generated/install-manifests \
      safe/generated/packaging/package-build-manifest.json >"$CHANGED_PACKAGE_AUTHORITY_FILES"
    if [ -s "$CHANGED_PACKAGE_AUTHORITY_FILES" ]; then
      jq -e '(.issues | length) > 0' safe/generated/baseline/client-app-regressions.json
      while IFS= read -r package_file; do
        jq --arg package_file "$package_file" -e '
          any(.issues[];
            (.status == "fixed") and
            (.impacted_cases | type == "array" and length > 0) and
            (.fix_files | index($package_file))
          )
        ' safe/generated/baseline/client-app-regressions.json
      done <"$CHANGED_PACKAGE_AUTHORITY_FILES"
    fi
    jq -e '.dependents | length == 16' dependents.json
    jq -e '.suites["all-runtime"].cases | length == 16' safe/generated/baseline/dependent-app-test-plan.json
    jq -s -e '([.[0].dependents[].name] | sort) == ([.[1].suites["all-runtime"].cases[]] | sort)' dependents.json safe/generated/baseline/dependent-app-test-plan.json
    jq -e '(.entries | length == 5584) and all(.entries[]; .coverage_status == "executed" or .coverage_status == "equivalent")' safe/generated/baseline/upstream-test-execution-ledger.json
    test -z "$(git ls-files -- safe/work)"
    ```

## Success Criteria

Run all four phase 15 checkers in order. Complete the phase only after source compatibility, link compatibility, runtime compatibility, package installability, client-app behavior, source-build behavior, regressions, safety policy, and validator checks pass.

Every declared New Output must be tracked by git at `HEAD`, and every phase-owned path must have no staged or unstaged changes after the phase commit. The phase succeeds only when the listed verification phases pass with the artifact-preservation and freshness contracts intact.

## Git Commit Requirement

The implementer must commit all phase-owned work to git before yielding. Checkers assume the phase work is committed at `HEAD` and verify path-scoped cleanliness for the phase-owned file set.
