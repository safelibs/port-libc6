#include <fnmatch.h>
#include <glob.h>
#include <regex.h>
#include <stdio.h>
#include <string.h>
#include <wordexp.h>

int main(void) {
    regex_t regex;
    glob_t glob_state;
    wordexp_t words;

    if (regcomp(&regex, "^link_[0-9]+$", REG_EXTENDED) != 0) {
        fputs("error: regcomp failed\n", stderr);
        return 1;
    }
    if (regexec(&regex, "link_08", 0, NULL, 0) != 0) {
        fputs("error: regexec failed\n", stderr);
        regfree(&regex);
        return 1;
    }
    regfree(&regex);

    if (fnmatch("link_*", "link_compat", 0) != 0) {
        fputs("error: fnmatch failed\n", stderr);
        return 1;
    }

    memset(&glob_state, 0, sizeof(glob_state));
    if (glob("/bin/sh", 0, NULL, &glob_state) != 0 || glob_state.gl_pathc == 0) {
        fputs("error: glob failed\n", stderr);
        globfree(&glob_state);
        return 1;
    }
    globfree(&glob_state);

    memset(&words, 0, sizeof(words));
    if (wordexp("link_compat", &words, WRDE_NOCMD) != 0 || words.we_wordc != 1) {
        fputs("error: wordexp failed\n", stderr);
        wordfree(&words);
        return 1;
    }
    puts(words.we_wordv[0]);
    wordfree(&words);
    return 0;
}
