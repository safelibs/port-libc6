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
#include <stdbool.h>
#include <sys/wait.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <support/temp_file.h>
#include <support/check.h>
#include <support/support.h>
#include <support/xthread.h>
#include <support/xunistd.h>
#include <unistd.h>

static pthread_barrier_t getmntent_barrier;
static pthread_barrier_t getpwuid_barrier;

struct traced_mntent_buffer
{
  struct mntent m;
  char buffer[4096];
};

struct getmntent_trace
{
  int allocations;
  int frees;
};

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

static void *
threaded_first_getmntent (void *closure)
{
  FILE *stream = closure;

  xpthread_barrier_wait (&getmntent_barrier);
  TEST_VERIFY_EXIT (getmntent (stream) == NULL);
  TEST_VERIFY_EXIT (feof (stream));
  TEST_VERIFY_EXIT (!ferror (stream));

  return NULL;
}

static void
run_getmntent_first_use_race (const char *trace_path)
{
  static const char empty_contents[] = "";
  enum { thread_count = 64 };
  FILE *streams[thread_count];
  pthread_t threads[thread_count];

  xpthread_barrier_init (&getmntent_barrier, NULL, thread_count + 1);
  for (int i = 0; i < thread_count; ++i)
    {
      streams[i] = fmemopen ((void *) empty_contents, sizeof (empty_contents),
                             "r");
      TEST_VERIFY_EXIT (streams[i] != NULL);
      threads[i] = xpthread_create (NULL, threaded_first_getmntent,
                                    streams[i]);
    }

  TEST_COMPARE (setenv ("MALLOC_TRACE", trace_path, 1), 0);
  mtrace ();
  xpthread_barrier_wait (&getmntent_barrier);

  for (int i = 0; i < thread_count; ++i)
    {
      xpthread_join (threads[i]);
      TEST_COMPARE (fclose (streams[i]), 0);
    }
  xpthread_barrier_destroy (&getmntent_barrier);
}

static struct getmntent_trace
parse_getmntent_trace (const char *trace_path)
{
  enum { max_traced_allocations = 256 };
  struct
  {
    void *ptr;
    bool live;
  } allocations[max_traced_allocations];
  int pointer_count = 0;
  struct getmntent_trace result = { 0, 0 };

  FILE *fp = fopen (trace_path, "r");
  TEST_VERIFY_EXIT (fp != NULL);

  char *line = NULL;
  size_t linecap = 0;
  while (getline (&line, &linecap, fp) >= 0)
    {
      char *allocation = strstr (line, " + ");
      if (allocation != NULL)
        {
          void *ptr;
          size_t size;
          if (sscanf (allocation + 3, "%p %zx", &ptr, &size) == 2
              && size == sizeof (struct traced_mntent_buffer))
            {
              TEST_VERIFY_EXIT (pointer_count < max_traced_allocations);
              allocations[pointer_count].ptr = ptr;
              allocations[pointer_count].live = true;
              ++pointer_count;
            }
          continue;
        }

      char *deallocation = strstr (line, " - ");
      if (deallocation != NULL)
        {
          void *ptr;
          if (sscanf (deallocation + 3, "%p", &ptr) == 1)
            for (int i = pointer_count - 1; i >= 0; --i)
              if (allocations[i].live && allocations[i].ptr == ptr)
                {
                  allocations[i].live = false;
                  ++result.frees;
                  break;
                }
        }
    }

  free (line);
  TEST_COMPARE (fclose (fp), 0);

  result.allocations = pointer_count;
  return result;
}

static void
check_getmntent_lost_race (void)
{
  enum { max_attempts = 32 };

  for (int attempt = 0; attempt < max_attempts; ++attempt)
    {
      char *trace_path;
      int trace_fd = create_temp_file ("tst-allocate_once-trace-", &trace_path);
      TEST_VERIFY_EXIT (trace_fd >= 0);
      xclose (trace_fd);

      pid_t pid = xfork ();
      if (pid == 0)
        {
          run_getmntent_first_use_race (trace_path);
          exit (0);
        }

      int status;
      xwaitpid (pid, &status, 0);
      TEST_VERIFY_EXIT (WIFEXITED (status));
      TEST_COMPARE (WEXITSTATUS (status), 0);

      struct getmntent_trace trace = parse_getmntent_trace (trace_path);
      free (trace_path);

      if (trace.allocations >= 2)
        {
          TEST_COMPARE (trace.frees, trace.allocations);
          return;
        }
    }

  FAIL_EXIT1 ("concurrent first use of getmntent never triggered a lost race");
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
  xpthread_barrier_wait (&getpwuid_barrier);

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

  xpthread_barrier_init (&getpwuid_barrier, NULL, thread_count + 1);
  for (int i = 0; i < thread_count; ++i)
    threads[i] = xpthread_create (NULL, threaded_getpwuid, NULL);

  xpthread_barrier_wait (&getpwuid_barrier);

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
  xpthread_barrier_destroy (&getpwuid_barrier);
  free (first_name);
}

static int
do_test (void)
{
  check_getmntent_lost_race ();

  mtrace ();
  check_getmntent_shared_buffer ();
  check_concurrent_getpwuid_r ();

  return 0;
}

#include <support/test-driver.c>
