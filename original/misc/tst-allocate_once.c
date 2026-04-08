/* Test one-time public initialization primitives.
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

#include <mcheck.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <support/check.h>
#include <support/support.h>
#include <support/xthread.h>

static pthread_once_t once_1 = PTHREAD_ONCE_INIT;
static pthread_once_t once_2 = PTHREAD_ONCE_INIT;
static pthread_once_t once_concurrent = PTHREAD_ONCE_INIT;
static atomic_int init_calls_1 = ATOMIC_VAR_INIT (0);
static atomic_int init_calls_2 = ATOMIC_VAR_INIT (0);
static atomic_int init_calls_concurrent = ATOMIC_VAR_INIT (0);
static char *string_1;
static char *string_2;
static char *string_concurrent;
static pthread_barrier_t start_barrier;

static void
init_string_1 (void)
{
  if (atomic_fetch_add_explicit (&init_calls_1, 1, memory_order_relaxed) != 0)
    FAIL_EXIT1 ("first initializer ran more than once");
  string_1 = xstrdup ("test string 1");
}

static void
init_string_2 (void)
{
  if (atomic_fetch_add_explicit (&init_calls_2, 1, memory_order_relaxed) != 0)
    FAIL_EXIT1 ("second initializer ran more than once");
  string_2 = xstrdup ("test string 2");
}

static void
init_string_concurrent (void)
{
  if (atomic_fetch_add_explicit (&init_calls_concurrent, 1,
                                 memory_order_relaxed) != 0)
    FAIL_EXIT1 ("concurrent initializer ran more than once");
  string_concurrent = xstrdup ("threaded string");
}

static char *
get_string_1 (void)
{
  xpthread_once (&once_1, init_string_1);
  return string_1;
}

static char *
get_string_2 (void)
{
  xpthread_once (&once_2, init_string_2);
  return string_2;
}

static void *
threaded_getter (void *closure)
{
  TEST_VERIFY (closure == NULL);
  xpthread_barrier_wait (&start_barrier);
  xpthread_once (&once_concurrent, init_string_concurrent);
  return string_concurrent;
}

static int
do_test (void)
{
  mtrace ();

  char *first = get_string_1 ();
  TEST_VERIFY_EXIT (first != NULL);
  TEST_COMPARE (strcmp (first, "test string 1"), 0);
  TEST_VERIFY (first == get_string_1 ());
  TEST_COMPARE (atomic_load_explicit (&init_calls_1, memory_order_relaxed), 1);

  char *second = get_string_2 ();
  TEST_VERIFY_EXIT (second != NULL);
  TEST_COMPARE (strcmp (second, "test string 2"), 0);
  TEST_VERIFY (second == get_string_2 ());
  TEST_VERIFY (first != second);
  TEST_COMPARE (atomic_load_explicit (&init_calls_2, memory_order_relaxed), 1);

  enum { thread_count = 8 };
  pthread_t threads[thread_count];
  xpthread_barrier_init (&start_barrier, NULL, thread_count + 1);
  for (int i = 0; i < thread_count; ++i)
    threads[i] = xpthread_create (NULL, threaded_getter, NULL);

  xpthread_barrier_wait (&start_barrier);
  for (int i = 0; i < thread_count; ++i)
    TEST_VERIFY (xpthread_join (threads[i]) == string_concurrent);
  xpthread_barrier_destroy (&start_barrier);

  TEST_VERIFY_EXIT (string_concurrent != NULL);
  TEST_COMPARE (strcmp (string_concurrent, "threaded string"), 0);
  TEST_COMPARE (atomic_load_explicit (&init_calls_concurrent,
                                      memory_order_relaxed), 1);

  free (string_concurrent);
  free (second);
  free (first);
  return 0;
}

#include <support/test-driver.c>
