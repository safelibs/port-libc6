#include <stdio.h>
#include <time.h>

int main(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
        perror("clock_gettime");
        return 1;
    }
    return 0;
}
