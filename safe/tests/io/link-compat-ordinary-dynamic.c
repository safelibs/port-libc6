#include <dlfcn.h>
#include <pthread.h>

int main(void) {
    return (dlopen(0, RTLD_NOW) == 0) || (pthread_self() == 0);
}
