# Client Regression Fixes

## Phase Name

Client Regression Fixes

## Implement Phase ID

`impl_14_client_regression_fixes`

## Preexisting Inputs

- Existing phase 12 runtime result JSON files and logs: `safe/work/dependent-apps/results/core-network.json`, `safe/work/dependent-apps/results/server-media-virtualization.json`, `safe/work/dependent-apps/results/diagnostics.json`, and `safe/work/dependent-apps/logs/**`.
- Existing phase 13 source-build result JSON file and logs: `safe/work/dependent-apps/results/source-builds.json` and `safe/work/dependent-apps/logs/source-builds/**`.
- Frozen phase-14 discovery snapshot produced by `check_13_freeze_phase14_discovery_inputs`: `safe/work/dependent-apps/artifact-snapshots/phase14-inputs/result-files.txt`, `artifact-files.txt`, `sha256sums.txt`, `compatibility-candidates.txt`, and `summary.json`.
- The phase-13 `test-original.sh` wrapper, which is a preexisting input in this phase and must not be edited.
- All existing SafeLibs verification artifacts under `safe/generated/**`.
- Rust implementation files under `safe/crates/**`.
- Package manifests and Debian assets under `safe/generated/baseline/package-files/*.json`, `safe/generated/install-manifests/*.json`, and `safe/debian/**`.

## New Outputs

- `safe/generated/baseline/client-app-regressions.json`
- `tests/port/regressions/run.sh`
- For each non-empty regression issue, either a tracked executable `tests/port/regressions/<issue-id>.sh` reproducer or a tracked existing or phase-updated `safe/tests/**` upstream-test reproducer referenced by the ledger. No per-issue `tests/port/regressions/<issue-id>.sh` script is required when the ledger has zero issues or when `safe/tests/**` covers the issue.
- Optional tracked fixtures under `tests/port/regressions/fixtures/**` when a regression reproducer needs fixture data.
- Updated `tests/port/dependent-apps/run.sh` impacted-suite handling.
- Targeted fixes in `safe/crates/**`, `safe/xtask/**`, `safe/debian/**`, or generated manifests when required.
- Updated `safe/upstream-compat/safety-policy.toml`, `safe/upstream-compat/package-scope.toml`, `safe/upstream-compat/cve-status.toml`, `safe/upstream-compat/port-status.toml`, `safe/generated/baseline/fallback-c-inventory.json`, `safe/generated/baseline/committed-safe-frontier.txt`, and `safe/generated/security/relevant-cves-index.json` if a fix changes unsafe boundaries, fallback classifications, package scope, safe-frontier status, or CVE disposition. Any changed path in this authority set must be listed in `fix_files` for a fixed issue in `safe/generated/baseline/client-app-regressions.json`; root `all_cves.json` and `relevant_cves.json` must not be edited.

## File Changes

- Add `tests/port/regressions/**`.
- Add `safe/generated/baseline/client-app-regressions.json`.
- Update `tests/port/dependent-apps/run.sh` only to add deterministic `impacted` suite resolution from the regression ledger.
- Add or update `safe/tests/**` only when the regression ledger uses an upstream-test reproducer instead of a `tests/port/regressions/<issue-id>.sh` script.
- Update `safe/upstream-compat/*.toml`, `safe/generated/baseline/fallback-c-inventory.json`, `safe/generated/baseline/committed-safe-frontier.txt`, or `safe/generated/security/relevant-cves-index.json` only when a regression fix changes those contracts and the exact path is listed in an issue `fix_files` entry.
- Modify only the implementation or packaging files required by the discovered regressions.
- Do not edit `test-original.sh`; phase 13 owns the wrapper rewrite and phase 14 must preserve it.
- Keep unrelated refactors out of this phase.

## Implementation Details

