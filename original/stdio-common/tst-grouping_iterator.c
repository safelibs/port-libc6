/* Test grouping through public formatting APIs.
   Copyright (C) 2022-2024 Free Software Foundation, Inc.
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
#include <stdio.h>
#include <string.h>

#include <support/check.h>
#include <support/support.h>

#define ARABIC_THOUSANDS "\xd9\xac"
#define NNBSP "\xe2\x80\xaf"

static void
check (const char *locale_name, long long value, const char *expected)
{
  char actual[64];
  xsetlocale (LC_ALL, locale_name);
  TEST_COMPARE (sprintf (actual, "%'lld", value), strlen (expected));
  TEST_COMPARE_STRING (actual, expected);
}

static int
do_test (void)
{
  check ("C", 1234567890LL, "1234567890");
  check ("en_US.ISO-8859-1", 1234567890LL, "1,234,567,890");
  check ("de_DE.UTF-8", 1234567890LL, "1.234.567.890");
  check ("bn_BD.UTF-8", 1234567890LL, "1,23,45,67,890");
  check ("ps_AF.UTF-8", 1234567890LL,
         "1" ARABIC_THOUSANDS "234" ARABIC_THOUSANDS
         "567" ARABIC_THOUSANDS "890");
  check ("unm_US.UTF-8", 1234567890LL,
         "1" NNBSP "234" NNBSP "56" NNBSP "78" NNBSP "90");

  return 0;
}

#include <support/test-driver.c>
