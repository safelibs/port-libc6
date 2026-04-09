/* Basic test of the public statx interface.
   Copyright (C) 2018-2024 Free Software Foundation, Inc.
   This file is part of the GNU C Library.

   The GNU C Library is free software; you can redistribute it and/or
   modify it under the terms of the GNU Lesser General Public
   License as published by the Free Software Foundation; either
   version 2.1 of the License, or (at your option) any later version.

   The GNU C Library is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
   Lesser General Public License for more details.

   You should have received a copy of the GNU Lesser General Public
   License along with the GNU C Library; if not, see
   <https://www.gnu.org/licenses/>.  */

#define _GNU_SOURCE 1

#include <errno.h>
#include <fcntl.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <support/check.h>
#include <support/support.h>
#include <support/temp_file.h>
#include <support/xunistd.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/sysmacros.h>
#include <unistd.h>

/* Ensure that the types have the kernel-expected layout.  */
_Static_assert (sizeof (struct statx_timestamp) == 16, "statx_timestamp size");
_Static_assert (sizeof (struct statx) == 256, "statx size");
_Static_assert (offsetof (struct statx, stx_nlink) == 16, "statx nlink");
_Static_assert (offsetof (struct statx, stx_ino) == 32, "statx ino");
_Static_assert (offsetof (struct statx, stx_atime) == 64, "statx atime");
_Static_assert (offsetof (struct statx, stx_rdev_major) == 128, "statx rdev");
_Static_assert (offsetof (struct statx, __statx_pad2) == 144, "statx pad2");

/* Return true if we have a real implementation of statx.  */
static bool
kernel_supports_statx (void)
{
#ifdef __NR_statx
  struct statx buf;
  return syscall (__NR_statx, 0, "", AT_EMPTY_PATH, 0, &buf) == 0
    || errno != ENOSYS;
#else
  return false;
#endif
}

/* Tests which apply to the public statx interface.  */
static void
public_statx_tests (const char *path, int fd)
{
  uint64_t ino;
  {
    struct statx buf = { 0, };
    TEST_COMPARE (statx (fd, "", AT_EMPTY_PATH, STATX_BASIC_STATS, &buf), 0);
    TEST_COMPARE (buf.stx_size, 3);
    ino = buf.stx_ino;
  }
  {
    struct statx buf = { 0, };
    TEST_COMPARE (statx (AT_FDCWD, path, 0, STATX_BASIC_STATS, &buf), 0);
    TEST_COMPARE (buf.stx_size, 3);
    TEST_COMPARE (buf.stx_ino, ino);
  }
  {
    struct statx stx = { 0, };
    TEST_COMPARE (statx (fd, "", AT_EMPTY_PATH, STATX_BASIC_STATS, &stx), 0);
    struct stat64 st;
    xfstat (fd, &st);
    TEST_COMPARE (stx.stx_mode, st.st_mode);
    TEST_COMPARE (stx.stx_dev_major, major (st.st_dev));
    TEST_COMPARE (stx.stx_dev_minor, minor (st.st_dev));
  }
  {
    struct statx stx = { 0, };
    TEST_COMPARE (statx (AT_FDCWD, "/dev/null", 0, STATX_BASIC_STATS, &stx),
                  0);
    struct stat64 st;
    xstat ("/dev/null", &st);
    TEST_COMPARE (stx.stx_mode, st.st_mode);
    TEST_COMPARE (stx.stx_dev_major, major (st.st_dev));
    TEST_COMPARE (stx.stx_dev_minor, minor (st.st_dev));
    TEST_COMPARE (stx.stx_rdev_major, major (st.st_rdev));
    TEST_COMPARE (stx.stx_rdev_minor, minor (st.st_rdev));
  }
}

static int
do_test (void)
{
  char *path;
  int fd = create_temp_file ("tst-statx-", &path);
  TEST_VERIFY_EXIT (fd >= 0);
  support_write_file_string (path, "abc");

  public_statx_tests (path, fd);

  if (kernel_supports_statx ())
    {
      puts ("info: kernel supports statx");
      struct statx buf;
      buf.stx_size = 0;
      TEST_COMPARE (statx (fd, "", AT_EMPTY_PATH | AT_STATX_FORCE_SYNC,
                           STATX_BASIC_STATS, &buf),
                    0);
      TEST_COMPARE (buf.stx_size, 3);
      buf.stx_size = 0;
      TEST_COMPARE (statx (fd, "", AT_EMPTY_PATH | AT_STATX_DONT_SYNC,
                           STATX_BASIC_STATS, &buf),
                    0);
      TEST_COMPARE (buf.stx_size, 3);
    }

  xclose (fd);
  free (path);

  return 0;
}

#include <support/test-driver.c>
