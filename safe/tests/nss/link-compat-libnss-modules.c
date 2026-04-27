#define _GNU_SOURCE
#include <dlfcn.h>

int
main (void)
{
  static const char *const modules[] =
    {
      "libnss_dns.so.2",
      "libnss_files.so.2",
      "libnss_compat.so.2",
      "libnss_hesiod.so.2",
    };

  for (unsigned int i = 0; i < sizeof (modules) / sizeof (modules[0]); ++i)
    {
      void *handle = dlopen (modules[i], RTLD_NOW | RTLD_LOCAL);
      if (handle == 0)
        return 1;
      dlclose (handle);
    }

  return 0;
}
