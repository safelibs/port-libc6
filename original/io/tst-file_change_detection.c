/* Test file-change handling through public resolver APIs.
   Copyright (C) 2020-2024 Free Software Foundation, Inc.
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
#include <resolv.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <support/check.h>
#include <support/test-driver.h>

struct resolv_conf_state
{
  bool present;
  int error;
  dev_t dev;
  ino_t ino;
  off_t size;
  struct timespec mtime;
  struct timespec ctime;
};

struct resolver_snapshot
{
  int retrans;
  int retry;
  unsigned long options;
  int nscount;
  struct sockaddr_in nsaddr_list[MAXNS];
  char defdname[sizeof (((struct __res_state *) 0)->defdname)];
  unsigned ndots;
  unsigned nsort;
  struct
  {
    struct in_addr addr;
    uint32_t mask;
  } sort_list[MAXRESOLVSORT];
  char dnsrch[MAXDNSRCH][256];
};

static void
capture_resolv_conf_state (struct resolv_conf_state *state)
{
  memset (state, 0, sizeof (*state));

  struct stat st;
  if (stat (_PATH_RESCONF, &st) == 0)
    {
      state->present = true;
      state->dev = st.st_dev;
      state->ino = st.st_ino;
      state->size = st.st_size;
      state->mtime = st.st_mtim;
      state->ctime = st.st_ctim;
      return;
    }

  switch (errno)
    {
    case EACCES:
    case EISDIR:
    case ELOOP:
    case ENOENT:
    case ENOTDIR:
    case EPERM:
      state->error = errno;
      return;
    default:
      FAIL_EXIT1 ("stat (\"%s\"): %m", _PATH_RESCONF);
    }
}

static void
capture_snapshot (struct resolver_snapshot *snapshot,
                  const struct __res_state *state)
{
  memset (snapshot, 0, sizeof (*snapshot));
  snapshot->retrans = state->retrans;
  snapshot->retry = state->retry;
  snapshot->options = state->options;
  snapshot->nscount = state->nscount;
  memcpy (snapshot->nsaddr_list, state->nsaddr_list,
          sizeof (snapshot->nsaddr_list));
  memcpy (snapshot->defdname, state->defdname, sizeof (snapshot->defdname));
  snapshot->ndots = state->ndots;
  snapshot->nsort = state->nsort;
  memcpy (snapshot->sort_list, state->sort_list, sizeof (snapshot->sort_list));
  for (int i = 0; i < MAXDNSRCH && state->dnsrch[i] != NULL; ++i)
    {
      size_t length = strlen (state->dnsrch[i]);
      TEST_VERIFY_EXIT (length < sizeof (snapshot->dnsrch[i]));
      memcpy (snapshot->dnsrch[i], state->dnsrch[i], length + 1);
    }
}

static bool
load_snapshot_with_stable_file (struct resolv_conf_state *state,
                                struct resolver_snapshot *snapshot)
{
  struct resolv_conf_state after;
  capture_resolv_conf_state (state);

  struct __res_state resolver = { 0 };
  TEST_COMPARE (res_ninit (&resolver), 0);
  capture_snapshot (snapshot, &resolver);
  res_nclose (&resolver);

  capture_resolv_conf_state (&after);
  return memcmp (state, &after, sizeof (*state)) == 0;
}

static int
do_test (void)
{
  unsetenv ("LOCALDOMAIN");
  unsetenv ("RES_OPTIONS");

  struct resolv_conf_state baseline_state;
  struct resolver_snapshot baseline_snapshot;
  bool have_baseline = false;
  int stable_observations = 0;

  /* Repeated public resolver initialization should keep producing the
     same externally visible configuration while /etc/resolv.conf does
     not change.  This exercises the unchanged-file fast path without
     using the internal helper entry points directly.  */
  for (int attempts = 0; attempts < 200 && stable_observations < 16; ++attempts)
    {
      struct resolv_conf_state state;
      struct resolver_snapshot snapshot;
      if (!load_snapshot_with_stable_file (&state, &snapshot))
        continue;

      if (!have_baseline
          || memcmp (&baseline_state, &state, sizeof (state)) != 0)
        {
          baseline_state = state;
          baseline_snapshot = snapshot;
          have_baseline = true;
          stable_observations = 1;
          continue;
        }

      TEST_COMPARE_BLOB (&baseline_snapshot, sizeof (baseline_snapshot),
                         &snapshot, sizeof (snapshot));
      ++stable_observations;
    }

  TEST_VERIFY (have_baseline);
  TEST_VERIFY (stable_observations >= 16);
  return 0;
}

#define TIMEOUT 10
#include <support/test-driver.c>
