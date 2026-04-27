#include <iconv.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    iconv_t cd = iconv_open("UTF-8", "UTF-8");
    char input[] = "compat";
    char output[32];
    char *inbuf = input;
    char *outbuf = output;
    size_t inbytes = strlen(input);
    size_t outbytes = sizeof(output);

    if (cd == (iconv_t)-1) {
        fputs("error: iconv_open failed\n", stderr);
        return 1;
    }

    memset(output, 0, sizeof(output));
    if (iconv(cd, &inbuf, &inbytes, &outbuf, &outbytes) == (size_t)-1) {
        fputs("error: iconv failed\n", stderr);
        iconv_close(cd);
        return 1;
    }

    iconv_close(cd);
    puts(output);
    return 0;
}