- Do not rerun full phase 12 or phase 13 suites to discover failures. First assert that the four result files listed in Preexisting Inputs, their logs, and the frozen phase-14 discovery snapshot exist. If any are missing, stop before changes with a clear precondition failure naming the missing file; the phase is not ready until that previous-phase output exists.
- Before reading any discovery result, validate `safe/work/dependent-apps/artifact-snapshots/phase14-inputs/sha256sums.txt` with `sha256sum -c`, verify `summary.json.head` equals the phase-14 `PHASE_BASE_REF`, and recompute the current compatibility-candidate set from the frozen result files to prove it still matches `compatibility-candidates.txt`.
- Treat `safe/work/dependent-apps/artifact-snapshots/phase14-inputs/compatibility-candidates.txt` as the authoritative discovered failure set: the exact sorted unique set of canonical case IDs from `core-network.json`, `server-media-virtualization.json`, `diagnostics.json`, and `source-builds.json` as frozen after phase 13. The phase-14 implementor may inspect frozen result JSON and logs for diagnosis, but must not edit, truncate, delete, rerun, or regenerate any frozen result, log, or snapshot file.
- For every compatibility failure:
  - Record an issue in `safe/generated/baseline/client-app-regressions.json` with `id`, `affected_apps`, `impacted_cases`, `observed_failure`, `root_cause`, `reproducer`, `fix_files`, and `status`. `impacted_cases` must contain the canonical `<suite>/<case>` IDs from the phase 12 and phase 13 result files.
  - Every `fix_files[]` entry must be a repository-relative path, must not be absolute, must not traverse through `..`, must be tracked by git at the phase-14 HEAD, and must exist in the working tree.
  - Create a minimal regression test before or with the fix. Prefer a small C/Rust shell test under `tests/port/regressions/` that directly exercises the public libc API. If an upstream glibc test already represents the issue, add or extend the corresponding tracked `safe/tests/**` port and reference that exact repository-relative path in the ledger.
  - Fix the smallest safe implementation or packaging surface needed.
  - Add Rust unit tests for pure Rust parser or data-structure fixes.
  - If a fix changes `safe/generated/baseline/package-files/*.json`, `safe/generated/install-manifests/*.json`, or `safe/generated/packaging/package-build-manifest.json`, include every changed manifest path in that issue's `fix_files`; package authority changes without a fixed issue and concrete impacted case are not allowed.
  - Update safety and package-scope ledgers if unsafe code, fallback assets, package provenance, safe-frontier status, or CVE status changes. When changing `safe/upstream-compat/*.toml`, `safe/generated/baseline/fallback-c-inventory.json`, `safe/generated/baseline/committed-safe-frontier.txt`, or `safe/generated/security/relevant-cves-index.json`, include the exact changed repository-relative path in that issue's `fix_files`. Do not edit root CVE source files `all_cves.json` or `relevant_cves.json`.
- Implement `tests/port/regressions/run.sh` as a ledger-driven runner. It must read `safe/generated/baseline/client-app-regressions.json` and execute every `.issues[].reproducer` exactly once per issue:
  - For `tests/port/regressions/*.sh`, run the executable script from the repository root.
  - For `safe/tests/**`, resolve the path through `safe/tests/manifest.toml` by matching either `safe_path` or an entry in `support_paths`, collect the corresponding `catalog_id` values, and run `cargo run -p xtask -- run-original-tests --root work/install-root --build-root work/original-build --tests <catalog_id> --privileged-container-tests` from `safe/`. If no catalog entry maps to the reproducer path, the runner must fail.
  - Write `safe/work/regressions/results.json` on every run with `schema_version: 1`, `summary.total`, `summary.passed`, `summary.failed`, and a `results` array. Each result row must include `issue_id`, `reproducer`, `runner_kind` (`port-regression` or `safe-tests`), `status`, `duration_seconds`, and `executed_command`. Zero ledger issues must produce `summary.total == 0`, `summary.failed == 0`, and `results == []`.
  - Exit zero only when every reproducer passes. A reproducer path that exists but is not executed is a checker failure.
