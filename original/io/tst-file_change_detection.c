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
#include <inttypes.h>
#include <resolv.h>
#include <sched.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <support/capture_subprocess.h>
#include <support/check.h>
#include <support/support.h>
#include <support/temp_file.h>
#include <support/test-driver.h>
#include <support/xunistd.h>
#include <unistd.h>

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
  TEST_COMPARE (left->retrans, right->retrans);
  TEST_COMPARE (left->retry, right->retry);
  TEST_COMPARE (left->options, right->options);
  TEST_COMPARE (left->nscount, right->nscount);
  TEST_COMPARE (left->ndots, right->ndots);
  TEST_COMPARE (left->nsort, right->nsort);
  TEST_COMPARE_STRING (left->defdname, right->defdname);

  for (int i = 0; i < left->nscount; ++i)
    {
      TEST_COMPARE (left->nsaddr_list[i].sin_family,
                    right->nsaddr_list[i].sin_family);
      TEST_COMPARE (left->nsaddr_list[i].sin_port,
                    right->nsaddr_list[i].sin_port);
      TEST_COMPARE (left->nsaddr_list[i].sin_addr.s_addr,
                    right->nsaddr_list[i].sin_addr.s_addr);
    }

  for (unsigned int i = 0; i < left->nsort; ++i)
    {
      TEST_COMPARE (left->sort_list[i].addr.s_addr,
                    right->sort_list[i].addr.s_addr);
      TEST_COMPARE (left->sort_list[i].mask, right->sort_list[i].mask);
    }

  for (int i = 0; i < MAXDNSRCH; ++i)
    TEST_COMPARE_STRING (left->dnsrch[i], right->dnsrch[i]);
}

