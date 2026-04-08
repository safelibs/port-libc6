/* Test the public legacy IPv4 text-to-address function inet_aton.
   Copyright (C) 2019-2024 Free Software Foundation, Inc.
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

#include <support/check.h>

static int
do_test (void)
{
  struct in_addr addr = { };

  TEST_COMPARE (inet_aton ("192.0.2.1", &addr), 1);
  TEST_COMPARE (ntohl (addr.s_addr), 0xC0000201);

  TEST_COMPARE (inet_aton ("192.000.002.010", &addr), 1);
  TEST_COMPARE (ntohl (addr.s_addr), 0xC0000208);
  TEST_COMPARE (inet_aton ("0xC0000234", &addr), 1);
  TEST_COMPARE (ntohl (addr.s_addr), 0xC0000234);

  TEST_COMPARE (inet_aton ("192.0.2.256", &addr), 0);
  TEST_COMPARE (inet_aton ("192.0.2.1.5", &addr), 0);
  TEST_COMPARE (inet_aton ("not-an-address", &addr), 0);

  return 0;
}

#include <support/test-driver.c>
