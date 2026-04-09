/* Test public AF_UNIX pathname handling through SunRPC Unix transports.
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

#include <errno.h>
#include <dlfcn.h>
#include <rpc/rpc.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <support/check.h>
#include <support/support.h>
#include <support/temp_file.h>
#include <support/xdlfcn.h>
#include <support/xunistd.h>

static SVCXPRT *(*svcunix_create_func) (int, u_int, u_int, char *);

static void
resolve_rpc_functions (void)
{
  if (svcunix_create_func == NULL)
    svcunix_create_func =
      (SVCXPRT *(*)(int, u_int, u_int, char *))
      xdlvsym (RTLD_DEFAULT, "svcunix_create", "GLIBC_2.2.5");
}

static void
check_path (const char *path)
{
  resolve_rpc_functions ();
  SVCXPRT *transport = svcunix_create_func (RPC_ANYSOCK, 0, 0, (char *) path);
  TEST_VERIFY_EXIT (transport != NULL);

  struct sockaddr_un actual;
  socklen_t actual_len = sizeof (actual);
  memset (&actual, 0xcc, sizeof (actual));
  TEST_COMPARE (getsockname (transport->xp_sock,
                             (struct sockaddr *) &actual, &actual_len), 0);
  TEST_COMPARE (actual.sun_family, AF_UNIX);
  TEST_COMPARE_STRING (actual.sun_path, path);

  SVC_DESTROY (transport);
  TEST_COMPARE (unlink (path), 0);
}

static int
do_test (void)
{
  char *tempdir = support_create_temp_directory ("tst-sockaddr_un_set-");
  char *cwd = getcwd (NULL, 0);
  TEST_VERIFY_EXIT (cwd != NULL);
  xchdir (tempdir);

  check_path ("sock");

  {
    char pathname[108];         /* Length of sun_path (ABI constant).  */
    memset (pathname, 'x', sizeof (pathname));
    pathname[sizeof (pathname) - 1] = '\0';
    check_path (pathname);
  }

  {
    char pathname[109];
    memset (pathname, 'x', sizeof (pathname));
    pathname[sizeof (pathname) - 1] = '\0';
    errno = 0;
    TEST_VERIFY (svcunix_create_func (RPC_ANYSOCK, 0, 0, pathname) == NULL);
    TEST_COMPARE (errno, EINVAL);
  }

  xchdir (cwd);
  free (cwd);
  free (tempdir);

  return 0;
}

#include <support/test-driver.c>
