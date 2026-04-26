SAFE_PHASE := impl_03_packaging_and_harness
SAFE_REQUIRED_PACKAGES := libc6 libc6-dev libc6-dbg libc-bin libc-dev-bin locales nscd
SAFE_DEFERRED_PACKAGES := libc-devtools libc-l10n locales-all glibc-doc glibc-source libc6-udeb

.PHONY: info

info:
	@printf 'phase=%s\n' '$(SAFE_PHASE)'
	@printf 'required=%s\n' '$(SAFE_REQUIRED_PACKAGES)'
	@printf 'deferred=%s\n' '$(SAFE_DEFERRED_PACKAGES)'
