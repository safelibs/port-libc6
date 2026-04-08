#define _GNU_SOURCE 1
/* Test public IDNA name handling through getaddrinfo.
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

#include <locale.h>
#include <netdb.h>
#include <stdio.h>
#include <support/check.h>

/* Route names through the public IDNA conversion path without relying on
   external name service configuration.  AI_NUMERICHOST ensures that names
   reaching the lookup phase fail with EAI_NONAME instead of going out to
   NSS or DNS.  */
static int
lookup_name (const char *name)
{
  struct addrinfo hints =
    {
      .ai_family = AF_UNSPEC,
      .ai_socktype = 0,
      .ai_protocol = 0,
      .ai_flags = AI_IDN | AI_NUMERICHOST,
    };
  struct addrinfo *ai = NULL;
  int ret = getaddrinfo (name, NULL, &hints, &ai);
  if (ret == 0)
    freeaddrinfo (ai);
  return ret;
}

static void
locale_insensitive_tests (void)
{
  TEST_COMPARE (lookup_name (""), EAI_NONAME);
  TEST_COMPARE (lookup_name ("abc"), EAI_NONAME);
  TEST_COMPARE (lookup_name (".."), EAI_NONAME);
  TEST_COMPARE (lookup_name ("\001abc\177"), EAI_NONAME);
  TEST_COMPARE (lookup_name ("\\065bc"), EAI_NONAME);
  TEST_COMPARE (lookup_name ("127.0.0.1"), 0);
  TEST_COMPARE (lookup_name ("::1"), 0);
}

/* Valid non-ASCII names either pass through IDNA conversion and then fail as
   non-numeric hosts, or they report that IDNA conversion is unavailable.  */
static void
check_convertible_or_encode (const char *name)
{
  int ret = lookup_name (name);
  TEST_VERIFY (ret == EAI_NONAME || ret == EAI_IDN_ENCODE);
}

static int
do_test (void)
{
  puts ("info: C locale tests");
  if (setlocale (LC_CTYPE, "C") == NULL)
    FAIL_EXIT1 ("setlocale for C locale: %m");
  locale_insensitive_tests ();
  TEST_COMPARE (lookup_name ("abc\200def"), EAI_IDN_ENCODE);
  TEST_COMPARE (lookup_name ("abc\200\\def"), EAI_IDN_ENCODE);
  TEST_COMPARE (lookup_name ("abc\377def"), EAI_IDN_ENCODE);

  puts ("info: en_US.ISO-8859-1 locale tests");
  if (setlocale (LC_CTYPE, "en_US.ISO-8859-1") == NULL)
    FAIL_EXIT1 ("setlocale for en_US.ISO-8859-1: %m");
  locale_insensitive_tests ();
  check_convertible_or_encode ("abc\337def");
  TEST_COMPARE (lookup_name ("abc\\\337def"), EAI_IDN_ENCODE);
  TEST_COMPARE (lookup_name ("abc\337\\def"), EAI_IDN_ENCODE);

  puts ("info: en_US.UTF-8 locale tests");
  if (setlocale (LC_CTYPE, "en_US.UTF-8") == NULL)
    FAIL_EXIT1 ("setlocale for en_US.UTF-8: %m");
  locale_insensitive_tests ();
  check_convertible_or_encode ("abc\xc3\x9f""def");
  TEST_COMPARE (lookup_name ("abc\\\xc3\x9f""def"), EAI_IDN_ENCODE);
  TEST_COMPARE (lookup_name ("abc\xc3\x9f\\def"), EAI_IDN_ENCODE);
  TEST_COMPARE (lookup_name ("abc\200def"), EAI_IDN_ENCODE);
  TEST_COMPARE (lookup_name ("abc\xc3""def"), EAI_IDN_ENCODE);
  TEST_COMPARE (lookup_name ("abc\xc3"), EAI_IDN_ENCODE);

  return 0;
}

#include <support/test-driver.c>
