struct timespec
{
  long tv_sec;
  long tv_nsec;
};

extern int gai_suspend (const void *const [], int, const struct timespec *);

int
main (void)
{
  (void) gai_suspend (0, 0, 0);
  return 0;
}
