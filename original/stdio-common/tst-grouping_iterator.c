/* Test grouping through public locale and formatting APIs.
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
#include <monetary.h>
#include <stdio.h>
#include <string.h>

#include <support/check.h>
#include <support/support.h>

#define ARABIC_THOUSANDS "\xd9\xac"
#define NNBSP "\xe2\x80\xaf"

static const char grouping_none[] = { 0 };
static const char grouping_3[] = { 3, 0 };
static const char grouping_3_2[] = { 3, 2, 0 };
static const char grouping_2_2_2_3[] = { 2, 2, 2, 3, 0 };

static void
check_grouping (const char *actual, const char *expected, size_t expected_length)
{
  TEST_COMPARE_BLOB (actual, expected_length, expected, expected_length);
}

static struct lconv *
set_test_locale (const char *locale_name)
{
  xsetlocale (LC_ALL, locale_name);
  return localeconv ();
}

static void
check_numeric (const char *locale_name, const char *expected_grouping,
               size_t expected_grouping_length, const char *expected)
{
  char actual[64];
  struct lconv *lc = set_test_locale (locale_name);
  check_grouping (lc->grouping, expected_grouping, expected_grouping_length);
  TEST_COMPARE (sprintf (actual, "%'lld", 12345678LL), strlen (expected));
  TEST_COMPARE_STRING (actual, expected);
}

static void
check_alt_digits (const char *locale_name, const char *expected)
{
  char actual[128];
  set_test_locale (locale_name);
  TEST_COMPARE (sprintf (actual, "%'Id", 12345678), strlen (expected));
  TEST_COMPARE_STRING (actual, expected);
}

static void
check_monetary (const char *locale_name, const char *expected_grouping,
                size_t expected_grouping_length, const char *expected)
{
  char actual[128];
  struct lconv *lc = set_test_locale (locale_name);
  check_grouping (lc->mon_grouping, expected_grouping, expected_grouping_length);
  TEST_COMPARE (strfmon (actual, sizeof (actual), "%!n", 12345678.0),
                strlen (expected));
  TEST_COMPARE_STRING (actual, expected);
}

static int
do_test (void)
{
  check_numeric ("C", grouping_none, sizeof (grouping_none), "12345678");
  check_numeric ("de_DE.UTF-8", grouping_3, sizeof (grouping_3),
                 "12.345.678");
  check_numeric ("hi_IN.UTF-8", grouping_3, sizeof (grouping_3),
                 "12,345,678");
  check_numeric ("bn_BD.UTF-8", grouping_3_2, sizeof (grouping_3_2),
                 "1,23,45,678");
  check_numeric ("ps_AF.UTF-8", grouping_3, sizeof (grouping_3),
                 "12" ARABIC_THOUSANDS "345" ARABIC_THOUSANDS "678");
  check_numeric ("rw_RW.UTF-8", grouping_none, sizeof (grouping_none),
                 "12345678");
  check_numeric ("unm_US.UTF-8", grouping_2_2_2_3,
                 sizeof (grouping_2_2_2_3),
                 "12" NNBSP "34" NNBSP "56" NNBSP "78");

  check_alt_digits ("hi_IN.UTF-8",
                    "\xe0\xa5\xa7\xe0\xa5\xa8,"
                    "\xe0\xa5\xa9\xe0\xa5\xaa\xe0\xa5\xab,"
                    "\xe0\xa5\xac\xe0\xa5\xad\xe0\xa5\xae");
  check_alt_digits ("bn_BD.UTF-8",
                    "\xe0\xa7\xa7,"
                    "\xe0\xa7\xa8\xe0\xa7\xa9,"
                    "\xe0\xa7\xaa\xe0\xa7\xab,"
                    "\xe0\xa7\xac\xe0\xa7\xad\xe0\xa7\xae");
  check_alt_digits ("ps_AF.UTF-8",
                    "\xd9\xa1\xd9\xa2"
                    ARABIC_THOUSANDS
                    "\xd9\xa3\xdb\xb4\xd9\xa5"
                    ARABIC_THOUSANDS
                    "\xd9\xa6\xd9\xa7\xd9\xa8");

  check_monetary ("C", grouping_none, sizeof (grouping_none), "12345678.00");
  check_monetary ("de_DE.UTF-8", grouping_3, sizeof (grouping_3),
                  "12.345.678,00");
  check_monetary ("hi_IN.UTF-8", grouping_3_2, sizeof (grouping_3_2),
                  "1,23,45,678.00");
  check_monetary ("rw_RW.UTF-8", grouping_3, sizeof (grouping_3),
                  "12.345.678,00");
  check_monetary ("unm_US.UTF-8", grouping_3, sizeof (grouping_3),
                  "12" NNBSP "345" NNBSP "678.00");

  return 0;
}

#include <support/test-driver.c>
