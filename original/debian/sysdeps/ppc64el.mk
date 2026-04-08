# configuration options for all flavours
extra_config_options = --enable-multi-arch --with-cpu=power8

# main library
libc_rtlddir = /lib64
# loop unrolling causes false positive warnings
# see: https://gcc.gnu.org/bugzilla/show_bug.cgi?id=92955
libc_extra_cflags = -O3 -fno-tree-vectorize -Wno-error=stringop-overflow