- After fixes, rerun only `tests/port/regressions/run.sh` and impacted app cases derived from the ledger. Before each regression rerun, delete `safe/work/regressions/results.json` and require the runner to recreate it. Before each impacted-suite rerun, delete `safe/work/dependent-apps/results/impacted.json` and `safe/work/dependent-apps/logs/impacted/` and require the suite to recreate the result file and selected-case logs. Do not rerun unrelated client-app suites in this phase.
- Define `tests/port/dependent-apps/run.sh --suite impacted` to read `safe/generated/baseline/client-app-regressions.json`, collect the unique `.issues[].impacted_cases[]` values, and run exactly those canonical `<suite>/<case>` IDs. Each impacted result case must include the same `case_id` value from the ledger, set `suite` to `impacted`, set `case` to the canonical `case_id` with `/` replaced by `__`, and write its log to `safe/work/dependent-apps/logs/impacted/<case>.log`. Each impacted log must include `suite=impacted`, `case=<case>`, `case_id=<original-suite>/<original-case>`, and `safe_version=<safe_version>`. Each non-empty issue must provide at least one `impacted_cases` entry. If the ledger has zero issues, the runner must emit `safe/work/dependent-apps/results/impacted.json` with `summary.total == 0`, `summary.failed == 0`, `summary.harness_failed == 0`, `cases == []`, and `status == "passed"` without requiring Docker work inside the image.
- Always commit `client-app-regressions.json` with `schema_version: 1`, an `issues` array, and a `validated_suites` section containing `phase12_results` exactly equal to `["safe/work/dependent-apps/results/core-network.json", "safe/work/dependent-apps/results/server-media-virtualization.json", "safe/work/dependent-apps/results/diagnostics.json"]`, `phase13_results` exactly equal to `["safe/work/dependent-apps/results/source-builds.json"]`, and `phase14_discovery_snapshot` with `snapshot_dir`, `result_files`, `artifact_sha256s`, `candidate_sha256`, `head`, and `created_by` copied from the frozen snapshot. The union of all `.issues[].impacted_cases[]` must exactly equal the candidate IDs in `safe/work/dependent-apps/artifact-snapshots/phase14-inputs/compatibility-candidates.txt`, with no missing, extra, or duplicated case IDs. If no compatibility failures are found, `issues` must be an empty array; `issues: []` is valid only when the frozen discovered failure set is empty.
- Commit the phase before yielding.

## Verification Phases

