/* Tests for public timeout deadline handling in SunRPC UDP calls.
   Copyright (C) 2017-2024 Free Software Foundation, Inc.
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

#include <limits.h>
#include <dlfcn.h>
#include <netinet/in.h>
#include <rpc/clnt.h>
#include <rpc/svc.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <support/check.h>
#include <support/test-driver.h>
#include <support/xdlfcn.h>
#include <support/xunistd.h>
#include <time.h>
#include <unistd.h>

static pid_t server_pid;
static CLIENT *(*clntudp_create_func) (struct sockaddr_in *, u_long, u_long,
                                       struct timeval, int *);
static SVCXPRT *(*svcudp_create_func) (int);
static bool_t (*svc_register_func) (SVCXPRT *, rpcprog_t, rpcvers_t,
                                    void (*)(struct svc_req *, SVCXPRT *),
                                    int);
static bool_t (*svc_sendreply_func) (SVCXPRT *, xdrproc_t, void *);
static void (*svc_run_func) (void);
static bool_t (*xdr_uint32_t_func) (XDR *, uint32_t *);
static xdrproc_t xdr_void_func;

enum
  {
    PROGNUM = 15737,
    VERSNUM = 13699,
    PROC_DELAY = 1,
    PROC_RESET_SEQ,
    PROC_EXIT,
    EXIT_MARKER = 56,
  };

struct deadline_request
{
  uint32_t delay_us;
  uint32_t wait_for_seq;
};

static bool_t
xdr_deadline_request (XDR *xdrs, void *data, ...)
{
  struct deadline_request *request = data;
  return xdr_uint32_t_func (xdrs, &request->delay_us)
    && xdr_uint32_t_func (xdrs, &request->wait_for_seq);
}

struct deadline_response
{
  uint32_t seq;
};

static bool_t
xdr_deadline_response (XDR *xdrs, void *data, ...)
{
  struct deadline_response *response = data;
  return xdr_uint32_t_func (xdrs, &response->seq);
}

static void
resolve_rpc_functions (void)
{
  if (clntudp_create_func != NULL)
    return;

  clntudp_create_func =
    (CLIENT *(*)(struct sockaddr_in *, u_long, u_long, struct timeval, int *))
    xdlvsym (RTLD_DEFAULT, "clntudp_create", "GLIBC_2.2.5");
  svcudp_create_func =
    (SVCXPRT *(*)(int)) xdlvsym (RTLD_DEFAULT, "svcudp_create",
                                 "GLIBC_2.2.5");
  svc_register_func =
    (bool_t (*)(SVCXPRT *, rpcprog_t, rpcvers_t,
                void (*)(struct svc_req *, SVCXPRT *), int))
    xdlvsym (RTLD_DEFAULT, "svc_register", "GLIBC_2.2.5");
  svc_sendreply_func =
    (bool_t (*)(SVCXPRT *, xdrproc_t, void *))
    xdlvsym (RTLD_DEFAULT, "svc_sendreply", "GLIBC_2.2.5");
  svc_run_func = (void (*)(void)) xdlvsym (RTLD_DEFAULT, "svc_run",
                                           "GLIBC_2.2.5");
  xdr_uint32_t_func =
    (bool_t (*)(XDR *, uint32_t *))
    xdlvsym (RTLD_DEFAULT, "xdr_uint32_t", "GLIBC_2.2.5");
  xdr_void_func = (xdrproc_t) xdlvsym (RTLD_DEFAULT, "xdr_void",
                                       "GLIBC_2.2.5");
}

static void
server_dispatch (struct svc_req *request, SVCXPRT *transport)
{
  static uint32_t seq;

  switch (request->rq_proc)
    {
    case PROC_DELAY:
      {
        struct deadline_request query = { 0 };
        TEST_VERIFY_EXIT (svc_getargs (transport, xdr_deadline_request,
                                       (void *) &query));

        ++seq;
        if (test_verbose)
          printf ("info: server seq=%u delay_us=%u wait_for_seq=%u\n",
                  seq, query.delay_us, query.wait_for_seq);

        if (seq < query.wait_for_seq)
          break;

        if (query.delay_us > 0)
          usleep (query.delay_us);

        struct deadline_response response = { .seq = seq };
        TEST_VERIFY_EXIT (svc_sendreply_func (transport, xdr_deadline_response,
                                              (void *) &response));
      }
      break;

    case PROC_RESET_SEQ:
      seq = 0;
      TEST_VERIFY_EXIT
        (svc_sendreply_func (transport, xdr_void_func, NULL));
      break;

    case PROC_EXIT:
      TEST_VERIFY_EXIT
        (svc_sendreply_func (transport, xdr_void_func, NULL));
      _exit (EXIT_MARKER);

    default:
      FAIL_EXIT1 ("unexpected procedure number: %lu", request->rq_proc);
    }
}

static void
kill_server (void)
{
  if (server_pid > 0)
    kill (server_pid, SIGTERM);
}

static double
get_ticks (void)
{
  struct timespec ts;
  TEST_COMPARE (clock_gettime (CLOCK_MONOTONIC, &ts), 0);
  return ts.tv_sec + ts.tv_nsec * 1e-9;
}

static time_t
time_t_max (void)
{
  _Static_assert (0 > (time_t) -1, "time_t is signed");
  uintmax_t current = 1;
  while (true)
    {
      uintmax_t next = current * 2;
      TEST_VERIFY_EXIT (next > current);
      ++next;
      if ((time_t) next < 0 || next != (uintmax_t) (time_t) next)
        return current;
      current = next;
    }
}

static CLIENT *
make_client (int port, struct timeval retry_timeout)
{
  struct sockaddr_in sin =
    {
      .sin_family = AF_INET,
      .sin_addr.s_addr = htonl (INADDR_LOOPBACK),
      .sin_port = htons (port),
    };
  int sock = RPC_ANYSOCK;
  CLIENT *client = clntudp_create_func (&sin, PROGNUM, VERSNUM,
                                        retry_timeout, &sock);
  TEST_VERIFY_EXIT (client != NULL);
  return client;
}

static void
flush_calls (CLIENT *client)
{
  struct timeval retry_timeout = { 0, 200 * 1000 };
  TEST_COMPARE (clnt_control (client, CLSET_RETRY_TIMEOUT,
                              (char *) &retry_timeout),
                TRUE);
  struct timeval timeout = { 5, 0 };
  TEST_COMPARE (clnt_call (client, PROC_RESET_SEQ,
                           xdr_void_func, NULL,
                           xdr_void_func, NULL,
                           timeout),
                RPC_SUCCESS);
}

static double
call_timeout_elapsed (CLIENT *client, struct deadline_request request,
                      struct timeval timeout)
{
  struct deadline_response response;
  double before = get_ticks ();
  TEST_COMPARE (clnt_call (client, PROC_DELAY,
                           xdr_deadline_request, (void *) &request,
                           xdr_deadline_response, (void *) &response,
                           timeout),
                RPC_TIMEDOUT);
  return get_ticks () - before;
}

static struct deadline_response
call_success (CLIENT *client, struct deadline_request request,
              struct timeval timeout, double *elapsed)
{
  struct deadline_response response = { 0 };
  double before = get_ticks ();
  TEST_COMPARE (clnt_call (client, PROC_DELAY,
                           xdr_deadline_request, (void *) &request,
                           xdr_deadline_response, (void *) &response,
                           timeout),
                RPC_SUCCESS);
  *elapsed = get_ticks () - before;
  return response;
}

static void
test_short_timeout_rounding (CLIENT *client)
{
  struct timeval retry_timeout = { 0, 0 };
  struct timeval timeout = { 0, 5 * 1000 };
  TEST_COMPARE (clnt_control (client, CLSET_RETRY_TIMEOUT,
                              (char *) &retry_timeout),
                TRUE);

  double elapsed = call_timeout_elapsed
    (client, (struct deadline_request) { .delay_us = 200,
                                         .wait_for_seq = 1 },
     timeout);
  if (test_verbose)
    printf ("info: zero retry timeout took %.6f seconds\n", elapsed);
  TEST_VERIFY (elapsed < 0.25);
  flush_calls (client);

  retry_timeout = (struct timeval) { 0, 1 };
  TEST_COMPARE (clnt_control (client, CLSET_RETRY_TIMEOUT,
                              (char *) &retry_timeout),
                TRUE);
  struct deadline_response response = call_success
    (client, (struct deadline_request) { .delay_us = 200,
                                         .wait_for_seq = 1 },
     timeout, &elapsed);
  if (test_verbose)
    printf ("info: 1 usec retry timeout completed after %.6f seconds\n",
            elapsed);
  TEST_COMPARE (response.seq, 1);
  TEST_VERIFY (elapsed < 0.25);
  flush_calls (client);
}

static void
test_deadline_ordering (CLIENT *client)
{
  struct timeval retry_timeout = { 0, 200 * 1000 };
  TEST_COMPARE (clnt_control (client, CLSET_RETRY_TIMEOUT,
                              (char *) &retry_timeout),
                TRUE);
  double elapsed = call_timeout_elapsed
    (client, (struct deadline_request) { .wait_for_seq = 2 },
     (struct timeval) { 0, 50 * 1000 });
  if (test_verbose)
    printf ("info: total timeout beat retry timeout after %.6f seconds\n",
            elapsed);
  TEST_VERIFY (elapsed >= 0.03);
  TEST_VERIFY (elapsed < 0.25);
  flush_calls (client);

  retry_timeout = (struct timeval) { 0, 50 * 1000 };
  TEST_COMPARE (clnt_control (client, CLSET_RETRY_TIMEOUT,
                              (char *) &retry_timeout),
                TRUE);
  struct deadline_response response = call_success
    (client, (struct deadline_request) { .wait_for_seq = 2 },
     (struct timeval) { 0, 250 * 1000 }, &elapsed);
  if (test_verbose)
    printf ("info: retry timeout won after %.6f seconds\n", elapsed);
  TEST_COMPARE (response.seq, 2);
  TEST_VERIFY (elapsed >= 0.03);
  TEST_VERIFY (elapsed < 0.25);
  flush_calls (client);
}

static void
test_large_timeouts (CLIENT *client)
{
  struct timeval retry_timeout = { 0, 200 * 1000 };
  TEST_COMPARE (clnt_control (client, CLSET_RETRY_TIMEOUT,
                              (char *) &retry_timeout),
                TRUE);
  double elapsed;
  struct deadline_response response = call_success
    (client, (struct deadline_request) { .delay_us = 50 * 1000,
                                         .wait_for_seq = 1 },
     (struct timeval) { time_t_max (), 1 }, &elapsed);
  if (test_verbose)
    printf ("info: huge total timeout completed after %.6f seconds\n",
            elapsed);
  TEST_COMPARE (response.seq, 1);
  TEST_VERIFY (elapsed >= 0.04);
  TEST_VERIFY (elapsed < 0.30);
  flush_calls (client);

  retry_timeout = (struct timeval) { time_t_max (), 1 };
  TEST_COMPARE (clnt_control (client, CLSET_RETRY_TIMEOUT,
                              (char *) &retry_timeout),
                TRUE);
  response = call_success
    (client, (struct deadline_request) { .delay_us = 50 * 1000,
                                         .wait_for_seq = 1 },
     (struct timeval) { 1, 0 }, &elapsed);
  if (test_verbose)
    printf ("info: huge retry timeout completed after %.6f seconds\n",
            elapsed);
  TEST_COMPARE (response.seq, 1);
  TEST_VERIFY (elapsed >= 0.04);
  TEST_VERIFY (elapsed < 0.30);
  flush_calls (client);
}

static int
do_test (void)
{
  resolve_rpc_functions ();

  struct timespec now;
  TEST_COMPARE (clock_gettime (CLOCK_MONOTONIC, &now), 0);
  TEST_VERIFY (now.tv_sec >= 0);
  TEST_VERIFY (now.tv_sec > 0 || now.tv_nsec > 0);

  SVCXPRT *transport = svcudp_create_func (RPC_ANYSOCK);
  TEST_VERIFY_EXIT (transport != NULL);
  TEST_VERIFY (svc_register_func (transport, PROGNUM, VERSNUM,
                                  server_dispatch, 0));

  server_pid = xfork ();
  if (server_pid == 0)
    {
      svc_run_func ();
      FAIL_EXIT1 ("svc_run returned unexpectedly");
    }
  atexit (kill_server);

  CLIENT *client = make_client (transport->xp_port,
                                (struct timeval) { 0, 200 * 1000 });
  test_short_timeout_rounding (client);
  test_deadline_ordering (client);
  test_large_timeouts (client);

  struct timeval exit_timeout = { 5, 0 };
  TEST_COMPARE (clnt_call (client, PROC_EXIT,
                           xdr_void_func, NULL,
                           xdr_void_func, NULL,
                           exit_timeout),
                RPC_SUCCESS);
  clnt_destroy (client);

  int status;
  xwaitpid (server_pid, &status, 0);
  server_pid = 0;
  TEST_VERIFY (WIFEXITED (status) && WEXITSTATUS (status) == EXIT_MARKER);

  SVC_DESTROY (transport);
  return 0;
}

#define TIMEOUT 15
#include <support/test-driver.c>
