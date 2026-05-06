FROM ubuntu:24.04

ARG SAFE_VERSION
ARG DEPENDENT_COUNT

LABEL org.safelibs.library="libc6"
LABEL org.safelibs.safe_version="${SAFE_VERSION}"
LABEL org.safelibs.dependent_count="${DEPENDENT_COUNT}"

ENV DEBIAN_FRONTEND=noninteractive
ENV SAFE_VERSION="${SAFE_VERSION}"

SHELL ["/bin/bash", "-euxo", "pipefail", "-c"]
WORKDIR /workspace

COPY debs/ /workspace/safe/work/debs/
COPY dependents.json /workspace/dependents.json
COPY safe/scripts/install-safe-repo.sh /workspace/safe/scripts/install-safe-repo.sh
COPY safe/generated/packaging/package-build-manifest.json /workspace/safe/generated/packaging/package-build-manifest.json
COPY safe/generated/baseline/dependent-app-test-plan.json /workspace/safe/generated/baseline/dependent-app-test-plan.json

RUN chmod +x /workspace/safe/scripts/install-safe-repo.sh \
  && apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates jq dpkg-dev \
  && /workspace/safe/scripts/install-safe-repo.sh safe/work/debs \
  && mapfile -t safe_packages < <(jq -r '.packages[].name' /workspace/safe/generated/packaging/package-build-manifest.json | sort -u) \
  && mapfile -t dependent_packages < <(jq -r '.dependents[].binary_package' /workspace/dependents.json | sort -u) \
  && mapfile -t helper_packages < <(jq -r '.image_helper_packages.runtime[], .image_helper_packages.source_build_bootstrap[]' /workspace/safe/generated/baseline/dependent-app-test-plan.json | sort -u) \
  && safe_package_specs=() \
  && for pkg in "${safe_packages[@]}"; do safe_package_specs+=("${pkg}=${SAFE_VERSION}"); done \
  && apt-get install -y --no-install-recommends --allow-downgrades "${safe_package_specs[@]}" \
  && export LD_LIBRARY_PATH="/usr/lib/x86_64-linux-gnu:/lib/x86_64-linux-gnu:/usr/lib64:/lib64${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}" \
  && apt-get install -y --no-install-recommends "${dependent_packages[@]}" "${helper_packages[@]}" \
  && apt-get clean
