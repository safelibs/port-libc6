#include <math.h>
#include <stdio.h>

int main(void) {
    double value = cos(0.5) + sin(0.25) + pow(2.0, 3.0);
    if (!(value > 8.0 && value < 10.0)) {
        fprintf(stderr, "unexpected libm result\n");
        return 1;
    }
    return 0;
}
