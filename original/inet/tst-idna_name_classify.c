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

#include <arpa/inet.h>
#include <dlfcn.h>
#include <iconv.h>
#include <langinfo.h>
#include <locale.h>
#include <netdb.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <support/check.h>

#define LIBIDN2_SONAME "libidn2.so.0"

static int
lookup_result (const char *name)
{
  struct addrinfo hints =
    {
      .ai_flags = AI_IDN | AI_NUMERICHOST | AI_NUMERICSERV,
      .ai_socktype = SOCK_STREAM,
    };
  struct addrinfo *ai = NULL;
  int ret = getaddrinfo (name, "80", &hints, &ai);
  if (ret == 0)
    freeaddrinfo (ai);
  return ret;
}

static void
expect_result (const char *name, int expected)
{
  TEST_COMPARE (lookup_result (name), expected);
}

static bool
have_working_libidn2 (void)
{
  void *handle = dlopen (LIBIDN2_SONAME, RTLD_LAZY);
  if (handle == NULL)
    return false;

  const char *(*check_version) (const char *)
    = (const char *(*)(const char *)) dlsym (handle, "idn2_check_version");
  bool ok = check_version != NULL && check_version ("2.0.5") != NULL;
  dlclose (handle);
  return ok;
}

static bool
current_locale_supports_idn (void)
{
  if (!have_working_libidn2 ())
    return false;

  iconv_t cd = iconv_open ("UTF-8", nl_langinfo (CODESET));
  if (cd == (iconv_t) -1)
    return false;

  iconv_close (cd);
  return true;
}

static void
expect_numeric_address (const char *name, int family)
{
  struct addrinfo hints =
    {
      .ai_family = family,
      .ai_flags = AI_IDN | AI_NUMERICHOST | AI_NUMERICSERV,
      .ai_socktype = SOCK_STREAM,
    };
  struct addrinfo *ai = NULL;
  TEST_COMPARE (getaddrinfo (name, "80", &hints, &ai), 0);
  TEST_VERIFY_EXIT (ai != NULL);
  TEST_COMPARE (ai->ai_family, family);
  TEST_COMPARE (ai->ai_socktype, SOCK_STREAM);

  if (family == AF_INET)
    {
      struct sockaddr_in *sin = (struct sockaddr_in *) ai->ai_addr;
      struct in_addr expected;
      TEST_VERIFY_EXIT (ai->ai_addrlen >= sizeof (*sin));
      TEST_COMPARE (inet_pton (family, name, &expected), 1);
      TEST_COMPARE (sin->sin_port, htons (80));
      TEST_COMPARE (sin->sin_addr.s_addr, expected.s_addr);
    }
  else
    {
      struct sockaddr_in6 *sin6 = (struct sockaddr_in6 *) ai->ai_addr;
      struct in6_addr expected;
      TEST_VERIFY_EXIT (family == AF_INET6);
      TEST_VERIFY_EXIT (ai->ai_addrlen >= sizeof (*sin6));
      TEST_COMPARE (inet_pton (family, name, &expected), 1);
      TEST_COMPARE (sin6->sin6_port, htons (80));
      TEST_COMPARE (memcmp (&sin6->sin6_addr, &expected, sizeof (expected)),
                    0);
    }

  freeaddrinfo (ai);
}

static void
locale_insensitive_tests (void)
{
  expect_numeric_address ("127.0.0.1", AF_INET);
  expect_numeric_address ("::1", AF_INET6);
  expect_result ("", EAI_NONAME);
  expect_result ("abc", EAI_NONAME);
  expect_result ("..", EAI_NONAME);
  expect_result ("\001abc\177", EAI_NONAME);
  expect_result ("\\065bc", EAI_NONAME);
}

static int
do_test (void)
{
  puts ("info: C locale tests");
  if (setlocale (LC_CTYPE, "C") == NULL)
    FAIL_EXIT1 ("setlocale for C: %m\n");
  locale_insensitive_tests ();
  expect_result ("abc\200def", EAI_IDN_ENCODE);
  expect_result ("abc\200\\def", EAI_IDN_ENCODE);
  expect_result ("abc\377def", EAI_IDN_ENCODE);

  puts ("info: en_US.ISO-8859-1 locale tests");
  if (setlocale (LC_CTYPE, "en_US.ISO-8859-1") == NULL)
    FAIL_EXIT1 ("setlocale for en_US.ISO-8859-1: %m\n");
  int latin1_nonascii = current_locale_supports_idn ()
                        ? EAI_NONAME : EAI_IDN_ENCODE;
  locale_insensitive_tests ();
  expect_result ("abc\377def", latin1_nonascii);
  expect_result ("abc\337def", latin1_nonascii);
  expect_result ("abc\\\337def", EAI_IDN_ENCODE);
  expect_result ("abc\337\\def", EAI_IDN_ENCODE);

  puts ("info: en_US.UTF-8 locale tests");
  if (setlocale (LC_CTYPE, "en_US.UTF-8") == NULL)
    FAIL_EXIT1 ("setlocale for en_US.UTF-8: %m\n");
  int utf8_nonascii = current_locale_supports_idn ()
                      ? EAI_NONAME : EAI_IDN_ENCODE;
  locale_insensitive_tests ();
  expect_result ("abc\xc3\x9f""def", utf8_nonascii);
  expect_result ("abc\\\xc3\x9f""def", EAI_IDN_ENCODE);
  expect_result ("abc\xc3\x9f\\def", EAI_IDN_ENCODE);
  expect_result ("abc\200def", EAI_IDN_ENCODE);
  expect_result ("abc\xc3""def", EAI_IDN_ENCODE);
  expect_result ("abc\xc3", EAI_IDN_ENCODE);

  return 0;
}

#include <support/test-driver.c>
