/* Tests for public timeout primitives related to deadline handling.
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

#include <limits.h>
#include <poll.h>
#include <stdbool.h>
#include <stdint.h>
#include <support/check.h>
#include <support/xunistd.h>
#include <sys/time.h>
#include <time.h>

struct public_deadline
{
  bool infinite;
  struct timeval absolute;
};

/* Find the maximum value which can be represented in time_t.  */
static time_t
time_t_max (void)
{
  _Static_assert (0 > (time_t) -1, "time_t is signed");
  uintmax_t current = 1;
  while (true)
    {
      uintmax_t next = current * 2;
      TEST_VERIFY_EXIT (next > current);
      ++next;
      if ((time_t) next < 0 || next != (uintmax_t) (time_t) next)
        return current;
      current = next;
    }
}

static int
compare_timeval (struct timeval left, struct timeval right)
{
  if (timercmp (&left, &right, <))
    return -1;
  if (timercmp (&left, &right, >))
    return 1;
  return 0;
}

static struct timeval
current_time_now (void)
{
  struct timespec ts;
  if (clock_gettime (CLOCK_MONOTONIC, &ts) == 0)
    {
      TEST_VERIFY (ts.tv_sec >= 0);
      return (struct timeval)
        {
          .tv_sec = ts.tv_sec,
          .tv_usec = ts.tv_nsec / 1000,
        };
    }

  struct timeval tv;
  TEST_COMPARE (gettimeofday (&tv, NULL), 0);
  TEST_VERIFY (tv.tv_sec >= 0);
  return tv;
}

static struct public_deadline
deadline_from_timeout (struct timeval current, struct timeval timeout)
{
  TEST_VERIFY_EXIT (timeout.tv_sec >= 0);
  TEST_VERIFY_EXIT (timeout.tv_usec >= 0);
  TEST_VERIFY_EXIT (timeout.tv_usec < 1000 * 1000);

  uintmax_t sec = current.tv_sec;
  sec += timeout.tv_sec;
  if (sec < (uintmax_t) timeout.tv_sec)
    return (struct public_deadline) { .infinite = true };

  int usec = current.tv_usec + timeout.tv_usec;
  if (usec >= 1000 * 1000)
    {
      usec -= 1000 * 1000;
      if (sec + 1 < sec)
        return (struct public_deadline) { .infinite = true };
      ++sec;
    }

  if ((time_t) sec < 0 || sec != (uintmax_t) (time_t) sec)
    return (struct public_deadline) { .infinite = true };

  return (struct public_deadline)
    {
      .absolute =
        {
          .tv_sec = (time_t) sec,
          .tv_usec = usec,
        },
    };
}

static int
deadline_to_ms (struct timeval current, struct public_deadline deadline)
{
  if (deadline.infinite)
    return INT_MAX;

  if (compare_timeval (current, deadline.absolute) >= 0)
    return 0;

  time_t sec = deadline.absolute.tv_sec - current.tv_sec;
  if (sec >= INT_MAX)
    return INT_MAX;

  int usec = deadline.absolute.tv_usec - current.tv_usec;
  if (usec < 0)
    {
      TEST_VERIFY_EXIT (sec > 0);
      --sec;
      usec += 1000 * 1000;
    }

  usec += 999;
  if (usec >= 1000 * 1000)
    {
      TEST_VERIFY_EXIT (sec < INT_MAX);
      ++sec;
      usec -= 1000 * 1000;
    }

  unsigned int msec = (unsigned int) usec / 1000;
  if (sec > INT_MAX / 1000)
    return INT_MAX;
  msec += sec * 1000;
  if (msec > INT_MAX)
    return INT_MAX;
  return (int) msec;
}

static struct public_deadline
first_deadline (struct public_deadline left, struct public_deadline right)
{
  if (right.infinite || compare_timeval (left.absolute, right.absolute) < 0)
    return left;
  return right;
}

static double
get_ticks (void)
{
  struct timeval tv;
  TEST_COMPARE (gettimeofday (&tv, NULL), 0);
  return tv.tv_sec + tv.tv_usec * 1e-6;
}

static void
check_poll_timeout (int timeout_ms, double lower, double upper)
{
  int pipefd[2];
  xpipe (pipefd);
  struct pollfd fd =
    {
      .fd = pipefd[0],
      .events = POLLIN,
    };
  double before = get_ticks ();
  TEST_COMPARE (poll (&fd, 1, timeout_ms), 0);
  double after = get_ticks ();
  TEST_VERIFY (lower <= after - before);
  TEST_VERIFY (after - before < upper);
  xclose (pipefd[1]);
  xclose (pipefd[0]);
}

