/* Test file-change handling through public resolver APIs.
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

#include <arpa/inet.h>
#include <errno.h>
#include <resolv.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <support/check.h>
#include <support/namespace.h>
#include <support/support.h>
#include <support/test-driver.h>
#include <support/xunistd.h>
#include <unistd.h>

static struct support_chroot *chroot_env;

static const char resolv_conf_one[] =
  "options timeout:3 attempts:2 ndots:4 rotate\n"
  "search corp.example.com example.com\n"
  "nameserver 192.0.2.1\n";

static const char resolv_conf_two[] =
  "options timeout:5 attempts:4 ndots:2 single-request\n"
  "search example.net example.org\n"
  "nameserver 192.0.2.2\n"
  "nameserver 192.0.2.3\n";

struct resolver_snapshot
{
  int retrans;
  int retry;
  unsigned long options;
  int nscount;
  struct sockaddr_in nsaddr_list[MAXNS];
  char defdname[sizeof (((struct __res_state *) 0)->defdname)];
  unsigned ndots;
  unsigned nsort;
  struct
  {
    struct in_addr addr;
    uint32_t mask;
  } sort_list[MAXRESOLVSORT];
  char dnsrch[MAXDNSRCH][256];
};

static void
capture_snapshot (struct resolver_snapshot *snapshot,
                  const struct __res_state *state)
{
  memset (snapshot, 0, sizeof (*snapshot));
  snapshot->retrans = state->retrans;
  snapshot->retry = state->retry;
  snapshot->options = state->options;
  snapshot->nscount = state->nscount;
  memcpy (snapshot->nsaddr_list, state->nsaddr_list,
          sizeof (snapshot->nsaddr_list));
  memcpy (snapshot->defdname, state->defdname, sizeof (snapshot->defdname));
  snapshot->ndots = state->ndots;
  snapshot->nsort = state->nsort;
  memcpy (snapshot->sort_list, state->sort_list, sizeof (snapshot->sort_list));
  for (int i = 0; i < MAXDNSRCH && state->dnsrch[i] != NULL; ++i)
    {
      size_t length = strlen (state->dnsrch[i]);
      TEST_VERIFY_EXIT (length < sizeof (snapshot->dnsrch[i]));
      memcpy (snapshot->dnsrch[i], state->dnsrch[i], length + 1);
    }
}

static void
load_snapshot (struct resolver_snapshot *snapshot)
{
  struct __res_state resolver = { 0 };
  TEST_COMPARE (res_ninit (&resolver), 0);
  capture_snapshot (snapshot, &resolver);
  res_nclose (&resolver);
}

static void
check_same_snapshot (const char *left_name,
                     const struct resolver_snapshot *left,
                     const char *right_name,
                     const struct resolver_snapshot *right)
{
  if (test_verbose > 0)
    printf ("info: comparing %s and %s\n", left_name, right_name);
  TEST_COMPARE_BLOB (left, sizeof (*left), right, sizeof (*right));
}

static void
check_different_snapshot (const char *left_name,
                          const struct resolver_snapshot *left,
                          const char *right_name,
                          const struct resolver_snapshot *right)
{
  if (test_verbose > 0)
    printf ("info: ensuring %s and %s differ\n", left_name, right_name);
  TEST_VERIFY (memcmp (left, right, sizeof (*left)) != 0);
}

static void
remove_path_if_exists (const char *path)
{
  struct stat st;
  if (lstat (path, &st) != 0)
    {
      if (errno == ENOENT)
        return;
      FAIL_EXIT1 ("lstat (\"%s\"): %m", path);
    }

  if (S_ISDIR (st.st_mode))
    TEST_COMPARE (rmdir (path), 0);
  else
    TEST_COMPARE (unlink (path), 0);
}

static void
check_nameserver (const struct resolver_snapshot *snapshot, int index,
                  const char *address)
{
  struct in_addr expected;
  TEST_COMPARE (inet_pton (AF_INET, address, &expected), 1);
  TEST_COMPARE (snapshot->nsaddr_list[index].sin_family, AF_INET);
  TEST_COMPARE (snapshot->nsaddr_list[index].sin_addr.s_addr,
                expected.s_addr);
  TEST_COMPARE (snapshot->nsaddr_list[index].sin_port, htons (53));
}

static void
check_config_one (const struct resolver_snapshot *snapshot)
{
  TEST_COMPARE (snapshot->retrans, 3);
  TEST_COMPARE (snapshot->retry, 2);
  TEST_COMPARE (snapshot->ndots, 4);
  TEST_COMPARE (snapshot->nscount, 1);
  TEST_VERIFY (snapshot->options & RES_ROTATE);
  TEST_VERIFY (strcmp (snapshot->dnsrch[0], "corp.example.com") == 0);
  TEST_VERIFY (strcmp (snapshot->dnsrch[1], "example.com") == 0);
  TEST_VERIFY (snapshot->dnsrch[2][0] == '\0');
  check_nameserver (snapshot, 0, "192.0.2.1");
}

static void
check_config_two (const struct resolver_snapshot *snapshot)
{
  TEST_COMPARE (snapshot->retrans, 5);
  TEST_COMPARE (snapshot->retry, 4);
  TEST_COMPARE (snapshot->ndots, 2);
  TEST_COMPARE (snapshot->nscount, 2);
  TEST_VERIFY (snapshot->options & RES_SNGLKUP);
  TEST_VERIFY (strcmp (snapshot->dnsrch[0], "example.net") == 0);
  TEST_VERIFY (strcmp (snapshot->dnsrch[1], "example.org") == 0);
  TEST_VERIFY (snapshot->dnsrch[2][0] == '\0');
  check_nameserver (snapshot, 0, "192.0.2.2");
  check_nameserver (snapshot, 1, "192.0.2.3");
}

static void
run_test_in_subprocess (void *closure)
{
  xchroot (chroot_env->path_chroot);
  unsetenv ("LOCALDOMAIN");
  unsetenv ("RES_OPTIONS");

  struct resolver_snapshot empty;
  load_snapshot (&empty);

  remove_path_if_exists (_PATH_RESCONF);
  struct resolver_snapshot missing;
  load_snapshot (&missing);
  check_same_snapshot ("empty file", &empty, "missing file", &missing);

  TEST_COMPARE (symlink ("does-not-exist", _PATH_RESCONF), 0);
  struct resolver_snapshot dangling;
  load_snapshot (&dangling);
  check_same_snapshot ("empty file", &empty, "dangling symlink", &dangling);
  remove_path_if_exists (_PATH_RESCONF);

  support_write_file_string (_PATH_RESCONF, "");
  TEST_COMPARE (chmod (_PATH_RESCONF, 0), 0);
  struct resolver_snapshot unreadable;
  load_snapshot (&unreadable);
  check_same_snapshot ("empty file", &empty, "unreadable file", &unreadable);
  remove_path_if_exists (_PATH_RESCONF);

  TEST_COMPARE (mkdir (_PATH_RESCONF, 0777), 0);
  struct resolver_snapshot directory;
  load_snapshot (&directory);
  check_same_snapshot ("empty file", &empty, "directory", &directory);
  remove_path_if_exists (_PATH_RESCONF);

  support_write_file_string (_PATH_RESCONF, resolv_conf_one);
  struct resolver_snapshot direct;
  load_snapshot (&direct);
  check_different_snapshot ("empty file", &empty, "configured file", &direct);
  check_config_one (&direct);

  support_write_file_string ("/etc/resolv.target1", resolv_conf_one);
  remove_path_if_exists (_PATH_RESCONF);
  TEST_COMPARE (symlink ("resolv.target1", _PATH_RESCONF), 0);
  struct resolver_snapshot via_symlink;
  load_snapshot (&via_symlink);
  check_same_snapshot ("configured file", &direct,
                       "symlink to configured file", &via_symlink);

  support_write_file_string ("/etc/resolv.target2", resolv_conf_two);
  remove_path_if_exists (_PATH_RESCONF);
  TEST_COMPARE (symlink ("resolv.target2", _PATH_RESCONF), 0);
  struct resolver_snapshot reloaded;
  load_snapshot (&reloaded);
  check_different_snapshot ("configured file", &direct,
                            "reloaded configured file", &reloaded);
  check_config_two (&reloaded);

  remove_path_if_exists (_PATH_RESCONF);
  support_write_file_string (_PATH_RESCONF, "");
  struct resolver_snapshot restored_empty;
  load_snapshot (&restored_empty);
  check_same_snapshot ("empty file", &empty,
                       "restored empty file", &restored_empty);

  remove_path_if_exists ("/etc/resolv.target1");
  remove_path_if_exists ("/etc/resolv.target2");
}

static int
do_test (void)
{
  support_become_root ();
  if (!support_can_chroot ())
    return EXIT_UNSUPPORTED;

  chroot_env = support_chroot_create
    ((struct support_chroot_configuration)
     {
       .resolv_conf = "",
     });

  support_isolate_in_subprocess (run_test_in_subprocess, NULL);
  support_chroot_free (chroot_env);
  return 0;
}

#define TIMEOUT 10
#include <support/test-driver.c>
