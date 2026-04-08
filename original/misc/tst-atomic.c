/* Tests for public C11 atomics.
   Copyright (C) 2003-2024 Free Software Foundation, Inc.
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
#include <stdbool.h>
#include <stdatomic.h>
#include <support/check.h>

#ifndef TEST_ATOMIC_TYPE
# define TEST_ATOMIC_TYPE atomic_int
#endif

#ifndef TEST_VALUE_TYPE
# define TEST_VALUE_TYPE int
#endif

static TEST_VALUE_TYPE
bit_mask (unsigned int bit)
{
  return (TEST_VALUE_TYPE) (((unsigned long long) 1) << bit);
}

static TEST_VALUE_TYPE
decrement_if_positive (TEST_ATOMIC_TYPE *mem)
{
  TEST_VALUE_TYPE current = atomic_load_explicit (mem, memory_order_relaxed);

  while (current > 0)
    {
      TEST_VALUE_TYPE desired = current - 1;
      if (atomic_compare_exchange_weak_explicit (mem, &current, desired,
                                                 memory_order_acq_rel,
                                                 memory_order_relaxed))
        return current;
    }

  return current;
}

static bool
add_negative (TEST_ATOMIC_TYPE *mem, TEST_VALUE_TYPE delta)
{
  TEST_VALUE_TYPE new_value
    = atomic_fetch_add_explicit (mem, delta, memory_order_acq_rel) + delta;
  return new_value < 0;
}

static bool
add_zero (TEST_ATOMIC_TYPE *mem, TEST_VALUE_TYPE delta)
{
  TEST_VALUE_TYPE new_value
    = atomic_fetch_add_explicit (mem, delta, memory_order_acq_rel) + delta;
  return new_value == 0;
}

static bool
bit_test_set (TEST_ATOMIC_TYPE *mem, unsigned int bit)
{
  TEST_VALUE_TYPE mask = bit_mask (bit);
  return (atomic_fetch_or_explicit (mem, mask, memory_order_acq_rel) & mask)
         != 0;
}

static void
check_compare_exchange_weak (TEST_ATOMIC_TYPE *mem, TEST_VALUE_TYPE initial,
                             TEST_VALUE_TYPE desired, memory_order success)
{
  TEST_VALUE_TYPE expected = initial;

  atomic_store_explicit (mem, initial, memory_order_relaxed);
  while (!atomic_compare_exchange_weak_explicit (mem, &expected, desired,
                                                 success,
                                                 memory_order_relaxed))
    TEST_COMPARE (expected, initial);

  TEST_COMPARE (atomic_load_explicit (mem, memory_order_relaxed), desired);
  TEST_COMPARE (expected, initial);
}

static int
do_test (void)
{
  TEST_ATOMIC_TYPE mem = ATOMIC_VAR_INIT (0);
  TEST_VALUE_TYPE expected;

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 11, memory_order_relaxed);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 11);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_acquire),
                (TEST_VALUE_TYPE) 11);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 12, memory_order_relaxed);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 12);
  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 13, memory_order_release);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 13);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 24, memory_order_relaxed);
  expected = 24;
  TEST_VERIFY (atomic_compare_exchange_strong_explicit (&mem, &expected, 35,
                                                        memory_order_acquire,
                                                        memory_order_relaxed));
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 35);
  TEST_COMPARE (expected, (TEST_VALUE_TYPE) 24);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 12, memory_order_relaxed);
  expected = 15;
  TEST_VERIFY (!atomic_compare_exchange_strong_explicit (&mem, &expected, 10,
                                                         memory_order_acquire,
                                                         memory_order_relaxed));
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 12);
  TEST_COMPARE (expected, (TEST_VALUE_TYPE) 12);

  check_compare_exchange_weak (&mem, (TEST_VALUE_TYPE) 14,
                               (TEST_VALUE_TYPE) 25, memory_order_relaxed);
  expected = 14;
  TEST_VERIFY (!atomic_compare_exchange_weak_explicit (&mem, &expected, 14,
                                                       memory_order_relaxed,
                                                       memory_order_relaxed));
  TEST_COMPARE (expected, (TEST_VALUE_TYPE) 25);

  check_compare_exchange_weak (&mem, (TEST_VALUE_TYPE) 14,
                               (TEST_VALUE_TYPE) 25, memory_order_acquire);
  check_compare_exchange_weak (&mem, (TEST_VALUE_TYPE) 14,
                               (TEST_VALUE_TYPE) 25, memory_order_release);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 64, memory_order_relaxed);
  TEST_COMPARE (atomic_exchange_explicit (&mem, (TEST_VALUE_TYPE) 31,
                                          memory_order_acq_rel),
                (TEST_VALUE_TYPE) 64);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 31);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 2, memory_order_relaxed);
  TEST_COMPARE (atomic_fetch_add_explicit (&mem, (TEST_VALUE_TYPE) 11,
                                           memory_order_acq_rel),
                (TEST_VALUE_TYPE) 2);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 13);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) -21, memory_order_relaxed);
  atomic_fetch_add_explicit (&mem, (TEST_VALUE_TYPE) 22, memory_order_relaxed);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 1);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) -1, memory_order_relaxed);
  atomic_fetch_add_explicit (&mem, (TEST_VALUE_TYPE) 1, memory_order_relaxed);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 0);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 2, memory_order_relaxed);
  TEST_COMPARE (atomic_fetch_add_explicit (&mem, (TEST_VALUE_TYPE) 1,
                                           memory_order_relaxed) + 1,
                (TEST_VALUE_TYPE) 3);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 17, memory_order_relaxed);
  atomic_fetch_sub_explicit (&mem, (TEST_VALUE_TYPE) 1, memory_order_relaxed);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 16);
  TEST_COMPARE (atomic_fetch_sub_explicit (&mem, (TEST_VALUE_TYPE) 1,
                                           memory_order_relaxed) - 1,
                (TEST_VALUE_TYPE) 15);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 1, memory_order_relaxed);
  TEST_COMPARE (decrement_if_positive (&mem), (TEST_VALUE_TYPE) 1);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 0);
  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 0, memory_order_relaxed);
  TEST_COMPARE (decrement_if_positive (&mem), (TEST_VALUE_TYPE) 0);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 0);
  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) -1, memory_order_relaxed);
  TEST_COMPARE (decrement_if_positive (&mem), (TEST_VALUE_TYPE) -1);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) -1);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) -12, memory_order_relaxed);
  TEST_VERIFY (add_negative (&mem, (TEST_VALUE_TYPE) 10));
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) -2);
  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 0, memory_order_relaxed);
  TEST_VERIFY (!add_negative (&mem, (TEST_VALUE_TYPE) 100));
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 100);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) -36, memory_order_relaxed);
  TEST_VERIFY (add_zero (&mem, (TEST_VALUE_TYPE) 36));
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 0);
  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 10, memory_order_relaxed);
  TEST_VERIFY (!add_zero (&mem, (TEST_VALUE_TYPE) -20));
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) -10);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 0, memory_order_relaxed);
  atomic_fetch_or_explicit (&mem, bit_mask (1), memory_order_relaxed);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                bit_mask (1));
  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 8, memory_order_relaxed);
  atomic_fetch_or_explicit (&mem, bit_mask (3), memory_order_relaxed);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 8);

  if (sizeof (TEST_VALUE_TYPE) >= 8)
    {
      atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 16, memory_order_relaxed);
      atomic_fetch_or_explicit (&mem, bit_mask (35), memory_order_relaxed);
      TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                    (TEST_VALUE_TYPE) 0x800000010LL);
    }

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 0, memory_order_relaxed);
  TEST_VERIFY (!bit_test_set (&mem, 1));
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                bit_mask (1));
  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 8, memory_order_relaxed);
  TEST_VERIFY (bit_test_set (&mem, 3));
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 8);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 3, memory_order_relaxed);
  TEST_COMPARE (atomic_fetch_and_explicit (&mem, (TEST_VALUE_TYPE) 2,
                                           memory_order_acquire),
                (TEST_VALUE_TYPE) 3);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 2);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 4, memory_order_relaxed);
  TEST_COMPARE (atomic_fetch_or_explicit (&mem, (TEST_VALUE_TYPE) 2,
                                          memory_order_relaxed),
                (TEST_VALUE_TYPE) 4);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 6);

  atomic_store_explicit (&mem, (TEST_VALUE_TYPE) 7, memory_order_relaxed);
  TEST_COMPARE (atomic_fetch_xor_explicit (&mem, (TEST_VALUE_TYPE) 3,
                                           memory_order_acq_rel),
                (TEST_VALUE_TYPE) 7);
  TEST_COMPARE (atomic_load_explicit (&mem, memory_order_relaxed),
                (TEST_VALUE_TYPE) 4);

  atomic_thread_fence (memory_order_acquire);
  atomic_thread_fence (memory_order_release);
  atomic_thread_fence (memory_order_seq_cst);

  return 0;
}

#include <support/test-driver.c>
