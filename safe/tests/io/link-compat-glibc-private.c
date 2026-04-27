extern void *_dl_find_dso_for_object(void *);
__asm__(".symver _dl_find_dso_for_object,_dl_find_dso_for_object@GLIBC_PRIVATE");

int main(void) {
    return _dl_find_dso_for_object((void *) main) == 0;
}
