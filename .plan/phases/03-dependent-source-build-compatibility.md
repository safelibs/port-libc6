# Dependent Source-Build Compatibility

## Phase Name

Dependent Source-Build Compatibility

## Implement Phase ID

`impl_13_dependent_source_builds`

## Preexisting Inputs

- All outputs from `impl_12_dependent_runtime_smokes`.
- The unchanged legacy source-build logic in `test-original.sh`: build-dependency shims, `apt-get build-dep`, `apt-get source`, and local build rules for `strace`, `valgrind`, and `libvirt`.
- `dependents.json` entries whose `dependency_modes` include `compile_time_via_glibc_dev`.

## New Outputs

- `tests/port/dependent-apps/source-builds/strace.sh`
- `tests/port/dependent-apps/source-builds/valgrind.sh`
- `tests/port/dependent-apps/source-builds/libvirt.sh`
- `tests/port/dependent-apps/lib/source-build-common.sh`
- Updated `safe/generated/baseline/dependent-app-test-plan.json` source-build section.
- `test-original.sh` updated to call the completed dependent-app harness while preserving the no-argument compatibility entrypoint.

## Derived Workflow Artifacts

- `safe/work/dependent-apps/artifact-snapshots/phase14-inputs/**`, produced by `check_13_freeze_phase14_discovery_inputs` and preserved for phase 14. This is a derived `safe/work/**` artifact, not a committed New Output. No checker should require it to be tracked by git.

## File Changes

- Add source-build scripts under `tests/port/dependent-apps/source-builds/`.
- Move build-dependency shim creation from `test-original.sh` into `tests/port/dependent-apps/lib/source-build-common.sh`.
- Update `tests/port/dependent-apps/run.sh`.
- Update `test-original.sh` wrapper.

## Implementation Details

- Source-build tests must enable Ubuntu source repositories inside the container, install build dependencies, fetch the declared source package, build it, install into a scratch `DESTDIR`, and run a local smoke test against the built binary.
- Preserve the exact three compile-time dependents currently identified in `dependents.json`: `strace`, `valgrind`, and `libvirt`.
- `safe/generated/baseline/dependent-app-test-plan.json` must define both `suites["source-builds"].cases` and `source_builds` entries for exactly `strace`, `valgrind`, and `libvirt`.
- Keep the compatibility shim logic for multilib packages (`libc6-i386`, `libc6-x32`, `libc6-dev-i386`, `libc6-dev-x32`) because `valgrind` build dependencies can require them on amd64.
- Source-build case failures follow the runtime discovery contract. If the container, apt repository, source repository, build-dependency setup, or case harness cannot run, classify it as `failure_kind: "harness"` and fail the suite command. If the package configures, compiles, links, installs, or smokes incorrectly because of safe libc behavior, record `failure_kind: "compatibility_candidate"` and let phase 14 consume it.
- Running the source-build suite may replace only `safe/work/dependent-apps/results/source-builds.json` and `safe/work/dependent-apps/logs/source-builds/`; it must preserve phase 12 result files and logs byte-for-byte.
- The source-build runner must log:
  - `dpkg-query -W libc6 libc6-dev libc-bin`
  - `apt-cache policy libc6 libc6-dev`
  - the exact source package version fetched
  - configure/build/install commands
  - the final smoke command and output
- After the runtime and source-build scripts exist, replace `test-original.sh` with a thin root-level wrapper. With no arguments it must build safe packages, build the dependent image, run `--suite image-contract`, run `--suite all-runtime --privileged`, run `--suite source-builds --privileged`, and exit nonzero if any suite has `summary.failed > 0` or `summary.harness_failed > 0`. It may forward optional arguments to `tests/port/dependent-apps/run.sh`, but no-argument behavior must remain the full dependent compatibility check. The wrapper may replace only `image-contract`, `all-runtime`, and `source-builds` result/log outputs; it must leave the phase 12 split runtime result/log outputs byte-for-byte unchanged.
- Commit the phase before yielding.