- `check_14_regression_reproducers`
  - Type: `check`
  - Fixed `bounce_target`: `impl_14_client_regression_fixes`
  - Purpose: Prove every discovered app failure has a committed minimal reproducer and that the reproducer passes with the fix.
  - Commands:
    ```bash
    : "${PHASE_BASE_REF:?set to the commit before impl_14_client_regression_fixes began}"
    SNAPSHOT_DIR="safe/work/dependent-apps/artifact-snapshots/phase14-inputs"
    validate_phase14_discovery_snapshot() {
      test -d "$SNAPSHOT_DIR"
      test -f "$SNAPSHOT_DIR/result-files.txt"
      test -f "$SNAPSHOT_DIR/artifact-files.txt"
      test -f "$SNAPSHOT_DIR/sha256sums.txt"
      test -f "$SNAPSHOT_DIR/compatibility-candidates.txt"
      test -f "$SNAPSHOT_DIR/summary.json"
      EXPECTED_RESULT_FILES="$(mktemp)"
      cat >"$EXPECTED_RESULT_FILES" <<'EOF'
safe/work/dependent-apps/results/core-network.json
safe/work/dependent-apps/results/server-media-virtualization.json
safe/work/dependent-apps/results/diagnostics.json
safe/work/dependent-apps/results/source-builds.json
EOF
      diff -u "$EXPECTED_RESULT_FILES" "$SNAPSHOT_DIR/result-files.txt"
      EXPECTED_ARTIFACTS="$(mktemp)"
      {
        cat "$SNAPSHOT_DIR/result-files.txt"
        jq -r '.cases[].log' $(cat "$SNAPSHOT_DIR/result-files.txt")
      } | sort -u >"$EXPECTED_ARTIFACTS"
      diff -u "$EXPECTED_ARTIFACTS" "$SNAPSHOT_DIR/artifact-files.txt"
      test "$(jq -r '.created_by' "$SNAPSHOT_DIR/summary.json")" = "check_13_freeze_phase14_discovery_inputs"
      test "$(jq -r '.head' "$SNAPSHOT_DIR/summary.json")" = "$(git rev-parse "$PHASE_BASE_REF")"
      test "$(jq -r '.artifact_count' "$SNAPSHOT_DIR/summary.json")" = "$(wc -l <"$SNAPSHOT_DIR/artifact-files.txt" | tr -d ' ')"
      test "$(jq -r '.candidate_count' "$SNAPSHOT_DIR/summary.json")" = "$(wc -l <"$SNAPSHOT_DIR/compatibility-candidates.txt" | tr -d ' ')"
      sha256sum -c "$SNAPSHOT_DIR/sha256sums.txt"
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
    }
    validate_phase14_discovery_snapshot
    for result in \
      safe/work/dependent-apps/results/core-network.json \
      safe/work/dependent-apps/results/server-media-virtualization.json \
      safe/work/dependent-apps/results/diagnostics.json \
      safe/work/dependent-apps/results/source-builds.json
    do
      test -f "$result"
      jq -e '
        (.cases | type == "array") and
        all(.cases[];
          (has("failure_kind")) and
          (.status == "passed" or .status == "failed") and
          (if .status == "passed" then .failure_kind == null else .failure_kind == "compatibility_candidate" end)
        )
      ' "$result"
      jq -r '.cases[] | [.suite, .case_id, .case, .log, .safe_version] | @tsv' "$result" |
        while IFS=$'\t' read -r suite case_id case_name log_path case_version; do
          case "$log_path" in safe/work/dependent-apps/logs/*) ;; *) exit 1 ;; esac
          test -f "$log_path"
          grep -F "suite=${suite}" "$log_path"
          grep -F "case=${case_name}" "$log_path"
          grep -F "case_id=${case_id}" "$log_path"
          grep -F "safe_version=${case_version}" "$log_path"
        done
    done
    test -f safe/generated/baseline/client-app-regressions.json
    git ls-files --error-unmatch safe/generated/baseline/client-app-regressions.json
    git ls-files --error-unmatch tests/port/regressions/run.sh
    test -x tests/port/regressions/run.sh
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- original dependents.json .github/workflows/ci-release.yml test-original.sh
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- all_cves.json relevant_cves.json
    git diff --cached --exit-code -- original dependents.json .github/workflows/ci-release.yml test-original.sh
    git diff --cached --exit-code -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --cached --exit-code -- all_cves.json relevant_cves.json
    git diff --exit-code -- original dependents.json .github/workflows/ci-release.yml test-original.sh
    git diff --exit-code -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --exit-code -- all_cves.json relevant_cves.json
    git diff --name-only "$PHASE_BASE_REF"..HEAD -- \
      safe/generated/baseline/client-app-regressions.json \
      tests/port/regressions \
      tests/port/dependent-apps/run.sh \
      safe/tests \
      safe/crates \
      safe/xtask \
      safe/debian \
      safe/upstream-compat \
      safe/generated/baseline/fallback-c-inventory.json \
      safe/generated/baseline/committed-safe-frontier.txt \
      safe/generated/security/relevant-cves-index.json \
      safe/generated/baseline/package-files \
      safe/generated/install-manifests \
      safe/generated/packaging/package-build-manifest.json | grep -q .
    git diff --cached --exit-code -- \
      safe/generated/baseline/client-app-regressions.json \
      tests/port/regressions \
      tests/port/dependent-apps/run.sh \
      safe/tests \
      safe/crates \
      safe/xtask \
      safe/debian \
      safe/upstream-compat \
      safe/generated/baseline/fallback-c-inventory.json \
      safe/generated/baseline/committed-safe-frontier.txt \
      safe/generated/security/relevant-cves-index.json \
      safe/generated/baseline/package-files \
      safe/generated/install-manifests \
      safe/generated/packaging/package-build-manifest.json
    git diff --exit-code -- \
      safe/generated/baseline/client-app-regressions.json \
      tests/port/regressions \
      tests/port/dependent-apps/run.sh \
      safe/tests \
      safe/crates \
      safe/xtask \
      safe/debian \
      safe/upstream-compat \
      safe/generated/baseline/fallback-c-inventory.json \
      safe/generated/baseline/committed-safe-frontier.txt \
      safe/generated/security/relevant-cves-index.json \
      safe/generated/baseline/package-files \
      safe/generated/install-manifests \
      safe/generated/packaging/package-build-manifest.json
    find tests/port/regressions -type f -print |
      while IFS= read -r path; do
        git ls-files --error-unmatch -- "$path"
      done
    SNAPSHOT_HEAD="$(git rev-parse "$PHASE_BASE_REF")"
    jq --arg snapshot_head "$SNAPSHOT_HEAD" -e '
      (.schema_version == 1) and
      (.issues | type == "array") and
      (.validated_suites.phase12_results == [
        "safe/work/dependent-apps/results/core-network.json",
        "safe/work/dependent-apps/results/server-media-virtualization.json",
        "safe/work/dependent-apps/results/diagnostics.json"
      ]) and
      (.validated_suites.phase13_results == [
        "safe/work/dependent-apps/results/source-builds.json"
      ]) and
      (.validated_suites.phase14_discovery_snapshot.snapshot_dir == "safe/work/dependent-apps/artifact-snapshots/phase14-inputs") and
      (.validated_suites.phase14_discovery_snapshot.candidate_sha256 | test("^[0-9a-f]{64}$")) and
      (.validated_suites.phase14_discovery_snapshot.artifact_sha256s == "safe/work/dependent-apps/artifact-snapshots/phase14-inputs/sha256sums.txt") and
      (.validated_suites.phase14_discovery_snapshot.head == $snapshot_head) and
      (.validated_suites.phase14_discovery_snapshot.created_by == "check_13_freeze_phase14_discovery_inputs") and
      (.validated_suites.phase14_discovery_snapshot.result_files == [
        "safe/work/dependent-apps/results/core-network.json",
        "safe/work/dependent-apps/results/server-media-virtualization.json",
        "safe/work/dependent-apps/results/diagnostics.json",
        "safe/work/dependent-apps/results/source-builds.json"
      ]) and
      (
        ((.issues | length) == 0)
        or
        all(.issues[];
          (.id | type == "string" and length > 0) and
          (.status == "fixed") and
          (.reproducer | type == "string" and length > 0) and
          (.affected_apps | type == "array" and length >= 1) and
          (.impacted_cases | type == "array" and length >= 1) and
          (.observed_failure | type == "string" and length > 0) and
          (.root_cause | type == "string" and length > 0) and
          (.fix_files | type == "array" and length >= 1)
        )
      )
    ' safe/generated/baseline/client-app-regressions.json
    SNAPSHOT_CANDIDATE_SHA="$(sha256sum "$SNAPSHOT_DIR/compatibility-candidates.txt" | awk '{print $1}')"
    jq --arg sha "$SNAPSHOT_CANDIDATE_SHA" -e \
      '.validated_suites.phase14_discovery_snapshot.candidate_sha256 == $sha' \
      safe/generated/baseline/client-app-regressions.json
    EXPECTED_CASES="$(mktemp)"
    ACTUAL_CASES="$(mktemp)"
    cp "$SNAPSHOT_DIR/compatibility-candidates.txt" "$EXPECTED_CASES"
    jq -r '[.issues[].impacted_cases[]] | sort | unique | .[]' \
      safe/generated/baseline/client-app-regressions.json >"$ACTUAL_CASES"
    diff -u "$EXPECTED_CASES" "$ACTUAL_CASES"
    EXPECTED_COUNT="$(wc -l <"$EXPECTED_CASES" | tr -d ' ')"
    if [ "$EXPECTED_COUNT" -eq 0 ]; then
      jq -e '.issues == []' safe/generated/baseline/client-app-regressions.json
    else
      jq -e '
        (.issues | length > 0) and
        ([.issues[].impacted_cases[]] as $cases | ($cases | length) == ($cases | unique | length))
      ' safe/generated/baseline/client-app-regressions.json
    fi
    jq -r '.issues[].fix_files[]' safe/generated/baseline/client-app-regressions.json |
      while IFS= read -r fix_file; do
        case "$fix_file" in
          ""|/*|../*|*/../*) exit 1 ;;
        esac
        git ls-files --error-unmatch -- "$fix_file"
        test -e "$fix_file"
      done
    CHANGED_AUTHORITY_FILES="$(mktemp)"
    git diff --name-only "$PHASE_BASE_REF"..HEAD -- \
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
    git diff --name-only "$PHASE_BASE_REF"..HEAD -- \
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
    jq -r '.issues[].reproducer' safe/generated/baseline/client-app-regressions.json |
      while IFS= read -r reproducer; do
        case "$reproducer" in
          tests/port/regressions/*.sh)
            git ls-files --error-unmatch "$reproducer"
            test -x "$reproducer"
            ;;
          safe/tests/*)
            git ls-files --error-unmatch "$reproducer"
            test -f "$reproducer"
            ;;
          *)
            exit 1
            ;;
        esac
      done
    rm -f safe/work/regressions/results.json
    bash tests/port/regressions/run.sh
    validate_phase14_discovery_snapshot
    REGRESSION_RESULT="safe/work/regressions/results.json"
    test -f "$REGRESSION_RESULT"
    jq -s -e '
      .[0].issues as $issues |
      .[1] as $run |
      ($run.schema_version == 1) and
      ($run.summary.total == ($issues | length)) and
      ($run.summary.failed == 0) and
      ($run.results | type == "array") and
      (($run.results | length) == ($issues | length)) and
      (([$run.results[].issue_id] | sort) == ([$issues[].id] | sort)) and
      all($issues[];
        . as $issue |
        any($run.results[];
          (.issue_id == $issue.id) and
          (.reproducer == $issue.reproducer) and
          (.status == "passed") and
          (.executed_command | type == "string" and length > 0) and
          (if ($issue.reproducer | startswith("safe/tests/"))
           then (.runner_kind == "safe-tests") and (.executed_command | contains("run-original-tests"))
           else (.runner_kind == "port-regression") and (.executed_command | contains($issue.reproducer))
           end)
        )
      )
    ' safe/generated/baseline/client-app-regressions.json "$REGRESSION_RESULT"
    test -z "$(git ls-files -- safe/work)"
    ```

