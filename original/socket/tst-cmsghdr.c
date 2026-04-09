/* Test ancillary data header creation.
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

#include <sys/socket.h>
#include <support/check.h>

#define PAYLOAD "Hello, World!"

/* Exercise the public CMSG_NXTHDR macro.  This still covers the
   inline helper in bits/socket.h, but avoids depending on libc's
   non-exported fallback entry point.  */

#define CMSG_NXTHDR_IMPL CMSG_NXTHDR
#include "tst-cmsghdr-skeleton.c"
#undef CMSG_NXTHDR_IMPL

static int
do_test (void)
{
  run_test_CMSG_NXTHDR ();
  return 0;
}

#include <support/test-driver.c>