## Verification Phases

- `check_13_source_build_dependents`
  - Type: `check`
  - Fixed `bounce_target`: `impl_13_dependent_source_builds`
  - Purpose: Prove source compatibility for the dependent packages in `dependents.json` that build against `libc6-dev`. Requires Docker with network access to fetch Ubuntu source/build dependencies and runs the suite with `--privileged`.
  - Commands:
    ```bash
    SAFE_VERSION="$(jq -r '.safe_package_version' safe/generated/packaging/package-build-manifest.json)"
    SAFE_IMAGE_TAG="$(printf '%s' "$SAFE_VERSION" | sed 's/[^A-Za-z0-9_.-]/-/g')"
    IMAGE="safelibs-libc6-dependent:${SAFE_IMAGE_TAG}"
    for result in \
      safe/work/dependent-apps/results/core-network.json \
      safe/work/dependent-apps/results/server-media-virtualization.json \
      safe/work/dependent-apps/results/diagnostics.json
    do
      test -f "$result"
    done
    mapfile -t PRESERVE_ARTIFACTS < <(
      printf '%s\n' \
        safe/work/dependent-apps/results/core-network.json \
        safe/work/dependent-apps/results/server-media-virtualization.json \
        safe/work/dependent-apps/results/diagnostics.json
      jq -r '.cases[].log' \
        safe/work/dependent-apps/results/core-network.json \
        safe/work/dependent-apps/results/server-media-virtualization.json \
        safe/work/dependent-apps/results/diagnostics.json
    )
    for path in "${PRESERVE_ARTIFACTS[@]}"; do test -f "$path"; done
    PRESERVE_SNAPSHOT="$(mktemp)"
    sha256sum "${PRESERVE_ARTIFACTS[@]}" >"$PRESERVE_SNAPSHOT"
    bash safe/scripts/build-debs.sh
    bash tests/port/dependent-apps/build-image.sh --debs safe/work/debs --manifest dependents.json --tag "$IMAGE"
    sha256sum -c "$PRESERVE_SNAPSHOT"
    rm -f safe/work/dependent-apps/results/source-builds.json
    rm -rf safe/work/dependent-apps/logs/source-builds
    bash tests/port/dependent-apps/run.sh --image "$IMAGE" --suite source-builds --privileged
    sha256sum -c "$PRESERVE_SNAPSHOT"
    RESULT="safe/work/dependent-apps/results/source-builds.json"
    jq -s -e --arg suite "source-builds" --arg safe_version "$SAFE_VERSION" '
      .[0].suites[$suite].cases as $expected |
      .[1] as $result |
      ($result.suite == $suite) and
      (if ($result.summary.failed == 0) then $result.status == "passed" else $result.status == "completed_with_compatibility_candidates" end) and
      ($result.summary.total == ($result.cases | length)) and
      ($result.summary.passed == ([$result.cases[] | select(.status == "passed")] | length)) and
      ($result.summary.failed == ([$result.cases[] | select(.status == "failed")] | length)) and
      ($result.summary.harness_failed == 0) and
      (($result.cases | map(.case) | sort) == ($expected | sort)) and
      (($result.cases | map(.case) | length) == ($result.cases | map(.case) | unique | length)) and
      all($result.cases[];
        (.case | type == "string") and
        (.suite == $suite) and
        (.case_id == ($suite + "/" + .case)) and
        (.status == "passed" or .status == "failed") and
        (.duration_seconds | type == "number") and
        (.log == ("safe/work/dependent-apps/logs/" + $suite + "/" + .case + ".log")) and
        (.safe_version == $safe_version) and
        (.rerun | type == "string" and length > 0) and
        (has("failure_kind")) and
        (if .status == "passed" then .failure_kind == null else .failure_kind == "compatibility_candidate" end)
      )
    ' safe/generated/baseline/dependent-app-test-plan.json "$RESULT"
    jq -r '.cases[] | [.case_id, .case, .log, .safe_version] | @tsv' "$RESULT" |
      while IFS=$'\t' read -r case_id case_name log_path case_version; do
        test -x "tests/port/dependent-apps/source-builds/${case_name}.sh"
        case "$log_path" in safe/work/dependent-apps/logs/source-builds/*) ;; *) exit 1 ;; esac
        test -f "$log_path"
        grep -F "suite=source-builds" "$log_path"
        grep -F "case=${case_name}" "$log_path"
        grep -F "case_id=${case_id}" "$log_path"
        grep -F "safe_version=${case_version}" "$log_path"
      done
    test -z "$(git ls-files -- safe/work)"
    ```

