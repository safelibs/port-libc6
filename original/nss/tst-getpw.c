/* Copyright (C) 1999-2024 Free Software Foundation, Inc.
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

#include <stdio.h>
#include <pwd.h>
#include <errno.h>
#include <stdbool.h>

/* We want to test getpw by calling it with a uid that does
   exist and one that doesn't exist. We track if we've met those
   conditions and exit. We also track if we've failed due to lack
   of memory. That constitutes all of the standard failure cases.  */
bool seen_hit;
bool seen_miss;
bool seen_oom;
bool have_highest_uid;
uid_t highest_uid;

/* How many errors we've had while running the test.  */
int errors;

static void
check (uid_t uid)
{
  int ret;
  char buf[1024];

  ret = getpw (uid, buf);

  /* Successfully read a password line.  */
  if (ret == 0 && !seen_hit)
    {
      printf ("PASS: Read a password line given a uid.\n");
      seen_hit = true;
    }
  if (ret == 0 && (!have_highest_uid || uid > highest_uid))
    {
      highest_uid = uid;
      have_highest_uid = true;
    }

  /* Failed to read a password line. Why?  */
  if (ret == -1)
    {
      /* No entry?  NSS backends differ here; some leave errno at 0
         while others report ENOENT for a missing uid.  */
      if (errno == 0 || errno == ENOENT)
        {
          if (!seen_miss)
            {
              printf ("PASS: Found an invalid uid.\n");
              seen_miss = true;
            }
          return;
        }

      /* Out of memory?  */
      if (errno == ENOMEM && !seen_oom)
        {
          printf ("FAIL: Failed with ENOMEM.\n");
          seen_oom = true;
          errors++;
        }

      /* We don't expect any other values for errno.  */
      if (errno != ENOMEM && errno != 0)
        errors++;
    }
}

static int
do_test (void)
{
  int ret;
  uid_t uid;

  /* Should return -1 and set errnot to EINVAL.  */
  ret = getpw (0, NULL);
  if (ret == -1 && errno == EINVAL)
    {
      printf ("PASS: NULL buffer returns -1 and sets errno to EINVAL.\n");
    }
  else
    {
      printf ("FAIL: NULL buffer did not return -1 or set errno to EINVAL.\n");
      errors++;
    }

  /* Look for one matching uid and one non-found uid in the low range first,
     then probe above the highest observed uid if the namespace is dense.  */
  for (uid = 0; uid < ((uid_t) 65535); ++uid)
    {
      check (uid);
      if (seen_miss && seen_hit)
	break;
    }

  if (!seen_miss && have_highest_uid)
    {
      uid_t probe = highest_uid;

      while (!seen_miss)
        {
          uid_t next = probe + 1;

          if (next <= probe)
            next = (uid_t) -1;

          check (next);
          if (seen_miss || next == (uid_t) -1)
            break;

          probe = next < ((uid_t) -1) / 2 ? next * 2 + 1 : (uid_t) -1;
        }
    }

  if (!seen_hit)
    {
      printf ("FAIL: Did not read even one password line given a uid.\n");
      errors++;
    }

  if (!seen_miss)
    {
      printf ("FAIL: Did not find even one invalid uid.\n");
      errors++;
    }

  return errors;
}

#define TEST_FUNCTION do_test ()
#include "../test-skeleton.c"
