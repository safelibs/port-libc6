#include <dlfcn.h>
#include <stdio.h>

int main(void) {
    void *handle = dlopen("libc.so.6", RTLD_NOW | RTLD_LOCAL);
    if (handle == NULL) {
        fprintf(stderr, "dlopen failed: %s\n", dlerror());
        return 1;
    }
    void *symbol = dlsym(handle, "printf");
    if (symbol == NULL) {
        fprintf(stderr, "dlsym failed: %s\n", dlerror());
        dlclose(handle);
        return 1;
    }
    dlclose(handle);
    return 0;
}
