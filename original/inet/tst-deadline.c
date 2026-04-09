/* Tests for public absolute-deadline timeout handling.
   Copyright (C) 2017-2024 Free Software Foundation, Inc.
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

#include <time.h>
#include <support/check.h>

static struct timespec
monotonic_now (void)
{
  struct timespec now;
  TEST_COMPARE (clock_gettime (CLOCK_MONOTONIC, &now), 0);
  return now;
}

static struct timespec
add_ns (struct timespec base, long long delta_ns)
{
  long long nsec = (long long) base.tv_nsec + delta_ns;
  base.tv_sec += nsec / 1000000000LL;
  nsec %= 1000000000LL;
  if (nsec < 0)
    {
      --base.tv_sec;
      nsec += 1000000000LL;
    }
  base.tv_nsec = nsec;
  return base;
}

static long long
diff_ns (struct timespec left, struct timespec right)
{
  return ((long long) left.tv_sec - right.tv_sec) * 1000000000LL
    + (left.tv_nsec - right.tv_nsec);
}

static void
expect_wait_between (struct timespec deadline,
                     long long min_ns, long long max_ns)
{
  struct timespec before = monotonic_now ();
  TEST_COMPARE (clock_nanosleep (CLOCK_MONOTONIC, TIMER_ABSTIME,
                                 &deadline, NULL), 0);
  struct timespec after = monotonic_now ();
  long long elapsed = diff_ns (after, before);
  TEST_VERIFY (elapsed >= min_ns);
  TEST_VERIFY (elapsed < max_ns);
}

static int
do_test (void)
{
  struct timespec now = monotonic_now ();
  TEST_VERIFY (now.tv_sec >= 0);
  TEST_VERIFY (now.tv_sec > 0 || now.tv_nsec > 0);

  expect_wait_between (add_ns (monotonic_now (), -1000 * 1000),
                       0, 50LL * 1000 * 1000);
  expect_wait_between (monotonic_now (),
                       0, 50LL * 1000 * 1000);
  expect_wait_between (add_ns (monotonic_now (), 2LL * 1000 * 1000),
                       1LL * 1000 * 1000, 200LL * 1000 * 1000);

  return 0;
}

#include <support/test-driver.c>
