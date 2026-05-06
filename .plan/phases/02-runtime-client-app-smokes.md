# Runtime Client Application Smokes

## Phase Name

Runtime Client Application Smokes

## Implement Phase ID

`impl_12_dependent_runtime_smokes`

## Preexisting Inputs

- All outputs from `impl_11_dependent_app_image_contract`.
- The unchanged legacy `test-original.sh` body, especially `test_bash`, `test_coreutils`, `test_systemd`, `test_python312_minimal`, `test_git`, `test_openssh_client`, `test_network_manager`, `test_nginx`, `test_postgresql_16`, `test_ffmpeg`, `test_qemu_system_x86`, `test_podman`, `test_gnome_shell`, `test_strace`, `test_valgrind`, and `test_libvirt`.

## New Outputs

- `tests/port/dependent-apps/cases/bash.sh`
- `tests/port/dependent-apps/cases/coreutils.sh`
- `tests/port/dependent-apps/cases/systemd.sh`
- `tests/port/dependent-apps/cases/python3.12-minimal.sh`
- `tests/port/dependent-apps/cases/git.sh`
- `tests/port/dependent-apps/cases/openssh-client.sh`
- `tests/port/dependent-apps/cases/network-manager.sh`
- `tests/port/dependent-apps/cases/nginx.sh`
- `tests/port/dependent-apps/cases/postgresql-16.sh`
- `tests/port/dependent-apps/cases/ffmpeg.sh`
- `tests/port/dependent-apps/cases/qemu-system-x86.sh`
- `tests/port/dependent-apps/cases/podman.sh`
- `tests/port/dependent-apps/cases/gnome-shell.sh`
- `tests/port/dependent-apps/cases/strace.sh`
- `tests/port/dependent-apps/cases/valgrind.sh`
- `tests/port/dependent-apps/cases/libvirt.sh`
- Updated `safe/generated/baseline/dependent-app-test-plan.json` suite definitions.

## File Changes

- Add per-app runtime case scripts under `tests/port/dependent-apps/cases/`.
- Extend `tests/port/dependent-apps/run.sh` to select cases by suite or by explicit app name.
- Update `safe/generated/baseline/dependent-app-test-plan.json`.
- Do not edit `test-original.sh` in this phase; phase 13 still needs the source-build logic from the legacy body.

## Implementation Details

- Factor the current inline runtime functions from the unchanged `test-original.sh` into per-app scripts. Each script must be independently runnable inside the image and must use the common log/result helpers.
- Keep runtime tests functional rather than superficial:
  - `bash`: run a shell command and verify output.
  - `coreutils`: sort data under a minimal environment.
  - `systemd`: verify a unit file and exercise `systemd-tmpfiles`.
  - `python3.12-minimal`: execute a Python expression.
  - `git`: initialize a repo and commit.
  - `openssh-client`: generate a key and inspect `ssh -G`.
  - `network-manager`: create an offline connection with `nmcli`.
  - `nginx`: validate a real config.
  - `postgresql-16`: initialize a database, start a local server on a Unix socket, and run SQL.
  - `ffmpeg`: generate and process synthetic audio.
  - `qemu-system-x86`: start and stop a no-machine TCG instance.
  - `podman`: use isolated vfs storage and run an Alpine smoke container.
  - `gnome-shell`: run under Xvfb and D-Bus/logind shims as the existing harness does.
  - `strace`: trace a shell write and verify both traced output and captured syscall.
  - `valgrind`: compile and run a small allocation/free program under `valgrind --error-exitcode=1`.
  - `libvirt`: start a local `libvirtd` with scratch runtime directories and query it with `virt-admin`.
- The `all-runtime` suite must include all 16 names from `dependents.json`; no runtime-dependent app may be covered only by a source-build test.
- `tests/port/dependent-apps/run.sh` must distinguish runner failure from client compatibility failure. It must exit nonzero only when the requested suite cannot be executed or the result artifact cannot be written, including a missing image, invalid suite, missing case script, Docker/container startup failure, unwritable result directory, or malformed suite metadata. Once a selected case starts, a nonzero app command must be captured as a case failure in JSON and must not abort the whole suite.
- Suite results must be written as JSON with top-level `suite`, `status`, `summary`, and `cases`. `summary` must include `total`, `passed`, `failed`, and `harness_failed`. Each case object must include `case_id`, `case`, `suite`, `status`, `failure_kind`, `duration_seconds`, `log`, `safe_version`, and `rerun`. For normal suites, `case_id` must be `<suite>/<case>` and `log` must be `safe/work/dependent-apps/logs/<suite>/<case>.log`. Passed cases use `failure_kind: null`; application/source behavior failures use `failure_kind: "compatibility_candidate"`; harness failures use `failure_kind: "harness"` and make the suite command fail.
- Running a suite may replace only `safe/work/dependent-apps/results/<suite>.json` and `safe/work/dependent-apps/logs/<suite>/`. It must preserve all other files under `safe/work/dependent-apps/results/` and `safe/work/dependent-apps/logs/` byte-for-byte.
- Every case log must include machine-greppable provenance lines `suite=<suite>`, `case=<case>`, `case_id=<suite>/<case>`, and `safe_version=<safe_version>` matching the result row.
- Commit the phase before yielding.