- `check_13_source_build_provenance`
  - Type: `check`
  - Fixed `bounce_target`: `impl_13_dependent_source_builds`
  - Purpose: Ensure source-build tests actually use the safe development packages and do not satisfy build dependencies from Ubuntu archive libc packages.
  - Commands:
    ```bash
    jq -e '
      ((.source_builds | keys | sort) == ["libvirt","strace","valgrind"]) and
      ((.suites["source-builds"].cases | sort) == ["libvirt","strace","valgrind"])
    ' safe/generated/baseline/dependent-app-test-plan.json
    : "${PHASE_BASE_REF:?set to the commit before impl_13_dependent_source_builds began}"
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- original dependents.json .github/workflows/ci-release.yml
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- safe/generated/baseline/package-files safe/generated/install-manifests safe/generated/packaging/package-build-manifest.json
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- all_cves.json relevant_cves.json safe/generated/security/relevant-cves-index.json safe/generated/baseline/fallback-c-inventory.json safe/generated/baseline/committed-safe-frontier.txt safe/upstream-compat
    git diff --cached --exit-code -- original dependents.json .github/workflows/ci-release.yml
    git diff --cached --exit-code -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --cached --exit-code -- safe/generated/baseline/package-files safe/generated/install-manifests safe/generated/packaging/package-build-manifest.json
    git diff --cached --exit-code -- all_cves.json relevant_cves.json safe/generated/security/relevant-cves-index.json safe/generated/baseline/fallback-c-inventory.json safe/generated/baseline/committed-safe-frontier.txt safe/upstream-compat
    git diff --exit-code -- original dependents.json .github/workflows/ci-release.yml
    git diff --exit-code -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --exit-code -- safe/generated/baseline/package-files safe/generated/install-manifests safe/generated/packaging/package-build-manifest.json
    git diff --exit-code -- all_cves.json relevant_cves.json safe/generated/security/relevant-cves-index.json safe/generated/baseline/fallback-c-inventory.json safe/generated/baseline/committed-safe-frontier.txt safe/upstream-compat
    for path in \
      tests/port/dependent-apps/source-builds/strace.sh \
      tests/port/dependent-apps/source-builds/valgrind.sh \
      tests/port/dependent-apps/source-builds/libvirt.sh \
      tests/port/dependent-apps/lib/source-build-common.sh \
      tests/port/dependent-apps/run.sh \
      test-original.sh \
      safe/generated/baseline/dependent-app-test-plan.json
    do
      git ls-files --error-unmatch -- "$path"
    done
    git diff --name-only "$PHASE_BASE_REF"..HEAD -- \
      tests/port/dependent-apps/source-builds \
      tests/port/dependent-apps/lib/source-build-common.sh \
      tests/port/dependent-apps/run.sh \
      test-original.sh \
      safe/generated/baseline/dependent-app-test-plan.json | grep -q .
    git diff --cached --exit-code -- \
      tests/port/dependent-apps/source-builds \
      tests/port/dependent-apps/lib/source-build-common.sh \
      tests/port/dependent-apps/run.sh \
      safe/generated/baseline/dependent-app-test-plan.json \
      test-original.sh
    git diff --exit-code -- \
      tests/port/dependent-apps/source-builds \
      tests/port/dependent-apps/lib/source-build-common.sh \
      tests/port/dependent-apps/run.sh \
      safe/generated/baseline/dependent-app-test-plan.json \
      test-original.sh
    RESULT="safe/work/dependent-apps/results/source-builds.json"
    test -f "$RESULT"
    jq -s -e --arg suite "source-builds" '
      .[0].suites[$suite].cases as $expected |
      .[1] as $result |
      (($result.cases | map(.case) | sort) == ($expected | sort)) and
      (($result.cases | map(.case) | length) == ($result.cases | map(.case) | unique | length)) and
      all($result.cases[];
        (.suite == $suite) and
        (.case_id == ($suite + "/" + .case)) and
        (.log == ("safe/work/dependent-apps/logs/" + $suite + "/" + .case + ".log")) and
        (has("failure_kind")) and
        (if .status == "passed" then .failure_kind == null else .failure_kind == "compatibility_candidate" end)
      )
    ' safe/generated/baseline/dependent-app-test-plan.json "$RESULT"
    jq -r '.cases[] | [.case_id, .case, .log, .safe_version] | @tsv' "$RESULT" |
      while IFS=$'\t' read -r case_id case_name log_path case_version; do
        test -x "tests/port/dependent-apps/source-builds/${case_name}.sh"
        case "$log_path" in safe/work/dependent-apps/logs/source-builds/*) ;; *) exit 1 ;; esac
        test -f "$log_path"
        grep -F "suite=source-builds" "$log_path"
        grep -F "case=${case_name}" "$log_path"
        grep -F "case_id=${case_id}" "$log_path"
        grep -F "safe_version=${case_version}" "$log_path"
        grep -F "selected libc6-dev" "$log_path"
        grep -F "file:/tmp/safelibs-apt-repo" "$log_path"
        grep -F "apt-cache policy libc6 libc6-dev" "$log_path"
      done
    find safe/work/dependent-apps/logs/source-builds -type f -name '*.log' | wc -l | awk '{ exit !($1 == 3) }'
    test -z "$(git ls-files -- safe/work)"
    ```

