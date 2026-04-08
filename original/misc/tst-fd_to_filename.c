/* Test public descriptor-alias paths (/proc/self/fd or /dev/fd).
   Copyright (C) 2020-2024 Free Software Foundation, Inc.
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

#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <support/check.h>
#include <support/support.h>
#include <support/xunistd.h>

static const char *
fd_alias_prefix (void)
{
  static const char *candidates[] = { "/proc/self/fd/", "/dev/fd/" };
  for (size_t i = 0; i < sizeof (candidates) / sizeof (candidates[0]); ++i)
    {
      char *probe = xasprintf ("%s0", candidates[i]);
      int exists = access (probe, F_OK);
      free (probe);
      if (exists == 0)
        return candidates[i];
    }
  FAIL_UNSUPPORTED ("descriptor aliases are unavailable");
}

static int
open_alias (const char *prefix, int fd, int flags)
{
  char *path = xasprintf ("%s%d", prefix, fd);
  int result = xopen (path, flags, 0);
  free (path);
  return result;
}

static void
check_aliasing (void)
{
  const char *prefix = fd_alias_prefix ();
  int pipes[2];
  xpipe (pipes);

  int read_alias = open_alias (prefix, pipes[0], O_RDONLY);
  int write_alias = open_alias (prefix, pipes[1], O_WRONLY);

  /* Ensure that all the descriptor numbers are different.  */
  TEST_VERIFY (pipes[0] < pipes[1]);
  TEST_VERIFY (pipes[1] < read_alias);
  TEST_VERIFY (read_alias < write_alias);

  xwrite (write_alias, "1", 1);
  char buf[16];
  TEST_COMPARE_BLOB ("1", 1, buf, read (pipes[0], buf, sizeof (buf)));

  xwrite (pipes[1], "2", 1);
  TEST_COMPARE_BLOB ("2", 1, buf, read (read_alias, buf, sizeof (buf)));

  xwrite (write_alias, "3", 1);
  TEST_COMPARE_BLOB ("3", 1, buf, read (read_alias, buf, sizeof (buf)));

  xwrite (pipes[1], "4", 1);
  TEST_COMPARE_BLOB ("4", 1, buf, read (pipes[0], buf, sizeof (buf)));

  xclose (write_alias);
  xclose (read_alias);
  xclose (pipes[1]);
  xclose (pipes[0]);
}

static int
do_test (void)
{
  check_aliasing ();

  return 0;
}

#include <support/test-driver.c>
