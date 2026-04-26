SAFE_BUILD_OUT ?= work/debs

.PHONY: build-safe-packages

build-safe-packages:
	@cargo run -p xtask -- package-deb --out $(SAFE_BUILD_OUT)
