/* Test public libc entry points that use allocate_once internally.
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

#include <errno.h>
#include <mcheck.h>
#include <mntent.h>
#include <pwd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <support/check.h>
#include <support/support.h>
#include <support/xthread.h>
#include <unistd.h>

static pthread_barrier_t start_barrier;

static FILE *
make_mount_stream (const char *contents)
{
  FILE *fp = tmpfile ();
  TEST_VERIFY_EXIT (fp != NULL);
  TEST_VERIFY_EXIT (fputs (contents, fp) >= 0);
  rewind (fp);
  return fp;
}

static void
check_getmntent_shared_buffer (void)
{
  FILE *first = make_mount_stream ("/dev/one /mnt/one ext4 defaults 0 1\n");
  FILE *second
    = make_mount_stream ("/dev/two /mnt/two xfs ro,nosuid 1 2\n");

  struct mntent *entry = getmntent (first);
  TEST_VERIFY_EXIT (entry != NULL);
  TEST_COMPARE_STRING (entry->mnt_fsname, "/dev/one");
  TEST_COMPARE_STRING (entry->mnt_dir, "/mnt/one");
  TEST_COMPARE_STRING (entry->mnt_type, "ext4");
  TEST_COMPARE_STRING (entry->mnt_opts, "defaults");
  TEST_COMPARE (entry->mnt_freq, 0);
  TEST_COMPARE (entry->mnt_passno, 1);

  struct mntent *same_entry = getmntent (second);
  TEST_VERIFY_EXIT (same_entry != NULL);
  TEST_VERIFY (same_entry == entry);
  TEST_COMPARE_STRING (same_entry->mnt_fsname, "/dev/two");
  TEST_COMPARE_STRING (same_entry->mnt_dir, "/mnt/two");
  TEST_COMPARE_STRING (same_entry->mnt_type, "xfs");
  TEST_COMPARE_STRING (same_entry->mnt_opts, "ro,nosuid");
  TEST_COMPARE (same_entry->mnt_freq, 1);
  TEST_COMPARE (same_entry->mnt_passno, 2);

  TEST_COMPARE (fclose (second), 0);
  TEST_COMPARE (fclose (first), 0);
}

static size_t
password_buffer_size (void)
{
  long result = sysconf (_SC_GETPW_R_SIZE_MAX);
  return result > 0 ? result : 4096;
}

static void *
threaded_getpwuid (void *closure)
{
  TEST_VERIFY (closure == NULL);
  xpthread_barrier_wait (&start_barrier);

  size_t buflen = password_buffer_size ();
  char *buffer = xmalloc (buflen);
  char *name = NULL;

  for (int i = 0; i < 32; ++i)
    {
      struct passwd pwd;
      struct passwd *result = NULL;
      int ret;
      while ((ret = getpwuid_r (0, &pwd, buffer, buflen, &result)) == ERANGE)
        {
          buflen *= 2;
          buffer = xrealloc (buffer, buflen);
        }
      TEST_COMPARE (ret, 0);
      TEST_VERIFY_EXIT (result != NULL);
      TEST_COMPARE (result->pw_uid, (uid_t) 0);
      TEST_VERIFY (result->pw_name != NULL);
      TEST_VERIFY (result->pw_name[0] != '\0');

      if (name == NULL)
        name = xstrdup (result->pw_name);
      else
        TEST_COMPARE_STRING (name, result->pw_name);
    }

  free (buffer);
  return name;
}

static void
check_concurrent_getpwuid_r (void)
{
  enum { thread_count = 8 };
  pthread_t threads[thread_count];

  xpthread_barrier_init (&start_barrier, NULL, thread_count + 1);
  for (int i = 0; i < thread_count; ++i)
    threads[i] = xpthread_create (NULL, threaded_getpwuid, NULL);

  xpthread_barrier_wait (&start_barrier);

  char *first_name = NULL;
  for (int i = 0; i < thread_count; ++i)
    {
      char *name = xpthread_join (threads[i]);
      TEST_VERIFY_EXIT (name != NULL);
      if (first_name == NULL)
        first_name = name;
      else
        {
          TEST_COMPARE_STRING (first_name, name);
          free (name);
        }
    }
  xpthread_barrier_destroy (&start_barrier);
  free (first_name);
}

static int
do_test (void)
{
  mtrace ();

  check_getmntent_shared_buffer ();
  check_concurrent_getpwuid_r ();

  return 0;
}

#include <support/test-driver.c>
