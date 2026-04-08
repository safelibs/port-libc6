GLIBC_GIT = https://sourceware.org/git/glibc.git
GLIBC_BRANCH = release/$(DEB_VERSION_UPSTREAM)/master
GLIBC_TAG = glibc-$(DEB_VERSION_UPSTREAM)
GLIBC_CHECKOUT = glibc-checkout
GLIBC_DIR = glibc-$(DEB_VERSION_UPSTREAM)
DEB_ORIG = ../glibc_$(DEB_VERSION_UPSTREAM).orig.tar.xz
GIT_UPDATES_DIFF = debian/patches/git-updates.diff

get-orig-source: $(DEB_ORIG)
$(DEB_ORIG):
	dh_testdir
	git clone --bare $(GLIBC_GIT) $(GLIBC_CHECKOUT)
	mkdir -p $(GLIBC_DIR)
	(cd $(GLIBC_CHECKOUT) && git archive -v --format=tar $(GLIBC_TAG)) | tar -C $(GLIBC_DIR) -xf -
	rm -fr $(GLIBC_DIR)/manual
	tar --mode=go=rX,u+rw,a-s --owner=root --group=root --numeric-owner -Jcf $(DEB_ORIG) $(GLIBC_DIR)
	rm -rf $(GLIBC_DIR) $(GLIBC_CHECKOUT)

update-from-upstream:
	dh_testdir
	git fetch origin
	h=$$(git log -n 1 --format=%H origin/$(GLIBC_BRANCH)); \
	echo "GIT update of $(GLIBC_GIT)/$(GLIBC_BRANCH) from $(GLIBC_TAG) to $$h" > $(GIT_UPDATES_DIFF)
	echo "" >> $(GIT_UPDATES_DIFF)
	git diff --no-renames $(GLIBC_TAG) origin/$(GLIBC_BRANCH) >> $(GIT_UPDATES_DIFF)

make-new-snapshot:
	git fetch origin
	d=$$(git describe origin/master); \
	v=$$(echo $$d | sed 's/-/_/'); \
	dv=$$(echo $$v | sed 's/.*_//')-0ubuntu1; \
	git archive --prefix=$$d/ origin/master | xz -9evk > ../$$v.orig.tar.xz; \
	dch -v $$dv ''
