/* Test IDNA name classification.
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
#include <sys/socket.h>
#include <support/check.h>

static void
expect_idn_encode (const char *name)
{
  struct addrinfo hints =
    {
      .ai_flags = AI_IDN | AI_CANONNAME | AI_NUMERICSERV,
      .ai_socktype = SOCK_STREAM,
    };
  struct addrinfo *ai = NULL;
  int ret = getaddrinfo (name, "80", &hints, &ai);
  if (ret == 0)
    freeaddrinfo (ai);
  TEST_COMPARE (ret, EAI_IDN_ENCODE);
}

static void
expect_not_idn_encode (const char *name)
{
  struct addrinfo hints =
    {
      .ai_flags = AI_IDN | AI_CANONNAME | AI_NUMERICSERV,
      .ai_socktype = SOCK_STREAM,
    };
  struct addrinfo *ai = NULL;
  int ret = getaddrinfo (name, "80", &hints, &ai);
  if (ret == 0)
    freeaddrinfo (ai);
  TEST_VERIFY (ret != EAI_IDN_ENCODE);
}

static void
locale_insensitive_tests (void)
{
  expect_not_idn_encode ("localhost");
  expect_not_idn_encode ("example.com");
}

static int
do_test (void)
{
  puts ("info: C locale tests");
  locale_insensitive_tests ();
  expect_idn_encode ("abc\200def");
  expect_idn_encode ("abc\200\\def");
  expect_idn_encode ("abc\377def");

  puts ("info: en_US.ISO-8859-1 locale tests");
  if (setlocale (LC_CTYPE, "en_US.ISO-8859-1") == 0)
    FAIL_EXIT1 ("setlocale for en_US.ISO-8859-1: %m\n");
  locale_insensitive_tests ();
  expect_not_idn_encode ("abc\377def");
  expect_not_idn_encode ("abc\337def");
  expect_idn_encode ("abc\\\337def");
  expect_idn_encode ("abc\337\\def");

  puts ("info: en_US.UTF-8 locale tests");
  if (setlocale (LC_CTYPE, "en_US.UTF-8") == 0)
    FAIL_EXIT1 ("setlocale for en_US.UTF-8: %m\n");
  locale_insensitive_tests ();
  expect_not_idn_encode ("abc\xc3\x9f""def");
  expect_idn_encode ("abc\\\xc3\x9f""def");
  expect_idn_encode ("abc\xc3\x9f\\def");
  expect_idn_encode ("abc\200def");
  expect_idn_encode ("abc\xc3""def");
  expect_idn_encode ("abc\xc3");

  return 0;
}

#include <support/test-driver.c>
