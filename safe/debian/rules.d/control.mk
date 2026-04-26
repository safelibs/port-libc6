SAFE_DEBIAN_CONTROL := debian/control
SAFE_PACKAGE_BUILD_MANIFEST := safe/generated/packaging/package-build-manifest.json

.PHONY: control-check

control-check:
	@test -f $(SAFE_DEBIAN_CONTROL)
	@test -f $(SAFE_PACKAGE_BUILD_MANIFEST)
