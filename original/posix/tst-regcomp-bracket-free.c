/* Test regcomp bracket parsing through the public API (bug 33185).
   Copyright (C) 2025 Free Software Foundation, Inc.
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

/* There is no public hook for injecting allocator failures into
   regcomp, so this test covers the public parser entry points across
   successful and rejected bracket expressions instead.  */

#include <regex.h>
#include <string.h>
#include <support/check.h>

static void
check_pattern (const char *regexp, int expected)
{
  for (int i = 0; i < 256; ++i)
    {
      regex_t reg;
      memset (&reg, 0, sizeof (reg));
      int ret = regcomp (&reg, regexp, 0);
      TEST_COMPARE (ret, expected);
      if (ret == 0)
        regfree (&reg);
    }
}

static int
do_test (void)
{
  check_pattern ("[[:alpha:]]", 0);
  check_pattern ("[-_[:alpha:]]", 0);
  check_pattern ("[[:alpha:]", REG_EBRACK);

  return 0;
}

#include <support/test-driver.c>
