#include <locale.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    locale_t locale = newlocale(LC_ALL_MASK, "C", (locale_t)0);
    const char *active = setlocale(LC_ALL, "C");
    if (locale == (locale_t)0 || active == NULL) {
        fputs("error: locale setup failed\n", stderr);
        return 1;
    }

    uselocale(locale);
    printf("%s %s\n", active, localeconv()->decimal_point);
    freelocale(locale);
    return 0;
}