- `check_13_legacy_wrapper_no_argument_behavior`
  - Type: `check`
  - Fixed `bounce_target`: `impl_13_dependent_source_builds`
  - Purpose: Verify the phase-owned `test-original.sh` rewrite in the same phase. Requires Docker with network access; wrapper all-runtime and source-build suites run with `--privileged`. Allows discovery-mode compatibility candidates, but requires fresh wrapper-owned artifacts, zero harness failures, aggregate all-runtime failures matching the already-discovered phase-12 runtime case names, source-build failures matching the pre-wrapper source-build discovery result, and strict wrapper exit semantics.
  - Commands:
    ```bash
    for result in \
      safe/work/dependent-apps/results/core-network.json \
      safe/work/dependent-apps/results/server-media-virtualization.json \
      safe/work/dependent-apps/results/diagnostics.json \
      safe/work/dependent-apps/results/source-builds.json
    do
      test -f "$result"
    done
    mapfile -t PRESERVE_ARTIFACTS < <(
      printf '%s\n' \
        safe/work/dependent-apps/results/core-network.json \
        safe/work/dependent-apps/results/server-media-virtualization.json \
        safe/work/dependent-apps/results/diagnostics.json
      jq -r '.cases[].log' \
        safe/work/dependent-apps/results/core-network.json \
        safe/work/dependent-apps/results/server-media-virtualization.json \
        safe/work/dependent-apps/results/diagnostics.json
    )
    for path in "${PRESERVE_ARTIFACTS[@]}"; do test -f "$path"; done
    PRESERVE_SNAPSHOT="$(mktemp)"
    sha256sum "${PRESERVE_ARTIFACTS[@]}" >"$PRESERVE_SNAPSHOT"
    RUNTIME_DISCOVERED_CASES="$(mktemp)"
    jq -s -r '
      [
        .[] |
        .cases[] |
        select(.status == "failed" and has("failure_kind") and .failure_kind == "compatibility_candidate") |
        .case
      ] | sort | unique | .[]
    ' \
      safe/work/dependent-apps/results/core-network.json \
      safe/work/dependent-apps/results/server-media-virtualization.json \
      safe/work/dependent-apps/results/diagnostics.json >"$RUNTIME_DISCOVERED_CASES"
    SOURCE_BUILD_DISCOVERED_CASES="$(mktemp)"
    jq -r '
      [
        .cases[] |
        select(.status == "failed" and has("failure_kind") and .failure_kind == "compatibility_candidate") |
        .case
      ] | sort | unique | .[]
    ' safe/work/dependent-apps/results/source-builds.json >"$SOURCE_BUILD_DISCOVERED_CASES"
    bash -n test-original.sh
    rm -f \
      safe/work/dependent-apps/results/image-contract.json \
      safe/work/dependent-apps/results/all-runtime.json \
      safe/work/dependent-apps/results/source-builds.json
    rm -rf \
      safe/work/dependent-apps/logs/image-contract \
      safe/work/dependent-apps/logs/all-runtime \
      safe/work/dependent-apps/logs/source-builds
    WRAPPER_STATUS=0
    bash test-original.sh || WRAPPER_STATUS=$?
    sha256sum -c "$PRESERVE_SNAPSHOT"
    SAFE_VERSION="$(jq -r '.safe_package_version' safe/generated/packaging/package-build-manifest.json)"
    for suite in image-contract all-runtime source-builds; do
      result="safe/work/dependent-apps/results/${suite}.json"
      test -f "$result"
      test -d "safe/work/dependent-apps/logs/${suite}"
    done
    jq -s -e --arg suite "image-contract" --arg safe_version "$SAFE_VERSION" '
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
    ' safe/generated/baseline/dependent-app-test-plan.json safe/work/dependent-apps/results/image-contract.json
    for suite in all-runtime source-builds; do
      result="safe/work/dependent-apps/results/${suite}.json"
      jq -s -e --arg suite "$suite" --arg safe_version "$SAFE_VERSION" '
        .[0].suites[$suite].cases as $expected |
        .[1] as $result |
        ($result.suite == $suite) and
        (if ($result.summary.failed == 0) then $result.status == "passed" else $result.status == "completed_with_compatibility_candidates" end) and
        ($result.summary.total == ($expected | length)) and
        ($result.summary.total == ($result.cases | length)) and
        ($result.summary.passed == ([$result.cases[] | select(.status == "passed")] | length)) and
        ($result.summary.failed == ([$result.cases[] | select(.status == "failed")] | length)) and
        ($result.summary.harness_failed == 0) and
        (($result.cases | map(.case) | sort) == ($expected | sort)) and
        (($result.cases | map(.case) | length) == ($result.cases | map(.case) | unique | length)) and
        all($result.cases[];
          (.case | type == "string") and
          (.suite == $suite) and
          (.case_id == ($suite + "/" + .case)) and
          (.status == "passed" or .status == "failed") and
          (.duration_seconds | type == "number") and
          (.log == ("safe/work/dependent-apps/logs/" + $suite + "/" + .case + ".log")) and
          (.safe_version == $safe_version) and
          (.rerun | type == "string" and length > 0) and
          (has("failure_kind")) and
          (if .status == "passed" then .failure_kind == null else .failure_kind == "compatibility_candidate" end)
        )
      ' safe/generated/baseline/dependent-app-test-plan.json "$result"
    done
    jq -s -e '[.[].summary.harness_failed] | add == 0' \
      safe/work/dependent-apps/results/image-contract.json \
      safe/work/dependent-apps/results/all-runtime.json \
      safe/work/dependent-apps/results/source-builds.json
    WRAPPER_RUNTIME_CASES="$(mktemp)"
    WRAPPER_SOURCE_BUILD_CASES="$(mktemp)"
    jq -r '
      [
        .cases[] |
        select(.status == "failed" and has("failure_kind") and .failure_kind == "compatibility_candidate") |
        .case
      ] | sort | unique | .[]
    ' safe/work/dependent-apps/results/all-runtime.json >"$WRAPPER_RUNTIME_CASES"
    diff -u "$RUNTIME_DISCOVERED_CASES" "$WRAPPER_RUNTIME_CASES"
    jq -r '
      [
        .cases[] |
        select(.status == "failed" and has("failure_kind") and .failure_kind == "compatibility_candidate") |
        .case
      ] | sort | unique | .[]
    ' safe/work/dependent-apps/results/source-builds.json >"$WRAPPER_SOURCE_BUILD_CASES"
    diff -u "$SOURCE_BUILD_DISCOVERED_CASES" "$WRAPPER_SOURCE_BUILD_CASES"
    FAILED_TOTAL="$(jq -s '[.[].summary.failed] | add' \
      safe/work/dependent-apps/results/image-contract.json \
      safe/work/dependent-apps/results/all-runtime.json \
      safe/work/dependent-apps/results/source-builds.json)"
    if [ "$FAILED_TOTAL" -eq 0 ]; then
      test "$WRAPPER_STATUS" -eq 0
    else
      test "$WRAPPER_STATUS" -ne 0
    fi
    for suite in image-contract all-runtime source-builds; do
      result="safe/work/dependent-apps/results/${suite}.json"
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

- `check_13_freeze_phase14_discovery_inputs`
  - Type: `check`
  - Fixed `bounce_target`: `impl_13_dependent_source_builds`
  - Purpose: Freeze the phase-12 and phase-13 discovery artifacts consumed by phase 14 so untracked `safe/work/**` results and logs cannot be silently replaced across the phase-14 boundary. Does not require Docker or `--privileged`; hashes only artifacts produced by earlier phase-12 and phase-13 checkers.
  - Commands:
    ```bash
    SNAPSHOT_DIR="safe/work/dependent-apps/artifact-snapshots/phase14-inputs"
    rm -rf "$SNAPSHOT_DIR"
    mkdir -p "$SNAPSHOT_DIR"
    cat >"$SNAPSHOT_DIR/result-files.txt" <<'EOF'
safe/work/dependent-apps/results/core-network.json
safe/work/dependent-apps/results/server-media-virtualization.json
safe/work/dependent-apps/results/diagnostics.json
safe/work/dependent-apps/results/source-builds.json
EOF
    while IFS= read -r result; do
      test -f "$result"
      jq -e '
        (.cases | type == "array") and
        (.summary.harness_failed == 0) and
        all(.cases[];
          (.case_id | type == "string" and contains("/")) and
          (.suite | type == "string") and
          (.case | type == "string") and
          (.log | type == "string") and
          (.safe_version | type == "string" and length > 0) and
          (has("failure_kind")) and
          (.status == "passed" or .status == "failed") and
          (if .status == "passed" then .failure_kind == null else .failure_kind == "compatibility_candidate" end)
        )
      ' "$result"
    done <"$SNAPSHOT_DIR/result-files.txt"
    {
      cat "$SNAPSHOT_DIR/result-files.txt"
      jq -r '.cases[].log' $(cat "$SNAPSHOT_DIR/result-files.txt")
    } | sort -u >"$SNAPSHOT_DIR/artifact-files.txt"
    while IFS= read -r artifact; do
      case "$artifact" in
        safe/work/dependent-apps/results/*.json|safe/work/dependent-apps/logs/*/*.log) ;;
        *) exit 1 ;;
      esac
      test -f "$artifact"
    done <"$SNAPSHOT_DIR/artifact-files.txt"
    xargs -r sha256sum <"$SNAPSHOT_DIR/artifact-files.txt" >"$SNAPSHOT_DIR/sha256sums.txt"
    jq -s -r '
      [
        .[] |
        .cases[] |
        select(.status == "failed" and has("failure_kind") and .failure_kind == "compatibility_candidate") |
        .case_id
      ] | sort | unique | .[]
    ' $(cat "$SNAPSHOT_DIR/result-files.txt") >"$SNAPSHOT_DIR/compatibility-candidates.txt"
    ARTIFACT_COUNT="$(wc -l <"$SNAPSHOT_DIR/artifact-files.txt" | tr -d ' ')"
    CANDIDATE_COUNT="$(wc -l <"$SNAPSHOT_DIR/compatibility-candidates.txt" | tr -d ' ')"
    jq -n \
      --arg head "$(git rev-parse HEAD)" \
      --arg created_by "check_13_freeze_phase14_discovery_inputs" \
      --argjson artifact_count "$ARTIFACT_COUNT" \
      --argjson candidate_count "$CANDIDATE_COUNT" \
      '{
        schema_version: 1,
        head: $head,
        created_by: $created_by,
        result_files_path: "safe/work/dependent-apps/artifact-snapshots/phase14-inputs/result-files.txt",
        artifact_files_path: "safe/work/dependent-apps/artifact-snapshots/phase14-inputs/artifact-files.txt",
        sha256sums_path: "safe/work/dependent-apps/artifact-snapshots/phase14-inputs/sha256sums.txt",
        compatibility_candidates_path: "safe/work/dependent-apps/artifact-snapshots/phase14-inputs/compatibility-candidates.txt",
        artifact_count: $artifact_count,
        candidate_count: $candidate_count
      }' >"$SNAPSHOT_DIR/summary.json"
    sha256sum -c "$SNAPSHOT_DIR/sha256sums.txt"
    jq -e '
      (.schema_version == 1) and
      (.created_by == "check_13_freeze_phase14_discovery_inputs") and
      (.artifact_count >= 20) and
      (.candidate_count >= 0) and
      (.head | type == "string" and length == 40)
    ' "$SNAPSHOT_DIR/summary.json"
    CURRENT_CANDIDATES="$(mktemp)"
    jq -s -r '
      [
        .[] |
        .cases[] |
        select(.status == "failed" and has("failure_kind") and .failure_kind == "compatibility_candidate") |
        .case_id
      ] | sort | unique | .[]
    ' $(cat "$SNAPSHOT_DIR/result-files.txt") >"$CURRENT_CANDIDATES"
    diff -u "$SNAPSHOT_DIR/compatibility-candidates.txt" "$CURRENT_CANDIDATES"
    test -z "$(git ls-files -- safe/work)"
    ```

## Success Criteria

Run all four phase 13 checkers. Accept complete source-build provenance, fresh no-argument wrapper artifacts with strict exit semantics, aggregate runtime failures already present in the phase-12 split runtime results, wrapper source-build candidate names matching the pre-wrapper discovery result, and source-compatibility failures recorded as `compatibility_candidate`. The final checker must produce the phase-14 discovery snapshot from existing split runtime and source-build artifacts without rerunning or mutating those suites. Fail only for wrapper, harness, infrastructure, provenance, aggregate-suite mismatch, candidate-set mismatch, snapshot freshness, or malformed-artifact problems. Safe-libc-caused source-build or split-runtime failures become phase 14 regression issues.

Every declared New Output must be tracked by git at `HEAD`, and every phase-owned path must have no staged or unstaged changes after the phase commit. The phase succeeds only when the listed verification phases pass with the artifact-preservation and freshness contracts intact.

## Git Commit Requirement

The implementer must commit all phase-owned work to git before yielding. Checkers assume the phase work is committed at `HEAD` and verify path-scoped cleanliness for the phase-owned file set.
