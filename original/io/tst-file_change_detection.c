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

#include <arpa/inet.h>
#include <errno.h>
#include <link.h>
#include <resolv.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <support/check.h>
#include <support/support.h>
#include <support/temp_file.h>
#include <support/test-driver.h>
#include <support/xunistd.h>
#include <unistd.h>

#undef p_type

static const char resolv_conf_one[] =
  "options timeout:3 attempts:2 ndots:4 rotate\n"
  "search corp.example.com example.com\n"
  "nameserver 192.0.2.1\n";

static const char resolv_conf_two[] =
  "options timeout:5 attempts:4 ndots:2 single-request\n"
  "search example.net example.org\n"
  "nameserver 192.0.2.2\n"
  "nameserver 192.0.2.3\n";

static const char resolv_conf_path[] = _PATH_RESCONF;

struct path_patch_state
{
  const char *replacement;
  size_t replacement_length;
  long page_size;
  size_t patches;
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
patch_bytes (uintptr_t address, size_t length, int prot,
             const struct path_patch_state *state)
{
  uintptr_t page_mask = state->page_size - 1;
  uintptr_t page_start = address & ~page_mask;
  uintptr_t page_end = (address + length + page_mask) & ~page_mask;
  size_t page_length = page_end - page_start;

  TEST_COMPARE (mprotect ((void *) page_start, page_length, prot | PROT_WRITE),
                0);
  memset ((void *) address, 0, length);
  memcpy ((void *) address, state->replacement, state->replacement_length);
  TEST_COMPARE (mprotect ((void *) page_start, page_length, prot), 0);
}

static bool
is_libc_object (const char *name)
{
  if (name == NULL || name[0] == '\0')
    return false;

  const char *base = strrchr (name, '/');
  if (base != NULL)
    ++base;
  else
    base = name;

  return strstr (base, "libc.so") != NULL;
}

static int
patch_resolv_conf_path_cb (struct dl_phdr_info *info, size_t size,
                           void *closure)
{
  struct path_patch_state *state = closure;
  if (!is_libc_object (info->dlpi_name))
    return 0;

  for (ElfW(Half) i = 0; i < info->dlpi_phnum; ++i)
    {
      const ElfW(Phdr) *phdr = &info->dlpi_phdr[i];
      if (phdr->p_type != PT_LOAD || !(phdr->p_flags & PF_R))
        continue;

      unsigned char *segment
        = (unsigned char *) (info->dlpi_addr + phdr->p_vaddr);
      size_t segment_length = phdr->p_memsz;
      if (segment_length < sizeof (resolv_conf_path))
        continue;

      int prot = 0;
      if (phdr->p_flags & PF_R)
        prot |= PROT_READ;
      if (phdr->p_flags & PF_W)
        prot |= PROT_WRITE;
      if (phdr->p_flags & PF_X)
        prot |= PROT_EXEC;

      for (size_t offset = 0;
           offset + sizeof (resolv_conf_path) <= segment_length;
           ++offset)
        if (memcmp (segment + offset, resolv_conf_path,
                    sizeof (resolv_conf_path)) == 0)
          {
            patch_bytes ((uintptr_t) (segment + offset),
                         sizeof (resolv_conf_path), prot, state);
            ++state->patches;
            offset += sizeof (resolv_conf_path) - 1;
          }
    }

  return 0;
}

static void
redirect_resolv_conf_path (const char *replacement)
{
  struct path_patch_state state =
    {
      .replacement = replacement,
      .replacement_length = strlen (replacement) + 1,
      .page_size = xsysconf (_SC_PAGESIZE),
    };
  TEST_VERIFY (state.page_size > 0);
  TEST_VERIFY_EXIT (state.replacement_length <= sizeof (resolv_conf_path));
  dl_iterate_phdr (patch_resolv_conf_path_cb, &state);
  TEST_VERIFY_EXIT (state.patches > 0);
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

static void
load_snapshot (struct resolver_snapshot *snapshot)
{
  struct __res_state resolver = { 0 };
  TEST_COMPARE (res_ninit (&resolver), 0);
  capture_snapshot (snapshot, &resolver);
  res_nclose (&resolver);
}

static void
check_same_snapshot (const char *left_name,
                     const struct resolver_snapshot *left,
                     const char *right_name,
                     const struct resolver_snapshot *right)
{
  if (test_verbose > 0)
    printf ("info: comparing %s and %s\n", left_name, right_name);
  TEST_COMPARE (left->retrans, right->retrans);
  TEST_COMPARE (left->retry, right->retry);
  TEST_COMPARE (left->options, right->options);
  TEST_COMPARE (left->nscount, right->nscount);
  TEST_COMPARE (left->ndots, right->ndots);
  TEST_COMPARE (left->nsort, right->nsort);
  TEST_COMPARE_STRING (left->defdname, right->defdname);

  for (int i = 0; i < left->nscount; ++i)
    {
      TEST_COMPARE (left->nsaddr_list[i].sin_family,
                    right->nsaddr_list[i].sin_family);
      TEST_COMPARE (left->nsaddr_list[i].sin_port,
                    right->nsaddr_list[i].sin_port);
      TEST_COMPARE (left->nsaddr_list[i].sin_addr.s_addr,
                    right->nsaddr_list[i].sin_addr.s_addr);
    }

  for (unsigned int i = 0; i < left->nsort; ++i)
    {
      TEST_COMPARE (left->sort_list[i].addr.s_addr,
                    right->sort_list[i].addr.s_addr);
      TEST_COMPARE (left->sort_list[i].mask, right->sort_list[i].mask);
    }

  for (int i = 0; i < MAXDNSRCH; ++i)
    TEST_COMPARE_STRING (left->dnsrch[i], right->dnsrch[i]);
}

static void
check_different_snapshot (const char *left_name,
                          const struct resolver_snapshot *left,
                          const char *right_name,
                          const struct resolver_snapshot *right)
{
  if (test_verbose > 0)
    printf ("info: ensuring %s and %s differ\n", left_name, right_name);
  bool same = left->retrans == right->retrans
    && left->retry == right->retry
    && left->options == right->options
    && left->nscount == right->nscount
    && left->ndots == right->ndots
    && left->nsort == right->nsort
    && strcmp (left->defdname, right->defdname) == 0;

  for (int i = 0; same && i < left->nscount; ++i)
    same = left->nsaddr_list[i].sin_family == right->nsaddr_list[i].sin_family
      && left->nsaddr_list[i].sin_port == right->nsaddr_list[i].sin_port
      && left->nsaddr_list[i].sin_addr.s_addr
         == right->nsaddr_list[i].sin_addr.s_addr;

  for (unsigned int i = 0; same && i < left->nsort; ++i)
    same = left->sort_list[i].addr.s_addr == right->sort_list[i].addr.s_addr
      && left->sort_list[i].mask == right->sort_list[i].mask;

  for (int i = 0; same && i < MAXDNSRCH; ++i)
    same = strcmp (left->dnsrch[i], right->dnsrch[i]) == 0;

  TEST_VERIFY (!same);
}

static void
remove_path_if_exists (const char *path)
{
  struct stat st;
  if (lstat (path, &st) != 0)
    {
      if (errno == ENOENT)
        return;
      FAIL_EXIT1 ("lstat (\"%s\"): %m", path);
    }

  if (S_ISDIR (st.st_mode))
    TEST_COMPARE (rmdir (path), 0);
  else
    TEST_COMPARE (unlink (path), 0);
}

static void
check_nameserver (const struct resolver_snapshot *snapshot, int index,
                  const char *address)
{
  struct in_addr expected;
  TEST_COMPARE (inet_pton (AF_INET, address, &expected), 1);
  TEST_COMPARE (snapshot->nsaddr_list[index].sin_family, AF_INET);
  TEST_COMPARE (snapshot->nsaddr_list[index].sin_addr.s_addr,
                expected.s_addr);
  TEST_COMPARE (snapshot->nsaddr_list[index].sin_port, htons (53));
}

static void
check_config_one (const struct resolver_snapshot *snapshot)
{
  TEST_COMPARE (snapshot->retrans, 3);
  TEST_COMPARE (snapshot->retry, 2);
  TEST_COMPARE (snapshot->ndots, 4);
  TEST_COMPARE (snapshot->nscount, 1);
  TEST_VERIFY (snapshot->options & RES_ROTATE);
  TEST_VERIFY (strcmp (snapshot->dnsrch[0], "corp.example.com") == 0);
  TEST_VERIFY (strcmp (snapshot->dnsrch[1], "example.com") == 0);
  TEST_VERIFY (snapshot->dnsrch[2][0] == '\0');
  check_nameserver (snapshot, 0, "192.0.2.1");
}

static void
check_config_two (const struct resolver_snapshot *snapshot)
{
  TEST_COMPARE (snapshot->retrans, 5);
  TEST_COMPARE (snapshot->retry, 4);
  TEST_COMPARE (snapshot->ndots, 2);
  TEST_COMPARE (snapshot->nscount, 2);
  TEST_VERIFY (snapshot->options & RES_SNGLKUP);
  TEST_VERIFY (strcmp (snapshot->dnsrch[0], "example.net") == 0);
  TEST_VERIFY (strcmp (snapshot->dnsrch[1], "example.org") == 0);
  TEST_VERIFY (snapshot->dnsrch[2][0] == '\0');
  check_nameserver (snapshot, 0, "192.0.2.2");
  check_nameserver (snapshot, 1, "192.0.2.3");
}

static int
do_test (void)
{
  char *redirect_path;
  int redirect_fd = create_temp_file ("rc", &redirect_path);
  TEST_VERIFY_EXIT (redirect_fd >= 0);
  xclose (redirect_fd);
  redirect_resolv_conf_path (redirect_path);

  char *tempdir = support_create_temp_directory ("tst-file-change-");
  char *path_dangling = xasprintf ("%s/dangling", tempdir);
  char *path_loop = xasprintf ("%s/loop", tempdir);
  char *path_target_one = xasprintf ("%s/target1", tempdir);
  char *path_target_two = xasprintf ("%s/target2", tempdir);

  unsetenv ("LOCALDOMAIN");
  unsetenv ("RES_OPTIONS");

  support_write_file_string (redirect_path, "");
  struct resolver_snapshot empty;
  load_snapshot (&empty);

  remove_path_if_exists (redirect_path);
  struct resolver_snapshot missing;
  load_snapshot (&missing);
  check_same_snapshot ("empty file", &empty, "missing file", &missing);

  TEST_COMPARE (symlink (path_dangling, redirect_path), 0);
  struct resolver_snapshot dangling;
  load_snapshot (&dangling);
  check_same_snapshot ("empty file", &empty, "dangling symlink", &dangling);
  remove_path_if_exists (redirect_path);

  TEST_COMPARE (symlink ("loop", path_loop), 0);
  TEST_COMPARE (symlink (path_loop, redirect_path), 0);
  struct resolver_snapshot loop;
  load_snapshot (&loop);
  check_same_snapshot ("empty file", &empty, "symbolic link loop", &loop);
  remove_path_if_exists (redirect_path);

  support_write_file_string (redirect_path, "");
  TEST_COMPARE (chmod (redirect_path, 0), 0);
  struct resolver_snapshot unreadable;
  load_snapshot (&unreadable);
  check_same_snapshot ("empty file", &empty, "unreadable file", &unreadable);
  remove_path_if_exists (redirect_path);

  TEST_COMPARE (mkdir (redirect_path, 0777), 0);
  struct resolver_snapshot directory;
  load_snapshot (&directory);
  check_same_snapshot ("empty file", &empty, "directory", &directory);
  remove_path_if_exists (redirect_path);

  support_write_file_string (redirect_path, resolv_conf_one);
  struct resolver_snapshot direct;
  load_snapshot (&direct);
  check_different_snapshot ("empty file", &empty, "configured file", &direct);
  check_config_one (&direct);

  support_write_file_string (path_target_one, resolv_conf_one);
  remove_path_if_exists (redirect_path);
  TEST_COMPARE (symlink (path_target_one, redirect_path), 0);
  struct resolver_snapshot via_symlink;
  load_snapshot (&via_symlink);
  check_same_snapshot ("configured file", &direct,
                       "symlink to configured file", &via_symlink);

  support_write_file_string (path_target_two, resolv_conf_two);
  remove_path_if_exists (redirect_path);
  TEST_COMPARE (symlink (path_target_two, redirect_path), 0);
  struct resolver_snapshot reloaded;
  load_snapshot (&reloaded);
  check_different_snapshot ("configured file", &direct,
                            "reloaded configured file", &reloaded);
  check_config_two (&reloaded);

  remove_path_if_exists (redirect_path);
  support_write_file_string (redirect_path, "");
  struct resolver_snapshot restored_empty;
  load_snapshot (&restored_empty);
  check_same_snapshot ("empty file", &empty,
                       "restored empty file", &restored_empty);

  remove_path_if_exists (path_target_one);
  remove_path_if_exists (path_target_two);
  remove_path_if_exists (path_loop);
  free (path_target_two);
  free (path_target_one);
  free (path_loop);
  free (path_dangling);
  free (tempdir);
  free (redirect_path);
  return 0;
}

#define TIMEOUT 10
#include <support/test-driver.c>
