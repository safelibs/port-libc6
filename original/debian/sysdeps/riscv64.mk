# riscv64 buildd are QEMU based and thus very slow
TIMEOUTFACTOR=100

libc_extra_cflags = -fno-omit-frame-pointer