## Verification Phases

- `check_12_core_network_runtime_apps`
  - Type: `check`
  - Fixed `bounce_target`: `impl_12_dependent_runtime_smokes`
  - Purpose: Run deterministic runtime checks for the core and network-oriented applications. Requires Docker with network access to build the image; this suite must not require `--privileged`.
  - Commands:
    ```bash
    SAFE_VERSION="$(jq -r '.safe_package_version' safe/generated/packaging/package-build-manifest.json)"
    SAFE_IMAGE_TAG="$(printf '%s' "$SAFE_VERSION" | sed 's/[^A-Za-z0-9_.-]/-/g')"
    IMAGE="safelibs-libc6-dependent:${SAFE_IMAGE_TAG}"
    bash safe/scripts/build-debs.sh
    bash tests/port/dependent-apps/build-image.sh --debs safe/work/debs --manifest dependents.json --tag "$IMAGE"
    rm -f safe/work/dependent-apps/results/core-network.json
    rm -rf safe/work/dependent-apps/logs/core-network
    bash tests/port/dependent-apps/run.sh --image "$IMAGE" --suite core-network
    RESULT="safe/work/dependent-apps/results/core-network.json"
    jq -s -e --arg suite "core-network" --arg safe_version "$SAFE_VERSION" '
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
        test -x "tests/port/dependent-apps/cases/${case_name}.sh"
        case "$log_path" in safe/work/dependent-apps/logs/core-network/*) ;; *) exit 1 ;; esac
        test -f "$log_path"
        grep -F "suite=core-network" "$log_path"
        grep -F "case=${case_name}" "$log_path"
        grep -F "case_id=${case_id}" "$log_path"
        grep -F "safe_version=${case_version}" "$log_path"
      done
    test -z "$(git ls-files -- safe/work)"
    ```

- `check_12_server_media_virtualization_runtime_apps`
  - Type: `check`
  - Fixed `bounce_target`: `impl_12_dependent_runtime_smokes`
  - Purpose: Run heavier runtime checks for database, media, virtualization, container, and desktop applications. Requires Docker with network access and runs the suite with `--privileged`.
  - Commands:
    ```bash
    SAFE_VERSION="$(jq -r '.safe_package_version' safe/generated/packaging/package-build-manifest.json)"
    SAFE_IMAGE_TAG="$(printf '%s' "$SAFE_VERSION" | sed 's/[^A-Za-z0-9_.-]/-/g')"
    IMAGE="safelibs-libc6-dependent:${SAFE_IMAGE_TAG}"
    test -f safe/work/dependent-apps/results/core-network.json
    mapfile -t PRESERVE_ARTIFACTS < <(
      printf '%s\n' safe/work/dependent-apps/results/core-network.json
      jq -r '.cases[].log' safe/work/dependent-apps/results/core-network.json
    )
    for path in "${PRESERVE_ARTIFACTS[@]}"; do test -f "$path"; done
    PRESERVE_SNAPSHOT="$(mktemp)"
    sha256sum "${PRESERVE_ARTIFACTS[@]}" >"$PRESERVE_SNAPSHOT"
    bash safe/scripts/build-debs.sh
    bash tests/port/dependent-apps/build-image.sh --debs safe/work/debs --manifest dependents.json --tag "$IMAGE"
    sha256sum -c "$PRESERVE_SNAPSHOT"
    rm -f safe/work/dependent-apps/results/server-media-virtualization.json
    rm -rf safe/work/dependent-apps/logs/server-media-virtualization
    bash tests/port/dependent-apps/run.sh --image "$IMAGE" --suite server-media-virtualization --privileged
    sha256sum -c "$PRESERVE_SNAPSHOT"
    RESULT="safe/work/dependent-apps/results/server-media-virtualization.json"
    jq -s -e --arg suite "server-media-virtualization" --arg safe_version "$SAFE_VERSION" '
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
        test -x "tests/port/dependent-apps/cases/${case_name}.sh"
        case "$log_path" in safe/work/dependent-apps/logs/server-media-virtualization/*) ;; *) exit 1 ;; esac
        test -f "$log_path"
        grep -F "suite=server-media-virtualization" "$log_path"
        grep -F "case=${case_name}" "$log_path"
        grep -F "case_id=${case_id}" "$log_path"
        grep -F "safe_version=${case_version}" "$log_path"
      done
    test -z "$(git ls-files -- safe/work)"
    ```

