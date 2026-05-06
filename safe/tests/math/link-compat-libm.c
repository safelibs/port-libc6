#include <math.h>
#include <stdio.h>

static volatile double angle_a = 0.5;
static volatile double angle_b = 0.25;
static volatile double base = 2.0;
static volatile double exponent = 3.0;

int main(void) {
    double (*cos_fn)(double) = cos;
    double (*sin_fn)(double) = sin;
    double (*pow_fn)(double, double) = pow;
    double value = cos_fn(angle_a) + sin_fn(angle_b) + pow_fn(base, exponent);
    if (!(value > 8.0 && value < 10.0)) {
        fprintf(stderr, "unexpected libm result\n");
        return 1;
    }
    return 0;
}