static int
do_test (void)
{
  {
    struct timeval current = current_time_now ();
    struct timeval next = current_time_now ();
    TEST_VERIFY (current.tv_sec >= 0);
    TEST_VERIFY (compare_timeval (next, current) >= 0);
    TEST_VERIFY (next.tv_sec > 0 || next.tv_usec > 0);
  }

  struct timeval current = { 1, 123456 };
  struct public_deadline deadline
    = deadline_from_timeout (current, (struct timeval) { 0, 1 });
  TEST_VERIFY (!deadline.infinite);
  TEST_COMPARE (deadline.absolute.tv_sec, (time_t) 1);
  TEST_COMPARE (deadline.absolute.tv_usec, 123457);
  TEST_COMPARE (deadline_to_ms (current, deadline), 1);

  deadline = deadline_from_timeout (current, (struct timeval) { 0, 2 });
  TEST_COMPARE (deadline.absolute.tv_sec, (time_t) 1);
  TEST_COMPARE (deadline.absolute.tv_usec, 123458);
  TEST_COMPARE (deadline_to_ms (current, deadline), 1);

  deadline = deadline_from_timeout (current, (struct timeval) { 1, 0 });
  TEST_COMPARE (deadline.absolute.tv_sec, (time_t) 2);
  TEST_COMPARE (deadline.absolute.tv_usec, 123456);
  TEST_COMPARE (deadline_to_ms (current, deadline), 1000);

  for (int i = 0; i < 999; ++i)
    {
      ++current.tv_usec;
      TEST_COMPARE (deadline_to_ms (current, deadline), 1000);
    }

  ++current.tv_usec;
  TEST_COMPARE (deadline_to_ms (current, deadline), 999);

  current = (struct timeval) { 9, 123456 };
  deadline = (struct public_deadline) { .absolute = { 10, 122456 } };
  TEST_COMPARE (deadline_to_ms (current, deadline), 999);
  deadline = (struct public_deadline) { .absolute = { 10, 122457 } };
  TEST_COMPARE (deadline_to_ms (current, deadline), 1000);
  deadline = (struct public_deadline) { .absolute = { 10, 123455 } };
  TEST_COMPARE (deadline_to_ms (current, deadline), 1000);
  deadline = (struct public_deadline) { .absolute = { 10, 123456 } };
  TEST_COMPARE (deadline_to_ms (current, deadline), 1000);

  deadline = (struct public_deadline) { .absolute = { INT_MAX - 1, 1 } };
  TEST_COMPARE (deadline_to_ms (current, deadline), INT_MAX);

  current = (struct timeval) { 9, 123456 };
  deadline.absolute = current;
  TEST_COMPARE (deadline_to_ms (current, deadline), 0);
  current = (struct timeval) { 9, 123457 };
  TEST_COMPARE (deadline_to_ms (current, deadline), 0);
  current = (struct timeval) { 10, 0 };
  TEST_COMPARE (deadline_to_ms (current, deadline), 0);
  current = (struct timeval) { 10, 123455 };
  TEST_COMPARE (deadline_to_ms (current, deadline), 0);
  current = (struct timeval) { 10, 123456 };
  TEST_COMPARE (deadline_to_ms (current, deadline), 0);

  current = (struct timeval) { 9, 998000 };
  for (int i = 0; i < 2000; ++i)
    {
      deadline = deadline_from_timeout (current, (struct timeval) { 1, i });
      TEST_COMPARE (deadline.absolute.tv_sec, (time_t) 10);
      TEST_COMPARE (deadline.absolute.tv_usec, 998000 + i);
    }
  for (int i = 2000; i < 3000; ++i)
    {
      deadline = deadline_from_timeout (current, (struct timeval) { 2, i });
      TEST_COMPARE (deadline.absolute.tv_sec, (time_t) 12);
      TEST_COMPARE (deadline.absolute.tv_usec, i - 2000);
    }

  deadline = deadline_from_timeout ((struct timeval) { 0, 999999 },
                                    (struct timeval) { time_t_max (), 1 });
  TEST_VERIFY (deadline.infinite);
  deadline = deadline_from_timeout ((struct timeval) { 0, 999998 },
                                    (struct timeval) { time_t_max (), 1 });
  TEST_VERIFY (!deadline.infinite);
  deadline = deadline_from_timeout ((struct timeval) { time_t_max (), 999999 },
                                    (struct timeval) { 0, 1 });
  TEST_VERIFY (deadline.infinite);
  deadline = deadline_from_timeout ((struct timeval) { time_t_max () / 2 + 1, 0 },
                                    (struct timeval) { time_t_max () / 2 + 1, 0 });
  TEST_VERIFY (deadline.infinite);

  deadline = first_deadline ((struct public_deadline) { .absolute = { 1, 2 } },
                             (struct public_deadline) { .absolute = { 1, 3 } });
  TEST_COMPARE (deadline.absolute.tv_sec, (time_t) 1);
  TEST_COMPARE (deadline.absolute.tv_usec, 2);
  deadline = first_deadline ((struct public_deadline) { .absolute = { 1, 3 } },
                             (struct public_deadline) { .absolute = { 1, 2 } });
  TEST_COMPARE (deadline.absolute.tv_sec, (time_t) 1);
  TEST_COMPARE (deadline.absolute.tv_usec, 2);
  deadline = first_deadline ((struct public_deadline) { .absolute = { 1, 2 } },
                             (struct public_deadline) { .absolute = { 2, 1 } });
  TEST_COMPARE (deadline.absolute.tv_sec, (time_t) 1);
  TEST_COMPARE (deadline.absolute.tv_usec, 2);
  deadline = first_deadline ((struct public_deadline) { .absolute = { 1, 2 } },
                             (struct public_deadline) { .absolute = { 2, 4 } });
  TEST_COMPARE (deadline.absolute.tv_sec, (time_t) 1);
  TEST_COMPARE (deadline.absolute.tv_usec, 2);
  deadline = first_deadline ((struct public_deadline) { .absolute = { 2, 4 } },
                             (struct public_deadline) { .absolute = { 1, 2 } });
  TEST_COMPARE (deadline.absolute.tv_sec, (time_t) 1);
  TEST_COMPARE (deadline.absolute.tv_usec, 2);

  check_poll_timeout (0, 0.0, 0.1);
  check_poll_timeout (50, 0.03, 0.5);

  return 0;
}

#include <support/test-driver.c>