- `check_12_diagnostic_runtime_apps`
  - Type: `check`
  - Fixed `bounce_target`: `impl_12_dependent_runtime_smokes`
  - Purpose: Run runtime checks for the diagnostic and virtualization-management applications that are also source-build dependents. Requires Docker with network access and runs the suite with `--privileged`.
  - Commands:
    ```bash
    SAFE_VERSION="$(jq -r '.safe_package_version' safe/generated/packaging/package-build-manifest.json)"
    SAFE_IMAGE_TAG="$(printf '%s' "$SAFE_VERSION" | sed 's/[^A-Za-z0-9_.-]/-/g')"
    IMAGE="safelibs-libc6-dependent:${SAFE_IMAGE_TAG}"
    for result in \
      safe/work/dependent-apps/results/core-network.json \
      safe/work/dependent-apps/results/server-media-virtualization.json
    do
      test -f "$result"
    done
    mapfile -t PRESERVE_ARTIFACTS < <(
      printf '%s\n' \
        safe/work/dependent-apps/results/core-network.json \
        safe/work/dependent-apps/results/server-media-virtualization.json
      jq -r '.cases[].log' \
        safe/work/dependent-apps/results/core-network.json \
        safe/work/dependent-apps/results/server-media-virtualization.json
    )
    for path in "${PRESERVE_ARTIFACTS[@]}"; do test -f "$path"; done
    PRESERVE_SNAPSHOT="$(mktemp)"
    sha256sum "${PRESERVE_ARTIFACTS[@]}" >"$PRESERVE_SNAPSHOT"
    bash safe/scripts/build-debs.sh
    bash tests/port/dependent-apps/build-image.sh --debs safe/work/debs --manifest dependents.json --tag "$IMAGE"
    sha256sum -c "$PRESERVE_SNAPSHOT"
    rm -f safe/work/dependent-apps/results/diagnostics.json
    rm -rf safe/work/dependent-apps/logs/diagnostics
    bash tests/port/dependent-apps/run.sh --image "$IMAGE" --suite diagnostics --privileged
    sha256sum -c "$PRESERVE_SNAPSHOT"
    RESULT="safe/work/dependent-apps/results/diagnostics.json"
    jq -s -e --arg suite "diagnostics" --arg safe_version "$SAFE_VERSION" '
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
        test -x "tests/port/dependent-apps/cases/${case_name}.sh"
        case "$log_path" in safe/work/dependent-apps/logs/diagnostics/*) ;; *) exit 1 ;; esac
        test -f "$log_path"
        grep -F "suite=diagnostics" "$log_path"
        grep -F "case=${case_name}" "$log_path"
        grep -F "case_id=${case_id}" "$log_path"
        grep -F "safe_version=${case_version}" "$log_path"
      done
    test -z "$(git ls-files -- safe/work)"
    ```

