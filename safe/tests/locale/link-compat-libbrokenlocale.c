#include <stdio.h>
#include <stdlib.h>

extern size_t __ctype_get_mb_cur_max(void);

int main(void) {
    size_t width = __ctype_get_mb_cur_max();
    printf("%zu\n", width);
    return width == 0;
}
