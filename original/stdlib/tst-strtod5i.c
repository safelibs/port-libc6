/* Tests of locale-aware scanf parsing in a locale using decimal comma.
   Copyright (C) 2007-2024 Free Software Foundation, Inc.
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

#include <locale.h>
#include <support/check.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

#define NNBSP "\xe2\x80\xaf"

static const struct
{
  const char *in;
  int group;
  double expected;
} tests[] =
  {
    { "0", 0, 0.0 },
    { "000", 0, 0.0 },
    { "-0", 0, -0.0 },
    { "-000", 0, -0.0 },
    { "0,", 0, 0.0 },
    { "-0,", 0, -0.0 },
    { "0,0", 0, 0.0 },
    { "-0,0", 0, -0.0 },
    { "0e-10", 0, 0.0 },
    { "-0e-10", 0, -0.0 },
    { "0,e-10", 0, 0.0 },
    { "-0,e-10", 0, -0.0 },
    { "0,0e-10", 0, 0.0 },
    { "-0,0e-10", 0, -0.0 },
    { "0e-1000000", 0, 0.0 },
    { "-0e-1000000", 0, -0.0 },
    { "0,0e-1000000", 0, 0.0 },
    { "-0,0e-1000000", 0, -0.0 },
    { "0", 1, 0.0 },
    { "000", 1, 0.0 },
    { "-0", 1, -0.0 },
    { "-000", 1, -0.0 },
    { "0e-10", 1, 0.0 },
    { "-0e-10", 1, -0.0 },
    { "0e-1000000", 1, 0.0 },
    { "-0e-1000000", 1, -0.0 },
    { "000"NNBSP"000"NNBSP"000", 1, 0.0 },
    { "-000"NNBSP"000"NNBSP"000", 1, -0.0 }
  };
#define NTESTS (sizeof (tests) / sizeof (tests[0]))


static void
check_one (int index, const char *input, int group, double expected)
{
  double actual = 1.0;
  int nread = -1;
  int ret = (group
             ? sscanf (input, "%'lf%n", &actual, &nread)
             : sscanf (input, "%lf%n", &actual, &nread));

  TEST_COMPARE (ret, 1);
  TEST_COMPARE (nread, strlen (input));
  if (actual != expected || signbit (actual) != signbit (expected))
    FAIL_EXIT1 ("%d: got %g, expected %g", index, actual, expected);
}

static int
do_test (void)
{
  if (setlocale (LC_ALL, "cs_CZ.UTF-8") == NULL)
    {
      puts ("could not set locale");
      return 1;
    }

  int status = 0;

  for (int i = 0; i < NTESTS; ++i)
    check_one (i, tests[i].in, tests[i].group, tests[i].expected);

  return status;
}

#include <support/test-driver.c>
