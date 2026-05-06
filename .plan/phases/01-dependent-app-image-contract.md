# Dependent App Image Contract

## Phase Name

Dependent App Image Contract

## Implement Phase ID

`impl_11_dependent_app_image_contract`

## Preexisting Inputs

- `dependents.json`
- `test-original.sh`
- `safe/scripts/build-debs.sh`
- `safe/scripts/install-safe-repo.sh`
- `safe/generated/packaging/package-build-manifest.json`
- `safe/generated/baseline/package-files/*.json`
- `safe/generated/install-manifests/*.json`
- `scripts/build-debs.sh`

## New Outputs

- `tests/port/dependent-apps/Dockerfile`
- `tests/port/dependent-apps/build-image.sh`
- `tests/port/dependent-apps/run.sh`
- `tests/port/dependent-apps/lib/common.sh`
- `tests/port/dependent-apps/lib/image-contract.sh`
- `safe/generated/baseline/dependent-app-test-plan.json`

## File Changes

- Create `tests/port/dependent-apps/**`.
- Add committed harness metadata to `safe/generated/baseline/dependent-app-test-plan.json`.
- Do not edit `original/**`.
- Do not edit `test-original.sh` in this phase; it remains the legacy extraction source for phases 12 and 13.
- Do not replace or edit `dependents.json`; use `safe/generated/baseline/dependent-app-test-plan.json` for stable suite names and case metadata.

## Implementation Details

- Build the image from existing safe `.deb`s. The image builder must create `safe/work/dependent-apps/image-context/`, copy `.deb` files, `dependents.json`, `safe/generated/baseline/dependent-app-test-plan.json`, and enough of `safe/scripts/install-safe-repo.sh` plus `safe/generated/packaging/package-build-manifest.json` to configure the local apt repository. It must install the safe package set, every unique `binary_package` in `dependents.json`, and both helper package lists from `safe/generated/baseline/dependent-app-test-plan.json`.
- The image builder may delete and recreate only `safe/work/dependent-apps/image-context/`. It must create sibling `results/` and `logs/` directories if absent. It must never delete, rewrite, or truncate existing `safe/work/dependent-apps/results/*.json` or `safe/work/dependent-apps/logs/**`.
- The image builder must accept tags that are already sanitized. Any helper that derives a tag from `safe_package_version` must replace every character outside `[A-Za-z0-9_.-]` with `-`.
- `safe/generated/baseline/dependent-app-test-plan.json` must define `image_helper_packages.runtime` exactly `["dbus-user-session","xvfb","libvirt-clients"]`, `image_helper_packages.source_build_bootstrap` exactly `["ca-certificates","jq","build-essential","dpkg-dev","fakeroot"]`, and `suites["image-contract"].cases` exactly `["safe-packages","safe-apt-policy","dependent-packages","helper-packages","libc-resolution"]`.
- The image must be labeled with at least `org.safelibs.library=libc6`, `org.safelibs.safe_version=<version>`, and `org.safelibs.dependent_count=16`.
- The image-contract suite must verify:
  - `dpkg-query` reports the safe version for `libc6`, `libc6-dev`, `libc6-dbg`, `libc-bin`, `libc-dev-bin`, `locales`, and `nscd`.
  - `apt-cache policy` selects the local SafeLibs repository for those packages.
  - Every dependent `binary_package` is installed.
  - Every helper package from the two helper lists is installed and the commands `Xvfb`, `dbus-run-session`, `dbus-daemon`, `virt-admin`, `jq`, `dpkg-buildpackage`, `fakeroot`, and `cc` are resolvable.
  - Representative binaries resolve `libc.so.6` to the installed path owned by the safe `libc6` package, and `dpkg-query -S` plus `dpkg-query -W` proves that owner package has the safe version.
- The image-contract suite must write `safe/work/dependent-apps/results/image-contract.json` and one log per case under `safe/work/dependent-apps/logs/image-contract/`. Contract violations are harness failures; `run.sh --suite image-contract` must exit nonzero instead of recording them as compatibility candidates.
- Preserve `test-original.sh` byte-for-byte in this phase so the root-level compatibility entrypoint still has its preexisting behavior while the new image-contract harness is introduced alongside it.
- Commit the phase before yielding.