- `check_12_runtime_log_quality`
  - Type: `check`
  - Fixed `bounce_target`: `impl_12_dependent_runtime_smokes`
  - Purpose: Ensure each runtime app has a standalone log, status, and rerunnable case name.
  - Commands:
    ```bash
    jq -e '.suites["core-network"].cases | index("bash") and index("coreutils") and index("systemd") and index("python3.12-minimal") and index("git") and index("openssh-client") and index("network-manager") and index("nginx")' safe/generated/baseline/dependent-app-test-plan.json
    jq -e '.suites["server-media-virtualization"].cases | index("postgresql-16") and index("ffmpeg") and index("qemu-system-x86") and index("podman") and index("gnome-shell")' safe/generated/baseline/dependent-app-test-plan.json
    jq -e '.suites["diagnostics"].cases | index("strace") and index("valgrind") and index("libvirt")' safe/generated/baseline/dependent-app-test-plan.json
    jq -e '.suites["all-runtime"].cases | length == 16' safe/generated/baseline/dependent-app-test-plan.json
    jq -s -e '([.[0].dependents[].name] | sort) == ([.[1].suites["all-runtime"].cases[]] | sort)' dependents.json safe/generated/baseline/dependent-app-test-plan.json
    : "${PHASE_BASE_REF:?set to the commit before impl_12_dependent_runtime_smokes began}"
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- original dependents.json .github/workflows/ci-release.yml test-original.sh
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- safe/generated/baseline/package-files safe/generated/install-manifests safe/generated/packaging/package-build-manifest.json
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- all_cves.json relevant_cves.json safe/generated/security/relevant-cves-index.json safe/generated/baseline/fallback-c-inventory.json safe/generated/baseline/committed-safe-frontier.txt safe/upstream-compat
    git diff --cached --exit-code -- original dependents.json .github/workflows/ci-release.yml test-original.sh
    git diff --cached --exit-code -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --cached --exit-code -- safe/generated/baseline/package-files safe/generated/install-manifests safe/generated/packaging/package-build-manifest.json
    git diff --cached --exit-code -- all_cves.json relevant_cves.json safe/generated/security/relevant-cves-index.json safe/generated/baseline/fallback-c-inventory.json safe/generated/baseline/committed-safe-frontier.txt safe/upstream-compat
    git diff --exit-code -- original dependents.json .github/workflows/ci-release.yml test-original.sh
    git diff --exit-code -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --exit-code -- safe/generated/baseline/package-files safe/generated/install-manifests safe/generated/packaging/package-build-manifest.json
    git diff --exit-code -- all_cves.json relevant_cves.json safe/generated/security/relevant-cves-index.json safe/generated/baseline/fallback-c-inventory.json safe/generated/baseline/committed-safe-frontier.txt safe/upstream-compat
    for path in \
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
      tests/port/dependent-apps/run.sh \
      safe/generated/baseline/dependent-app-test-plan.json
    do
      git ls-files --error-unmatch -- "$path"
    done
    git diff --name-only "$PHASE_BASE_REF"..HEAD -- \
      tests/port/dependent-apps/cases \
      tests/port/dependent-apps/run.sh \
      safe/generated/baseline/dependent-app-test-plan.json | grep -q .
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- test-original.sh
    git diff --cached --exit-code -- \
      tests/port/dependent-apps/cases \
      tests/port/dependent-apps/run.sh \
      safe/generated/baseline/dependent-app-test-plan.json \
      test-original.sh
    git diff --exit-code -- \
      tests/port/dependent-apps/cases \
      tests/port/dependent-apps/run.sh \
      safe/generated/baseline/dependent-app-test-plan.json \
      test-original.sh
    for suite in core-network server-media-virtualization diagnostics; do
      result="safe/work/dependent-apps/results/${suite}.json"
      test -f "$result"
      jq -s -e --arg suite "$suite" '
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
      ' safe/generated/baseline/dependent-app-test-plan.json "$result"
      jq -r '.cases[] | [.case_id, .case, .log, .safe_version] | @tsv' "$result" |
        while IFS=$'\t' read -r case_id case_name log_path case_version; do
          test -x "tests/port/dependent-apps/cases/${case_name}.sh"
          case "$log_path" in safe/work/dependent-apps/logs/${suite}/*) ;; *) exit 1 ;; esac
          test -f "$log_path"
          grep -F "suite=${suite}" "$log_path"
          grep -F "case=${case_name}" "$log_path"
          grep -F "case_id=${case_id}" "$log_path"
          grep -F "safe_version=${case_version}" "$log_path"
        done
    done
    find safe/work/dependent-apps/logs/core-network safe/work/dependent-apps/logs/server-media-virtualization safe/work/dependent-apps/logs/diagnostics -type f -name '*.log' | wc -l | awk '{ exit !($1 == 16) }'
    test -z "$(git ls-files -- safe/work)"
    ```

## Success Criteria

Run all four phase 12 checkers. Accept complete runtime result artifacts, including failed apps recorded as `compatibility_candidate`. Fail only for harness/infrastructure problems or malformed artifacts. Do not fix app failures in this phase unless the fix is harness isolation; library and package compatibility fixes belong in phase 14 with regression reproducers.

Every declared New Output must be tracked by git at `HEAD`, and every phase-owned path must have no staged or unstaged changes after the phase commit. The phase succeeds only when the listed verification phases pass with the artifact-preservation and freshness contracts intact.

## Git Commit Requirement

The implementer must commit all phase-owned work to git before yielding. Checkers assume the phase work is committed at `HEAD` and verify path-scoped cleanliness for the phase-owned file set.
