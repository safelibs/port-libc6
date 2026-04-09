/* Test public AF_UNIX pathname handling through bind/getsockname.
   Copyright (C) 2022-2024 Free Software Foundation, Inc.
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

#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <support/check.h>
#include <support/support.h>
#include <support/temp_file.h>
#include <support/xunistd.h>

static void
bind_path (const char *path, struct sockaddr_un *actual)
{
  int fd = socket (AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
  TEST_VERIFY_EXIT (fd >= 0);

  struct sockaddr_un sun = { 0 };
  sun.sun_family = AF_UNIX;
  TEST_VERIFY_EXIT (strlen (path) < sizeof (sun.sun_path));
  memcpy (sun.sun_path, path, strlen (path) + 1);

  socklen_t len = offsetof (struct sockaddr_un, sun_path) + strlen (path) + 1;
  TEST_COMPARE (bind (fd, (struct sockaddr *) &sun, len), 0);

  socklen_t actual_len = sizeof (*actual);
  memset (actual, 0xcc, sizeof (*actual));
  TEST_COMPARE (getsockname (fd, (struct sockaddr *) actual, &actual_len), 0);
  TEST_COMPARE (actual->sun_family, AF_UNIX);

  xclose (fd);
}

static int
do_test (void)
{
  char *tempdir = support_create_temp_directory ("tst-sockaddr_un_set-");
  char *cwd = getcwd (NULL, 0);
  TEST_VERIFY_EXIT (cwd != NULL);
  xchdir (tempdir);

  struct sockaddr_un sun;

  bind_path ("sock", &sun);
  TEST_COMPARE_STRING (sun.sun_path, "sock");
  TEST_COMPARE (unlink ("sock"), 0);

  {
    char pathname[108];         /* Length of sun_path (ABI constant).  */
    memset (pathname, 'x', sizeof (pathname));
    pathname[sizeof (pathname) - 1] = '\0';
    bind_path (pathname, &sun);
    TEST_COMPARE_STRING (sun.sun_path, pathname);
    TEST_COMPARE (unlink (pathname), 0);
  }

  xchdir (cwd);
  free (cwd);
  free (tempdir);

  return 0;
}

#include <support/test-driver.c>