## Verification Phases

- `check_11_manifest_image_contract`
  - Type: `check`
  - Fixed `bounce_target`: `impl_11_dependent_app_image_contract`
  - Purpose: Verify the 16-app inventory is the image contract, the safe `.deb` set builds, and the Docker image contains the safe libc package set plus every runtime application from `dependents.json`. Requires Docker with network access; image-contract must not require `--privileged`.
  - Commands:
    ```bash
    jq -e '.dependents | length >= 12' dependents.json
    jq -e '[.dependents[].binary_package] | unique | length == 16' dependents.json
    bash safe/scripts/build-debs.sh
    SAFE_VERSION="$(jq -r '.safe_package_version' safe/generated/packaging/package-build-manifest.json)"
    SAFE_IMAGE_TAG="$(printf '%s' "$SAFE_VERSION" | sed 's/[^A-Za-z0-9_.-]/-/g')"
    IMAGE="safelibs-libc6-dependent:${SAFE_IMAGE_TAG}"
    bash tests/port/dependent-apps/build-image.sh \
      --debs safe/work/debs \
      --manifest dependents.json \
      --tag "$IMAGE"
    jq -e '
      (.image_helper_packages.runtime == ["dbus-user-session","xvfb","libvirt-clients"]) and
      (.image_helper_packages.source_build_bootstrap == ["ca-certificates","jq","build-essential","dpkg-dev","fakeroot"]) and
      ((.suites["image-contract"].cases | sort) == (["safe-packages","safe-apt-policy","dependent-packages","helper-packages","libc-resolution"] | sort))
    ' safe/generated/baseline/dependent-app-test-plan.json
    test "$(docker image inspect "$IMAGE" --format '{{ index .Config.Labels "org.safelibs.library" }}')" = "libc6"
    test "$(docker image inspect "$IMAGE" --format '{{ index .Config.Labels "org.safelibs.safe_version" }}')" = "$SAFE_VERSION"
    test "$(docker image inspect "$IMAGE" --format '{{ index .Config.Labels "org.safelibs.dependent_count" }}')" = "16"
    docker run --rm "$IMAGE" bash -lc '
      set -euo pipefail
      for pkg in dbus-user-session xvfb libvirt-clients ca-certificates jq build-essential dpkg-dev fakeroot; do
        dpkg-query -W "$pkg" >/dev/null
      done
      for cmd in Xvfb dbus-run-session dbus-daemon virt-admin jq dpkg-buildpackage fakeroot cc; do
        command -v "$cmd" >/dev/null
      done
    '
    rm -f safe/work/dependent-apps/results/image-contract.json
    rm -rf safe/work/dependent-apps/logs/image-contract
    bash tests/port/dependent-apps/run.sh \
      --image "$IMAGE" \
      --suite image-contract
    RESULT="safe/work/dependent-apps/results/image-contract.json"
    test -f "$RESULT"
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
      (($result.cases | map(.case) | length) == ($result.cases | map(.case) | unique | length)) and
      all($result.cases[];
        (.case | type == "string") and
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
    ' safe/generated/baseline/dependent-app-test-plan.json "$RESULT"
    jq -r '.cases[] | [.case_id, .case, .log, .safe_version] | @tsv' "$RESULT" |
      while IFS=$'\t' read -r case_id case_name log_path case_version; do
        case "$log_path" in safe/work/dependent-apps/logs/image-contract/*) ;; *) exit 1 ;; esac
        test -f "$log_path"
        grep -F "suite=image-contract" "$log_path"
        grep -F "case=${case_name}" "$log_path"
        grep -F "case_id=${case_id}" "$log_path"
        grep -F "safe_version=${case_version}" "$log_path"
      done
    test -z "$(git ls-files -- safe/work)"
    ```

