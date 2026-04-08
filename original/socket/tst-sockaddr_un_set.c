/* Test UNIX-domain pathname sockets through the public API.
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
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>

#include <support/check.h>
#include <support/temp_file.h>
#include <support/support.h>
#include <support/xsocket.h>
#include <support/xunistd.h>

static socklen_t
make_address (struct sockaddr_un *sun, const char *path)
{
  size_t path_length = strlen (path);
  TEST_VERIFY_EXIT (path_length < sizeof (sun->sun_path));

  memset (sun, 0, sizeof (*sun));
  sun->sun_family = AF_UNIX;
  memcpy (sun->sun_path, path, path_length + 1);
  return offsetof (struct sockaddr_un, sun_path) + path_length + 1;
}

static char *
make_socket_path (const char *dir, size_t name_length)
{
  char *path = xmalloc (strlen (dir) + 1 + name_length + 1);
  sprintf (path, "%s/", dir);
  memset (path + strlen (path), 'x', name_length);
  path[strlen (dir) + 1 + name_length] = '\0';
  return path;
}

static void
check_path (const char *path)
{
  struct sockaddr_un addr;
  socklen_t addrlen = make_address (&addr, path);

  int listener = xsocket (AF_UNIX, SOCK_STREAM, 0);
  xbind (listener, (const struct sockaddr *) &addr, addrlen);
  xlisten (listener, 1);

  struct sockaddr_un actual;
  socklen_t actual_len = sizeof (actual);
  memset (&actual, 0xcc, sizeof (actual));
  xgetsockname (listener, (struct sockaddr *) &actual, &actual_len);
  TEST_COMPARE (actual.sun_family, AF_UNIX);
  TEST_COMPARE_STRING (actual.sun_path, path);
  TEST_COMPARE (actual_len, addrlen);

  int client = xsocket (AF_UNIX, SOCK_STREAM, 0);
  xconnect (client, (const struct sockaddr *) &addr, addrlen);
  int accepted = xaccept (listener, NULL, NULL);

  xclose (accepted);
  xclose (client);
  xclose (listener);
  xunlink (path);
}

static int
do_test (void)
{
  char *dir = support_create_temp_directory ("tst-sockaddr_un_set-");

  char *short_path = xasprintf ("%s/socket", dir);
  check_path (short_path);
  free (short_path);

  size_t max_name_length = sizeof (((struct sockaddr_un *) 0)->sun_path) - 2
                           - strlen (dir);
  TEST_VERIFY_EXIT (max_name_length > 0);
  char *max_path = make_socket_path (dir, max_name_length);
  TEST_COMPARE (strlen (max_path),
                sizeof (((struct sockaddr_un *) 0)->sun_path) - 1);
  check_path (max_path);
  free (max_path);
  free (dir);

  return 0;
}

#include <support/test-driver.c>