- `check_14_software_tester_regression_suite`
  - Type: `check`
  - Fixed `bounce_target`: `impl_14_client_regression_fixes`
  - Purpose: Review fixes as a software tester: run impacted app cases, run the regression suite, and ensure failure logs explain the fix. Requires Docker with network access and runs impacted app cases with `--privileged` only when the regression ledger has impacted cases.
  - Commands:
    ```bash
    : "${PHASE_BASE_REF:?set to the commit before impl_14_client_regression_fixes began}"
    SNAPSHOT_DIR="safe/work/dependent-apps/artifact-snapshots/phase14-inputs"
    validate_phase14_discovery_snapshot() {
      test -d "$SNAPSHOT_DIR"
      test -f "$SNAPSHOT_DIR/result-files.txt"
      test -f "$SNAPSHOT_DIR/artifact-files.txt"
      test -f "$SNAPSHOT_DIR/sha256sums.txt"
      test -f "$SNAPSHOT_DIR/compatibility-candidates.txt"
      test -f "$SNAPSHOT_DIR/summary.json"
      EXPECTED_RESULT_FILES="$(mktemp)"
      cat >"$EXPECTED_RESULT_FILES" <<'EOF'
safe/work/dependent-apps/results/core-network.json
safe/work/dependent-apps/results/server-media-virtualization.json
safe/work/dependent-apps/results/diagnostics.json
safe/work/dependent-apps/results/source-builds.json
EOF
      diff -u "$EXPECTED_RESULT_FILES" "$SNAPSHOT_DIR/result-files.txt"
      EXPECTED_ARTIFACTS="$(mktemp)"
      {
        cat "$SNAPSHOT_DIR/result-files.txt"
        jq -r '.cases[].log' $(cat "$SNAPSHOT_DIR/result-files.txt")
      } | sort -u >"$EXPECTED_ARTIFACTS"
      diff -u "$EXPECTED_ARTIFACTS" "$SNAPSHOT_DIR/artifact-files.txt"
      test "$(jq -r '.created_by' "$SNAPSHOT_DIR/summary.json")" = "check_13_freeze_phase14_discovery_inputs"
      test "$(jq -r '.head' "$SNAPSHOT_DIR/summary.json")" = "$(git rev-parse "$PHASE_BASE_REF")"
      test "$(jq -r '.artifact_count' "$SNAPSHOT_DIR/summary.json")" = "$(wc -l <"$SNAPSHOT_DIR/artifact-files.txt" | tr -d ' ')"
      test "$(jq -r '.candidate_count' "$SNAPSHOT_DIR/summary.json")" = "$(wc -l <"$SNAPSHOT_DIR/compatibility-candidates.txt" | tr -d ' ')"
      sha256sum -c "$SNAPSHOT_DIR/sha256sums.txt"
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
    }
    validate_phase14_discovery_snapshot
    rm -f safe/work/regressions/results.json
    bash tests/port/regressions/run.sh
    validate_phase14_discovery_snapshot
    test -f safe/work/regressions/results.json
    jq -e '.summary.failed == 0 and all(.results[]; .status == "passed")' safe/work/regressions/results.json
    SAFE_VERSION="$(jq -r '.safe_package_version' safe/generated/packaging/package-build-manifest.json)"
    EXPECTED_CASES="$(mktemp)"
    ACTUAL_CASES="$(mktemp)"
    cp "$SNAPSHOT_DIR/compatibility-candidates.txt" "$EXPECTED_CASES"
    rm -f safe/work/dependent-apps/results/impacted.json
    rm -rf safe/work/dependent-apps/logs/impacted
    if jq -e '(.issues | length) == 0' safe/generated/baseline/client-app-regressions.json >/dev/null; then
      bash tests/port/dependent-apps/run.sh --suite impacted
    else
      SAFE_IMAGE_TAG="$(printf '%s' "$SAFE_VERSION" | sed 's/[^A-Za-z0-9_.-]/-/g')"
      IMAGE="safelibs-libc6-dependent:${SAFE_IMAGE_TAG}"
      bash safe/scripts/build-debs.sh
      bash tests/port/dependent-apps/build-image.sh --debs safe/work/debs --manifest dependents.json --tag "$IMAGE"
      validate_phase14_discovery_snapshot
      bash tests/port/dependent-apps/run.sh --image "$IMAGE" --suite impacted --privileged
    fi
    validate_phase14_discovery_snapshot
    test -f safe/work/dependent-apps/results/impacted.json
    jq -r '[.cases[].case_id] | sort | unique | .[]' safe/work/dependent-apps/results/impacted.json >"$ACTUAL_CASES"
    diff -u "$EXPECTED_CASES" "$ACTUAL_CASES"
    EXPECTED_COUNT="$(wc -l <"$EXPECTED_CASES" | tr -d ' ')"
    jq --argjson expected "$EXPECTED_COUNT" --arg safe_version "$SAFE_VERSION" -e '
      (.suite == "impacted") and
      (.status == "passed") and
      (.summary.total == $expected) and
      (.summary.total == (.cases | length)) and
      (.summary.passed == $expected) and
      (.summary.failed == 0) and
      (.summary.harness_failed == 0) and
      (.cases | type == "array") and
      all(.cases[];
        (.suite == "impacted") and
        (.case_id | type == "string" and contains("/")) and
        (.case == (.case_id | gsub("/"; "__"))) and
        (.status == "passed") and
        (has("failure_kind")) and
        (.failure_kind == null) and
        (.duration_seconds | type == "number") and
        (.log == ("safe/work/dependent-apps/logs/impacted/" + .case + ".log")) and
        (.safe_version == $safe_version) and
        (.rerun | type == "string" and length > 0)
      )
    ' safe/work/dependent-apps/results/impacted.json
    jq -r '.cases[] | [.case_id, .case, .log, .safe_version] | @tsv' safe/work/dependent-apps/results/impacted.json |
      while IFS=$'\t' read -r case_id case_name log_path case_version; do
        case "$log_path" in safe/work/dependent-apps/logs/impacted/*) ;; *) exit 1 ;; esac
        test -f "$log_path"
        grep -F "suite=impacted" "$log_path"
        grep -F "case=${case_name}" "$log_path"
        grep -F "case_id=${case_id}" "$log_path"
        grep -F "safe_version=${case_version}" "$log_path"
      done
    test -z "$(git ls-files -- safe/work)"
    ```