- `check_11_existing_artifacts_preserved`
  - Type: `check`
  - Fixed `bounce_target`: `impl_11_dependent_app_image_contract`
  - Purpose: Confirm the phase did not replace prepared authorities, recollect the dependent list, or change legacy `test-original.sh` before extracting runtime and source-build behavior. A byte-for-byte match against `PHASE_BASE_REF` preserves the no-argument entrypoint without running the full legacy app matrix. Does not require Docker or `--privileged`.
  - Commands:
    ```bash
    : "${PHASE_BASE_REF:?set to the commit before impl_11_dependent_app_image_contract began}"
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- original dependents.json .github/workflows/ci-release.yml
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- test-original.sh
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- safe/generated/baseline/package-files safe/generated/install-manifests safe/generated/packaging/package-build-manifest.json
    git diff --exit-code "$PHASE_BASE_REF"..HEAD -- all_cves.json relevant_cves.json safe/generated/security/relevant-cves-index.json safe/generated/baseline/fallback-c-inventory.json safe/generated/baseline/committed-safe-frontier.txt safe/upstream-compat
    git diff --cached --exit-code -- original dependents.json .github/workflows/ci-release.yml
    git diff --cached --exit-code -- test-original.sh
    git diff --cached --exit-code -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --cached --exit-code -- safe/generated/baseline/package-files safe/generated/install-manifests safe/generated/packaging/package-build-manifest.json
    git diff --cached --exit-code -- all_cves.json relevant_cves.json safe/generated/security/relevant-cves-index.json safe/generated/baseline/fallback-c-inventory.json safe/generated/baseline/committed-safe-frontier.txt safe/upstream-compat
    git diff --exit-code -- original dependents.json .github/workflows/ci-release.yml
    git diff --exit-code -- test-original.sh
    git diff --exit-code -- safe/generated/baseline/abi safe/generated/version-scripts safe/generated/baseline/link-compat-corpus.json safe/generated/baseline/test-catalog.json safe/generated/baseline/test-port-plan.json
    git diff --exit-code -- safe/generated/baseline/package-files safe/generated/install-manifests safe/generated/packaging/package-build-manifest.json
    git diff --exit-code -- all_cves.json relevant_cves.json safe/generated/security/relevant-cves-index.json safe/generated/baseline/fallback-c-inventory.json safe/generated/baseline/committed-safe-frontier.txt safe/upstream-compat
    bash -n test-original.sh
    for path in \
      tests/port/dependent-apps/Dockerfile \
      tests/port/dependent-apps/build-image.sh \
      tests/port/dependent-apps/run.sh \
      tests/port/dependent-apps/lib/common.sh \
      tests/port/dependent-apps/lib/image-contract.sh \
      safe/generated/baseline/dependent-app-test-plan.json
    do
      git ls-files --error-unmatch -- "$path"
    done
    git diff --name-only "$PHASE_BASE_REF"..HEAD -- \
      tests/port/dependent-apps \
      safe/generated/baseline/dependent-app-test-plan.json | grep -q .
    git diff --cached --exit-code -- \
      tests/port/dependent-apps \
      safe/generated/baseline/dependent-app-test-plan.json
    git diff --exit-code -- \
      tests/port/dependent-apps \
      safe/generated/baseline/dependent-app-test-plan.json
    jq -e '.metadata.counts.dependent_count == 16 and (.dependents | length == 16)' dependents.json
    test -x tests/port/dependent-apps/build-image.sh
    test -x tests/port/dependent-apps/run.sh
    test -z "$(git ls-files -- safe/work)"
    ```

## Success Criteria

Run `check_11_manifest_image_contract` followed by `check_11_existing_artifacts_preserved`.

Every declared New Output must be tracked by git at `HEAD`, and every phase-owned path must have no staged or unstaged changes after the phase commit. The phase succeeds only when the listed verification phases pass with the artifact-preservation and freshness contracts intact.

## Git Commit Requirement

The implementer must commit all phase-owned work to git before yielding. Checkers assume the phase work is committed at `HEAD` and verify path-scoped cleanliness for the phase-owned file set.
