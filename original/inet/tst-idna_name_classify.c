/* Test public multibyte decoding used by IDNA processing.
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

#include <errno.h>
#include <locale.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <support/check.h>
#include <wchar.h>

enum public_name_classification
{
  public_name_ascii,
  public_name_nonascii,
  public_name_nonascii_backslash,
  public_name_encoding_error,
};

static enum public_name_classification
classify_name (const char *name)
{
  mbstate_t mbs = { 0 };
  const char *p = name;
  size_t remaining = strlen (name) + 1;
  bool nonascii = false;
  bool backslash = false;

  while (true)
    {
      wchar_t wc;
      errno = 0;
      size_t result = mbrtowc (&wc, p, remaining, &mbs);
      if (result == 0)
        break;
      if (result == (size_t) -1 || result == (size_t) -2)
        return public_name_encoding_error;

      p += result;
      remaining -= result;
      if (wc == L'\\')
        backslash = true;
      else if (wc > 127)
        nonascii = true;
    }

  if (!nonascii)
    return public_name_ascii;
  return backslash ? public_name_nonascii_backslash : public_name_nonascii;
}

static void
locale_insensitive_tests (void)
{
  TEST_COMPARE (classify_name (""), public_name_ascii);
  TEST_COMPARE (classify_name ("abc"), public_name_ascii);
  TEST_COMPARE (classify_name (".."), public_name_ascii);
  TEST_COMPARE (classify_name ("\001abc\177"), public_name_ascii);
  TEST_COMPARE (classify_name ("\\065bc"), public_name_ascii);
}

static int
do_test (void)
{
  puts ("info: C locale tests");
  locale_insensitive_tests ();
  TEST_COMPARE (classify_name ("abc\200def"), public_name_encoding_error);
  TEST_COMPARE (classify_name ("abc\200\\def"), public_name_encoding_error);
  TEST_COMPARE (classify_name ("abc\377def"), public_name_encoding_error);

  puts ("info: en_US.ISO-8859-1 locale tests");
  if (setlocale (LC_CTYPE, "en_US.ISO-8859-1") == NULL)
    FAIL_EXIT1 ("setlocale for en_US.ISO-8859-1: %m");
  locale_insensitive_tests ();
  TEST_COMPARE (classify_name ("abc\200def"), public_name_nonascii);
  TEST_COMPARE (classify_name ("abc\377def"), public_name_nonascii);
  TEST_COMPARE (classify_name ("abc\\\200def"),
                public_name_nonascii_backslash);
  TEST_COMPARE (classify_name ("abc\200\\def"),
                public_name_nonascii_backslash);

  puts ("info: en_US.UTF-8 locale tests");
  if (setlocale (LC_CTYPE, "en_US.UTF-8") == NULL)
    FAIL_EXIT1 ("setlocale for en_US.UTF-8: %m");
  locale_insensitive_tests ();
  TEST_COMPARE (classify_name ("abc\xc3\x9f""def"), public_name_nonascii);
  TEST_COMPARE (classify_name ("abc\\\xc3\x9f""def"),
                public_name_nonascii_backslash);
  TEST_COMPARE (classify_name ("abc\xc3\x9f\\def"),
                public_name_nonascii_backslash);
  TEST_COMPARE (classify_name ("abc\200def"), public_name_encoding_error);
  TEST_COMPARE (classify_name ("abc\xc3""def"), public_name_encoding_error);
  TEST_COMPARE (classify_name ("abc\xc3"), public_name_encoding_error);

  return 0;
}

#include <support/test-driver.c>
