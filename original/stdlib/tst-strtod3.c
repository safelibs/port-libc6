#include <locale.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <support/check.h>

static const struct
{
  const char *in;
  const char *out;
  double expected;
} tests[] =
  {
    { "000,,,e1", "", 0.0 },
    { "000e1", "", 0.0 },
    { "000,1e1", "", 0.0 },
    { "1,234.5xyz", "xyz", 1234.5 }
  };
#define NTESTS (sizeof (tests) / sizeof (tests[0]))

static void
check_one (int index, const char *input, const char *expected_rest,
           double expected)
{
  double actual = 0.0;
  int nread = -1;
  int ret = sscanf (input, "%'lf%n", &actual, &nread);

  TEST_COMPARE (ret, 1);
  TEST_COMPARE_STRING (input + nread, expected_rest);
  if (actual != expected)
    FAIL_EXIT1 ("%d: got %g, expected %g", index, actual, expected);
}

static int
do_test (void)
{
  if (setlocale (LC_ALL, "en_US.ISO-8859-1") == NULL)
    {
      puts ("could not set locale");
      return 1;
    }

  for (int i = 0; i < NTESTS; ++i)
    check_one (i, tests[i].in, tests[i].out, tests[i].expected);

  return 0;
}

#include <support/test-driver.c>
