/* Basic locale-aware floating-point parsing tests via scanf.
   Copyright (C) 1991-2024 Free Software Foundation, Inc.
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

#include <ctype.h>
#include <locale.h>
#include <stddef.h>
#include <support/check.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <string.h>
#include <math.h>

static void
check_one (const char *input, double expected, ptrdiff_t expected_nread)
{
  double actual = 0.0;
  int nread = -1;
  int ret = sscanf (input, "%'lf%n", &actual, &nread);

  TEST_COMPARE (ret, 1);
  TEST_COMPARE (nread, expected_nread);
  if (actual != expected)
    FAIL_EXIT1 ("sscanf (\"%s\") returned %g, expected %g",
                input, actual, expected);
}

/* Perform a few tests in a locale with thousands separators.  */
static int
do_test (void)
{
  static const struct
  {
    const char *loc;
    const char *str;
    double exp;
    ptrdiff_t nread;
  } tests[] =
    {
      { "de_DE.UTF-8", "1,5", 1.5, 3 },
      { "de_DE.UTF-8", "1.5", 1.0, 3 },
      { "de_DE.UTF-8", "1.500", 1500.0, 5 },
      { "de_DE.UTF-8", "36.893.488.147.419.103.232", 0x1.0p65, 26 }
    };
#define ntests (sizeof (tests) / sizeof (tests[0]))
  size_t n;
  int result = 0;

  puts ("\nLocale tests");

  for (n = 0; n < ntests; ++n)
    {
      if (setlocale (LC_ALL, tests[n].loc) == NULL)
        FAIL_EXIT1 ("cannot set locale %s", tests[n].loc);

      check_one (tests[n].str, tests[n].exp, tests[n].nread);
    }

  return result ? EXIT_FAILURE : EXIT_SUCCESS;
}

#include <support/test-driver.c>