- `check_14_senior_compatibility_review`
  - Type: `check`
  - Fixed `bounce_target`: `impl_14_client_regression_fixes`
  - Purpose: Review fixes from a senior compatibility perspective: ABI, source compatibility, link compatibility, package provenance, and unsafe policy must remain intact.
  - Commands:
    ```bash
    : "${PHASE_BASE_REF:?set to the commit before impl_14_client_regression_fixes began}"
    SNAPSHOT_DIR="safe/work/dependent-apps/artifact-snapshots/phase14-inputs"
    validate_phase14_discovery_snapshot() {
      test -d "$SNAPSHOT_DIR"
      test -f "$SNAPSHOT_DIR/result-files.txt"
      test -f "$SNAPSHOT_DIR/artifact-files.txt"
      test -f "$SNAPSHOT_DIR/sha256sums.txt"
      test -f "$SNAPSHOT_DIR/compatibility-candidates.txt"
      test -f "$SNAPSHOT_DIR/summary.json"
      EXPECTED_RESULT_FILES="$(mktemp)"
      cat >"$EXPECTED_RESULT_FILES" <<'EOF'
safe/work/dependent-apps/results/core-network.json
safe/work/dependent-apps/results/server-media-virtualization.json
safe/work/dependent-apps/results/diagnostics.json
safe/work/dependent-apps/results/source-builds.json
EOF
      diff -u "$EXPECTED_RESULT_FILES" "$SNAPSHOT_DIR/result-files.txt"
      EXPECTED_ARTIFACTS="$(mktemp)"
      {
        cat "$SNAPSHOT_DIR/result-files.txt"
        jq -r '.cases[].log' $(cat "$SNAPSHOT_DIR/result-files.txt")
      } | sort -u >"$EXPECTED_ARTIFACTS"
      diff -u "$EXPECTED_ARTIFACTS" "$SNAPSHOT_DIR/artifact-files.txt"
      test "$(jq -r '.created_by' "$SNAPSHOT_DIR/summary.json")" = "check_13_freeze_phase14_discovery_inputs"
      test "$(jq -r '.head' "$SNAPSHOT_DIR/summary.json")" = "$(git rev-parse "$PHASE_BASE_REF")"
      test "$(jq -r '.artifact_count' "$SNAPSHOT_DIR/summary.json")" = "$(wc -l <"$SNAPSHOT_DIR/artifact-files.txt" | tr -d ' ')"
      test "$(jq -r '.candidate_count' "$SNAPSHOT_DIR/summary.json")" = "$(wc -l <"$SNAPSHOT_DIR/compatibility-candidates.txt" | tr -d ' ')"
      sha256sum -c "$SNAPSHOT_DIR/sha256sums.txt"
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
    }
    validate_phase14_discovery_snapshot
    rm -f safe/work/regressions/results.json
    bash tests/port/regressions/run.sh
    validate_phase14_discovery_snapshot
    test -f safe/work/regressions/results.json
    jq -e '.summary.failed == 0 and all(.results[]; .status == "passed")' safe/work/regressions/results.json
    cd safe
    cargo fmt --check
    cargo test --workspace
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check-abi --all-dsos
    cargo run -p xtask -- check-headers --root work/install-root --lang c --lang c++
    cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build
    cargo run -p xtask -- package-deb --out work/debs
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set basic-required-packages
    cargo run -p xtask -- audit-safety \
      --deny-unreviewed-unsafe \
      --deny-untracked-fallback-c \
      --deny-shipped-temporary-fallback-binaries \
      --deny-shipped-private-backend-dsos \
      --require-cve-disposition \
      --require-package-scope-clean
    test -z "$(git -C .. ls-files -- safe/work)"
    ```

## Success Criteria

Run all three phase 14 checkers. The software-tester check covers reproducibility and impacted app behavior. The senior compatibility check covers ABI, source, link, package, and safety contracts.

Every declared New Output must be tracked by git at `HEAD`, and every phase-owned path must have no staged or unstaged changes after the phase commit. The phase succeeds only when the listed verification phases pass with the artifact-preservation and freshness contracts intact.

## Git Commit Requirement

The implementer must commit all phase-owned work to git before yielding. Checkers assume the phase work is committed at `HEAD` and verify path-scoped cleanliness for the phase-owned file set.