static void
check_different_snapshot (const char *left_name,
                          const struct resolver_snapshot *left,
                          const char *right_name,
                          const struct resolver_snapshot *right)
{
  if (test_verbose > 0)
    printf ("info: ensuring %s and %s differ\n", left_name, right_name);
  bool same = left->retrans == right->retrans
    && left->retry == right->retry
    && left->options == right->options
    && left->nscount == right->nscount
    && left->ndots == right->ndots
    && left->nsort == right->nsort
    && strcmp (left->defdname, right->defdname) == 0;

  for (int i = 0; same && i < left->nscount; ++i)
    same = left->nsaddr_list[i].sin_family == right->nsaddr_list[i].sin_family
      && left->nsaddr_list[i].sin_port == right->nsaddr_list[i].sin_port
      && left->nsaddr_list[i].sin_addr.s_addr
         == right->nsaddr_list[i].sin_addr.s_addr;

  for (unsigned int i = 0; same && i < left->nsort; ++i)
    same = left->sort_list[i].addr.s_addr == right->sort_list[i].addr.s_addr
      && left->sort_list[i].mask == right->sort_list[i].mask;

  for (int i = 0; same && i < MAXDNSRCH; ++i)
    same = strcmp (left->dnsrch[i], right->dnsrch[i]) == 0;

  TEST_VERIFY (!same);
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
prepare_test_etc (const char *tempdir)
{
  char *etcdir = xasprintf ("%s/etc", tempdir);
  TEST_COMPARE (mkdir (etcdir, 0777), 0);
  char *hosts = xasprintf ("%s/hosts", etcdir);
  char *host_conf = xasprintf ("%s/host.conf", etcdir);
  char *aliases = xasprintf ("%s/aliases", etcdir);
  char *nsswitch = xasprintf ("%s/nsswitch.conf", etcdir);

  support_write_file_string (hosts, "127.0.0.1 localhost\n");
  support_write_file_string (host_conf, "");
  support_write_file_string (aliases, "");
  support_write_file_string (nsswitch, "hosts: files dns\n");

  free (nsswitch);
  free (aliases);
  free (host_conf);
  free (hosts);
  free (etcdir);
}

static int
run_fallback_smoke_test (void)
{
  unsetenv ("LOCALDOMAIN");
  unsetenv ("RES_OPTIONS");

  struct resolver_snapshot first;
  struct resolver_snapshot second;
  load_snapshot (&first);
  load_snapshot (&second);
  check_same_snapshot ("first public snapshot", &first,
                       "second public snapshot", &second);
  return 0;
}

static int
do_child_test (const char *tempdir, uid_t uid, gid_t gid)
{
#ifndef CLONE_NEWNS
  return EXIT_UNSUPPORTED;
#else
  char *etcdir = xasprintf ("%s/etc", tempdir);
  char *resolv_path = xasprintf ("%s/resolv.conf", etcdir);
  char *target_one = xasprintf ("%s/target-one", etcdir);
  char *target_two = xasprintf ("%s/target-two", etcdir);
  char *loop = xasprintf ("%s/loop", etcdir);

  if (unshare (CLONE_NEWNS) != 0)
    {
      if (errno == EPERM)
        return EXIT_UNSUPPORTED;
      FAIL_EXIT1 ("unshare (CLONE_NEWNS): %m");
    }
  if (mount ("none", "/", NULL, MS_REC | MS_PRIVATE, NULL) != 0)
    {
      if (errno == EPERM)
        return EXIT_UNSUPPORTED;
      FAIL_EXIT1 ("mount private /: %m");
    }
  TEST_COMPARE (mount (etcdir, "/etc", NULL, MS_BIND, NULL), 0);
  TEST_COMPARE (setgid (gid), 0);
  TEST_COMPARE (setuid (uid), 0);

  unsetenv ("LOCALDOMAIN");
  unsetenv ("RES_OPTIONS");

  support_write_file_string (resolv_path, "");
  struct resolver_snapshot empty;
  load_snapshot (&empty);

  remove_path_if_exists (resolv_path);
  struct resolver_snapshot missing;
  load_snapshot (&missing);
  check_same_snapshot ("empty file", &empty, "missing file", &missing);

  TEST_COMPARE (symlink ("does-not-exist", resolv_path), 0);
  struct resolver_snapshot dangling;
  load_snapshot (&dangling);
  check_same_snapshot ("empty file", &empty, "dangling symlink", &dangling);
  remove_path_if_exists (resolv_path);

  TEST_COMPARE (symlink ("loop", loop), 0);
  TEST_COMPARE (symlink ("loop", resolv_path), 0);
  struct resolver_snapshot looped;
  load_snapshot (&looped);
  check_same_snapshot ("empty file", &empty, "symbolic link loop", &looped);
  remove_path_if_exists (resolv_path);

  support_write_file_string (resolv_path, "");
  TEST_COMPARE (chmod (resolv_path, 0), 0);
  struct resolver_snapshot unreadable;
  load_snapshot (&unreadable);
  check_same_snapshot ("empty file", &empty, "unreadable file", &unreadable);
  remove_path_if_exists (resolv_path);

  TEST_COMPARE (mkdir (resolv_path, 0777), 0);
  struct resolver_snapshot directory;
  load_snapshot (&directory);
  check_same_snapshot ("empty file", &empty, "directory", &directory);
  remove_path_if_exists (resolv_path);

  support_write_file_string (resolv_path, resolv_conf_one);
  struct resolver_snapshot direct_one;
  load_snapshot (&direct_one);
  check_different_snapshot ("empty file", &empty,
                            "configured file one", &direct_one);
  check_config_one (&direct_one);

  support_write_file_string (target_one, resolv_conf_one);
  remove_path_if_exists (resolv_path);
  TEST_COMPARE (symlink ("target-one", resolv_path), 0);
  struct resolver_snapshot symlink_one;
  load_snapshot (&symlink_one);
  check_same_snapshot ("configured file one", &direct_one,
                       "symlink to file one", &symlink_one);

  remove_path_if_exists (resolv_path);
  support_write_file_string (resolv_path, resolv_conf_two);
  struct resolver_snapshot direct_two;
  load_snapshot (&direct_two);
  check_different_snapshot ("configured file one", &direct_one,
                            "configured file two", &direct_two);
  check_config_two (&direct_two);

  support_write_file_string (target_two, resolv_conf_two);
  remove_path_if_exists (resolv_path);
  TEST_COMPARE (symlink ("target-two", resolv_path), 0);
  struct resolver_snapshot symlink_two;
  load_snapshot (&symlink_two);
  check_same_snapshot ("configured file two", &direct_two,
                       "symlink to file two", &symlink_two);

  remove_path_if_exists (resolv_path);
  support_write_file_string (resolv_path, "");
  struct resolver_snapshot restored_empty;
  load_snapshot (&restored_empty);
  check_same_snapshot ("empty file", &empty,
                       "restored empty file", &restored_empty);

  free (loop);
  free (target_two);
  free (target_one);
  free (resolv_path);
  free (etcdir);
  return 0;
#endif
}

static bool
can_run_passwordless_sudo (void)
{
  static const char sudo_path[] = "/usr/bin/sudo";
  char *const argv[] = { (char *) sudo_path, (char *) "-n",
                         (char *) "true", NULL };
  struct support_capture_subprocess proc
    = support_capture_subprogram (sudo_path, argv);
  bool ok = WIFEXITED (proc.status) && WEXITSTATUS (proc.status) == 0;
  support_capture_subprocess_free (&proc);
  return ok;
}

static int
run_private_mount_child (const char *argv0)
{
  if (!can_run_passwordless_sudo ())
    {
      if (test_verbose > 0)
        puts ("warning: passwordless sudo unavailable, using public smoke test");
      return run_fallback_smoke_test ();
    }

  char *self = realpath (argv0, NULL);
  TEST_VERIFY_EXIT (self != NULL);
  char *tempdir = support_create_temp_directory ("tst-file-change-");
  prepare_test_etc (tempdir);
  static const char sudo_path[] = "/usr/bin/sudo";
  char *etcdir = xasprintf ("%s/etc", tempdir);
  char *hosts = xasprintf ("%s/hosts", etcdir);
  char *host_conf = xasprintf ("%s/host.conf", etcdir);
  char *aliases = xasprintf ("%s/aliases", etcdir);
  char *nsswitch = xasprintf ("%s/nsswitch.conf", etcdir);
  char *resolv_path = xasprintf ("%s/resolv.conf", etcdir);
  char *target_one = xasprintf ("%s/target-one", etcdir);
  char *target_two = xasprintf ("%s/target-two", etcdir);
  char *loop = xasprintf ("%s/loop", etcdir);

  char *uid = xasprintf ("%" PRIuMAX, (uintmax_t) getuid ());
  char *gid = xasprintf ("%" PRIuMAX, (uintmax_t) getgid ());
  char *const argv[] =
    {
      (char *) sudo_path,
      (char *) "-n",
      self,
      (char *) "--direct",
      (char *) "--",
      (char *) "--child",
      tempdir,
      uid,
      gid,
      NULL
    };

  struct support_capture_subprocess proc
    = support_capture_subprogram (sudo_path, argv);

  int result;
  if (WIFEXITED (proc.status) && WEXITSTATUS (proc.status) == 0)
    result = 0;
  else if (WIFEXITED (proc.status)
           && WEXITSTATUS (proc.status) == EXIT_UNSUPPORTED)
    {
      if (test_verbose > 0)
        puts ("warning: private mount namespace unavailable, using public smoke test");
      result = run_fallback_smoke_test ();
    }
  else
    {
      if (proc.out.buffer[0] != '\0')
        printf ("%s", proc.out.buffer);
      if (proc.err.buffer[0] != '\0')
        printf ("%s", proc.err.buffer);
      FAIL_EXIT1 ("sudo child failed with status %#x", proc.status);
    }

  support_capture_subprocess_free (&proc);
  remove_path_if_exists (resolv_path);
  remove_path_if_exists (target_one);
  remove_path_if_exists (target_two);
  remove_path_if_exists (loop);
  remove_path_if_exists (nsswitch);
  remove_path_if_exists (aliases);
  remove_path_if_exists (host_conf);
  remove_path_if_exists (hosts);
  remove_path_if_exists (etcdir);
  free (loop);
  free (target_two);
  free (target_one);
  free (resolv_path);
  free (nsswitch);
  free (aliases);
  free (host_conf);
  free (hosts);
  free (etcdir);
  free (gid);
  free (uid);
  free (tempdir);
  free (self);
  return result;
}

static int
do_test_argv (int argc, char **argv)
{
  for (int i = 1; i + 3 < argc; ++i)
    if (strcmp (argv[i], "--child") == 0)
      return do_child_test (argv[i + 1],
                            (uid_t) strtoumax (argv[i + 2], NULL, 10),
                            (gid_t) strtoumax (argv[i + 3], NULL, 10));

  return run_private_mount_child (argv[0]);
}

#define TEST_FUNCTION_ARGV do_test_argv
#define TIMEOUT 20
#include <support/test-driver.c>
