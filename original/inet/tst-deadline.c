/* Test public timeout behavior that depends on libc deadline handling.
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

#include <netinet/in.h>
#include <rpc/clnt.h>
#include <rpc/svc.h>
#include <stdbool.h>
#include <stdint.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <support/check.h>
#include <support/test-driver.h>
#include <support/xunistd.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static pid_t server_pid;

extern bool_t xdr_uint32_t_glibc_2_2_5 (XDR *, uint32_t *) __THROW;
extern bool_t xdr_void_glibc_2_2_5 (void) __THROW;
extern CLIENT *clntudp_create_glibc_2_2_5 (struct sockaddr_in *, u_long,
                                           u_long, struct timeval, int *)
  __THROW;
extern bool_t svc_register_glibc_2_2_5 (SVCXPRT *, rpcprog_t, rpcvers_t,
                                        __dispatch_fn_t, rpcprot_t) __THROW;
extern bool_t svc_sendreply_glibc_2_2_5 (SVCXPRT *, xdrproc_t, void *)
  __THROW;
extern SVCXPRT *svcudp_create_glibc_2_2_5 (int) __THROW;
extern void svc_run_glibc_2_2_5 (void) __THROW;

asm (".symver xdr_uint32_t_glibc_2_2_5, xdr_uint32_t@GLIBC_2.2.5");
asm (".symver xdr_void_glibc_2_2_5, xdr_void@GLIBC_2.2.5");
asm (".symver clntudp_create_glibc_2_2_5, clntudp_create@GLIBC_2.2.5");
asm (".symver svc_register_glibc_2_2_5, svc_register@GLIBC_2.2.5");
asm (".symver svc_sendreply_glibc_2_2_5, svc_sendreply@GLIBC_2.2.5");
asm (".symver svcudp_create_glibc_2_2_5, svcudp_create@GLIBC_2.2.5");
asm (".symver svc_run_glibc_2_2_5, svc_run@GLIBC_2.2.5");

struct test_query
{
  uint32_t a;
  uint32_t b;
  uint32_t timeout_ms;
  uint32_t wait_for_seq;
  uint32_t garbage_packets;
};

static bool_t
xdr_test_query (XDR *xdrs, void *data, ...)
{
  struct test_query *p = data;
  return xdr_uint32_t_glibc_2_2_5 (xdrs, &p->a)
    && xdr_uint32_t_glibc_2_2_5 (xdrs, &p->b)
    && xdr_uint32_t_glibc_2_2_5 (xdrs, &p->timeout_ms)
    && xdr_uint32_t_glibc_2_2_5 (xdrs, &p->wait_for_seq)
    && xdr_uint32_t_glibc_2_2_5 (xdrs, &p->garbage_packets);
}

struct test_response
{
  uint32_t seq;
  uint32_t sum;
};

static bool_t
xdr_test_response (XDR *xdrs, void *data, ...)
{
  struct test_response *p = data;
  return xdr_uint32_t_glibc_2_2_5 (xdrs, &p->seq)
    && xdr_uint32_t_glibc_2_2_5 (xdrs, &p->sum);
}

enum
  {
    PROGNUM = 15717,
    VERSNUM = 13689,
    PROC_ADD = 1,
    PROC_RESET_SEQ,
    PROC_EXIT,
    EXIT_MARKER = 55,
  };

static void
server_dispatch (struct svc_req *request, SVCXPRT *transport)
{
  static uint32_t seq = 0;
  ++seq;

  switch (request->rq_proc)
    {
    case PROC_ADD:
      {
        struct test_query query;
        memset (&query, 0, sizeof (query));
        TEST_VERIFY_EXIT (svc_getargs (transport, xdr_test_query,
                                       (void *) &query));

        if (seq < query.wait_for_seq)
          break;

        if (query.garbage_packets > 0)
          {
            int per_packet_timeout = 0;
            if (query.timeout_ms > 0)
              per_packet_timeout
                = query.timeout_ms * 1000 / query.garbage_packets;

            char buf[20];
            memset (&buf, 0xc0, sizeof (buf));
            for (uint32_t i = 0; i < query.garbage_packets; ++i)
              {
                size_t len = (i * 13 + 1) % (sizeof (buf) + 1);
                TEST_VERIFY (sendto (transport->xp_sock,
                                     buf, len, MSG_NOSIGNAL,
                                     (struct sockaddr *) &transport->xp_raddr,
                                     transport->xp_addrlen) == len);
                if (per_packet_timeout > 0)
                  usleep (per_packet_timeout);
              }
          }
        else if (query.timeout_ms > 0)
          usleep (query.timeout_ms * 1000);

        struct test_response response =
          {
            .seq = seq,
            .sum = query.a + query.b,
          };
        TEST_VERIFY (svc_sendreply_glibc_2_2_5 (transport,
                                                xdr_test_response,
                                                (void *) &response));
      }
      break;

    case PROC_RESET_SEQ:
      seq = 0;
      TEST_VERIFY (svc_sendreply_glibc_2_2_5
                   (transport, (xdrproc_t) xdr_void_glibc_2_2_5, NULL));
      break;

    case PROC_EXIT:
      TEST_VERIFY (svc_sendreply_glibc_2_2_5
                   (transport, (xdrproc_t) xdr_void_glibc_2_2_5, NULL));
      _exit (EXIT_MARKER);

    default:
      FAIL_EXIT1 ("invalid rq_proc value: %lu", request->rq_proc);
    }
}

static void
kill_server (void)
{
  if (server_pid > 0)
    kill (server_pid, SIGTERM);
}

static struct test_response
test_call (CLIENT *clnt, struct test_query query, struct timeval timeout)
{
  struct test_response response;
  TEST_COMPARE (clnt_call (clnt, PROC_ADD,
                           xdr_test_query, (void *) &query,
                           xdr_test_response, (void *) &response,
                           timeout),
                RPC_SUCCESS);
  return response;
}

static void
test_call_timeout (CLIENT *clnt, struct test_query query,
                   struct timeval timeout)
{
  struct test_response response;
  TEST_COMPARE (clnt_call (clnt, PROC_ADD,
                           xdr_test_query, (void *) &query,
                           xdr_test_response, (void *) &response,
                           timeout),
                RPC_TIMEDOUT);
}

static void
test_call_flush (CLIENT *clnt)
{
  TEST_COMPARE (clnt_call (clnt, PROC_RESET_SEQ,
                           (xdrproc_t) xdr_void_glibc_2_2_5, NULL,
                           (xdrproc_t) xdr_void_glibc_2_2_5, NULL,
                           ((struct timeval) { 5, 0 })),
                RPC_SUCCESS);
}

static double
get_ticks (void)
{
  struct timespec ts;
  if (clock_gettime (CLOCK_MONOTONIC, &ts) == 0)
    return ts.tv_sec + ts.tv_nsec * 1e-9;

  struct timeval tv;
  TEST_COMPARE (gettimeofday (&tv, NULL), 0);
  return tv.tv_sec + tv.tv_usec * 1e-6;
}

static void
check_runtime (const char *label, double seconds, double lower, double upper)
{
  if (test_verbose)
    printf ("info: %s took %f seconds\n", label, seconds);
  TEST_VERIFY (lower <= seconds);
  TEST_VERIFY (seconds < upper);
}

static void
test_udp_deadlines (int port)
{
  struct sockaddr_in sin =
    {
      .sin_family = AF_INET,
      .sin_addr.s_addr = htonl (INADDR_LOOPBACK),
      .sin_port = htons (port),
    };
  int sock = RPC_ANYSOCK;
  CLIENT *clnt = clntudp_create_glibc_2_2_5
    (&sin, PROGNUM, VERSNUM, (struct timeval) { 1, 500 * 1000 }, &sock);
  TEST_VERIFY_EXIT (clnt != NULL);

  double before = get_ticks ();
  struct test_response response = test_call
    (clnt,
     (struct test_query) {
       .a = 19, .b = 4, .timeout_ms = 500, .garbage_packets = 21,
     },
     (struct timeval) { 3, 0 });
  double after = get_ticks ();
  TEST_COMPARE (response.sum, (uint32_t) 23);
  TEST_COMPARE (response.seq, (uint32_t) 1);
  check_runtime ("garbage packets with eventual response", after - before,
                 0.45, 1.2);
  test_call_flush (clnt);

  before = get_ticks ();
  response = test_call
    (clnt,
     (struct test_query) {
       .a = 170, .b = 40, .wait_for_seq = 2,
     },
     (struct timeval) { 3, 0 });
  after = get_ticks ();
  TEST_COMPARE (response.sum, (uint32_t) 210);
  TEST_COMPARE (response.seq, (uint32_t) 2);
  check_runtime ("one missed response before retry", after - before,
                 1.45, 2.9);
  test_call_flush (clnt);

  before = get_ticks ();
  test_call_timeout
    (clnt,
     (struct test_query) {
       .a = 170, .b = 41, .wait_for_seq = 2,
     },
     (struct timeval) { 0, 750 * 1000 });
  after = get_ticks ();
  check_runtime ("overall timeout beats retry timeout", after - before,
                 0.70, 1.4);
  test_call_flush (clnt);

  before = get_ticks ();
  test_call_timeout
    (clnt,
     (struct test_query) {
       .a = 170, .b = 42, .timeout_ms = 1200, .garbage_packets = 21,
     },
     (struct timeval) { 0, 750 * 1000 });
  after = get_ticks ();
  check_runtime ("garbage packets do not extend the total timeout",
                 after - before, 0.70, 1.4);
  test_call_flush (clnt);

  TEST_COMPARE (clnt_call (clnt, PROC_EXIT,
                           (xdrproc_t) xdr_void_glibc_2_2_5, NULL,
                           (xdrproc_t) xdr_void_glibc_2_2_5, NULL,
                           ((struct timeval) { 5, 0 })),
                RPC_SUCCESS);
  clnt_destroy (clnt);
}

static int
do_test (void)
{
  SVCXPRT *transport = svcudp_create_glibc_2_2_5 (RPC_ANYSOCK);
  TEST_VERIFY_EXIT (transport != NULL);
  TEST_VERIFY_EXIT (svc_register_glibc_2_2_5
                    (transport, PROGNUM, VERSNUM, server_dispatch, 0));

  server_pid = xfork ();
  if (server_pid == 0)
    {
      svc_run_glibc_2_2_5 ();
      FAIL_EXIT1 ("svc_run returned unexpectedly");
    }
  atexit (kill_server);

  test_udp_deadlines (transport->xp_port);

  int status;
  xwaitpid (server_pid, &status, 0);
  server_pid = 0;
  TEST_VERIFY (WIFEXITED (status) && WEXITSTATUS (status) == EXIT_MARKER);

  SVC_DESTROY (transport);
  return 0;
}

#define TIMEOUT 20
#include <support/test-driver.c>
